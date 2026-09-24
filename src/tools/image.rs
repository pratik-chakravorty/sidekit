//! Tools that show a picture: Base64 images and QR codes.

use std::path::PathBuf;
use std::sync::Arc;

use gpui_kit::component::input::{EditorState, TextareaState};
use gpui_kit::{
    AnyElement, App, Context, Entity, ExternalPaths, Image, ImageFormat, InteractiveElement,
    IntoElement, ObjectFit, ParentElement, PathPromptOptions, Render, SharedString,
    Styled, StyledImage, Subscription, Window, div, img, prelude::FluentBuilder, px,
};

use super::*;
use crate::logic::media::{self, ImageInfo};
use crate::logic::{plural, thousands};
use crate::ui::{self, BtnKind, Tone};

fn format_of(ext: &str) -> ImageFormat {
    match ext {
        "png" => ImageFormat::Png,
        "jpg" => ImageFormat::Jpeg,
        "gif" => ImageFormat::Gif,
        "webp" => ImageFormat::Webp,
        "bmp" => ImageFormat::Bmp,
        "tiff" => ImageFormat::Tiff,
        "ico" => ImageFormat::Ico,
        _ => ImageFormat::Svg,
    }
}

/// A picture centred on a checkerboard-free, neutral stage.
fn stage(image: Option<Arc<Image>>, empty: &'static str, height: f32, light: bool, cx: &App) -> AnyElement {
    let p = Pal::get(cx);
    div()
        .flex()
        .items_center()
        .justify_center()
        .h(px(height))
        .p(px(16.))
        .rounded(px(8.))
        .border_1()
        .border_color(p.stroke)
        .bg(if light { gpui_kit::hsla(0., 0., 1., 1.) } else { p.editor })
        .map(|d| match image {
            Some(i) => d.child(img(i).size_full().object_fit(ObjectFit::Contain)),
            None => d.child(div().text_size(px(13.)).text_color(p.text3).child(empty)),
        })
        .into_any_element()
}

fn save_bytes<V: 'static>(bytes: Vec<u8>, name: String, window: &mut Window, cx: &mut Context<V>, done: fn(&mut V, Result<PathBuf, String>, &mut Context<V>)) {
    let dir = crate::library::home_dir().unwrap_or_default();
    let rx = cx.prompt_for_new_path(&dir, Some(&name));
    cx.spawn_in(window, async move |this, cx| {
        if let Ok(Ok(Some(path))) = rx.await {
            let r = std::fs::write(&path, bytes).map(|_| path).map_err(|e| e.to_string());
            let _ = this.update(cx, |v, cx| done(v, r, cx));
        }
    })
    .detach();
}

// ------------------------------------------------------------ Base64 image

pub struct B64ImageView {
    encode: bool,
    input: Entity<TextareaState>,
    output: Entity<EditorState>,
    decoded: Option<(ImageInfo, Arc<Image>)>,
    err: Option<String>,
    /// Encode mode: the file that was picked.
    source: Option<String>,
    note: Option<String>,
    wrap: bool,
    _subs: Vec<Subscription>,
}

const SAMPLE_PNG: &str = "data:image/png;base64,iVBORw0KGgoAAAANSUhEUgAAACAAAAAgCAYAAABzenr0AAAATElEQVR42u3WsQ0AIAgAQTZ1TbfTCYhoAIn+JfbfCIgA0LQ+XB8BngGalICV0ACrkIBdrgGnCCDgnYDr37DEICoxikssI+6BvADgFxN3uWrFv/PlEwAAAABJRU5ErkJggg==";

impl B64ImageView {
    pub fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let input = editor(SAMPLE_PNG, "Paste Base64 or a data:image/… URI", window, cx);
        input.update(cx, |s, cx| s.set_soft_wrap(true, window, cx));
        let output = code_editor("", "", "plaintext", window, cx);
        output.update(cx, |s, cx| s.set_soft_wrap(true, window, cx));
        let subs = vec![watch(&input, window, cx, |this: &mut Self, _, cx| this.decode(cx)), watch(&output, window, cx, |_, _, _| {})];
        let mut this = Self { encode: false, input, output, decoded: None, err: None, source: None, note: None, wrap: true, _subs: subs };
        this.decode(cx);
        this
    }

    fn decode(&mut self, cx: &mut Context<Self>) {
        let src = text_of(&self.input, cx);
        self.note = None;
        if src.trim().is_empty() {
            (self.decoded, self.err) = (None, None);
        } else {
            match media::decode_image(&src) {
                Ok(info) => {
                    let image = Arc::new(Image::from_bytes(format_of(info.ext), info.bytes.clone()));
                    (self.decoded, self.err) = (Some((info, image)), None);
                }
                Err(e) => (self.decoded, self.err) = (None, Some(e)),
            }
        }
        cx.notify();
    }

    pub fn set_input(&mut self, text: &str, window: &mut Window, cx: &mut Context<Self>) {
        self.encode = false;
        set_text(&self.input, text, window, cx);
        self.decode(cx);
    }

    fn load_file(&mut self, path: PathBuf, window: &mut Window, cx: &mut Context<Self>) {
        self.encode = true;
        match std::fs::read(&path).map_err(|e| e.to_string()).and_then(|b| media::data_uri(&b)) {
            Ok(uri) => {
                self.source = path.file_name().map(|n| n.to_string_lossy().into_owned());
                set_text(&self.output, &uri, window, cx);
                self.decoded = media::decode_image(&uri).ok().map(|info| {
                    let image = Arc::new(Image::from_bytes(format_of(info.ext), info.bytes.clone()));
                    (info, image)
                });
                self.err = None;
            }
            Err(e) => self.err = Some(e),
        }
        cx.notify();
    }

    fn pick(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let rx = cx.prompt_for_paths(PathPromptOptions { files: true, directories: false, multiple: false, prompt: Some("Encode".into()) });
        cx.spawn_in(window, async move |this, cx| {
            if let Ok(Ok(Some(mut paths))) = rx.await {
                if let Some(p) = paths.pop() {
                    let _ = this.update_in(cx, |v, w, cx| v.load_file(p, w, cx));
                }
            }
        })
        .detach();
    }

    fn set_mode(&mut self, encode: bool, window: &mut Window, cx: &mut Context<Self>) {
        self.encode = encode;
        self.err = None;
        if encode {
            self.decoded = None;
            if let Some(uri) = Some(text_of(&self.output, cx)).filter(|s| !s.is_empty()) {
                self.decoded = media::decode_image(&uri).ok().map(|info| {
                    let image = Arc::new(Image::from_bytes(format_of(info.ext), info.bytes.clone()));
                    (info, image)
                });
            }
        } else {
            self.decode(cx);
        }
        let _ = window;
        cx.notify();
    }
}

impl Render for B64ImageView {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let pal = Pal::get(cx);
        let on_mode = on_index(cx, |this: &mut Self, i, w, cx| this.set_mode(i == 1, w, cx));
        let info_rows = self.decoded.as_ref().map(|(info, _)| {
            let mut rows = vec![("Type", info.mime.to_string())];
            if let Some((w, h)) = info.size {
                rows.push(("Dimensions", format!("{w} × {h} px")));
            }
            rows.push(("Size", format!("{} bytes", thousands(info.bytes.len() as u64))));
            rows
        });
        let image = self.decoded.as_ref().map(|(_, i)| i.clone());

        let left: AnyElement = if self.encode {
            let uri: SharedString = text_of(&self.output, cx).into();
            let raw: SharedString = uri.split_once(',').map(|(_, b)| b.to_string()).unwrap_or_default().into();
            div()
                .flex()
                .flex_col()
                .gap(px(8.))
                .flex_1()
                .child(
                    div()
                        .id("b64-drop")
                        .flex()
                        .items_center()
                        .gap(px(12.))
                        .p(px(14.))
                        .rounded(px(8.))
                        .border_1()
                        .border_dashed()
                        .border_color(pal.stroke_strong)
                        .drag_over::<ExternalPaths>(move |s, _, _, _| s.bg(pal.accent_soft))
                        .on_drop(cx.listener(|this, paths: &ExternalPaths, w, cx| {
                            if let Some(p) = paths.paths().first().cloned() {
                                this.load_file(p, w, cx);
                            }
                        }))
                        .child(ui::btn("b64-pick", Some("folder"), "Choose image…", BtnKind::Accent, &pal, cx.listener(|this, _, w, cx| this.pick(w, cx))))
                        .child(div().text_size(px(13.)).text_color(pal.text3).child(match &self.source {
                            Some(n) => n.clone(),
                            None => "or drop an image file here".into(),
                        })),
                )
                .child(
                    ui::pane(is_focused(&self.output, window, cx), &pal)
                        .flex_1()
                        .min_h(px(260.))
                        .child(
                            ui::pane_head("Data URI", None, &pal)
                                .child(ui::copy_btn_labeled("b64-copy-raw", "Base64 only", raw, &pal, window, cx))
                                .child(ui::copy_btn("b64-copy", uri, &pal, window, cx)),
                        )
                        .child(code_editor_el(&self.output, true, cx)),
                )
                .into_any_element()
        } else {
            sync_wrap(&[&self.input], &mut self.wrap, window, cx);
            let [paste, clear] = paste_clear("b64i", &self.input, &pal, cx, |this: &mut Self, _, cx| this.decode(cx));
            ui::pane(is_focused(&self.input, window, cx), &pal)
                .flex_1()
                .min_h(px(320.))
                .child(ui::pane_head("Base64 or data URI", None, &pal).child(paste).child(clear))
                .child(editor_el(&self.input, false, cx))
                .into_any_element()
        };

        let right = div()
            .flex()
            .flex_col()
            .gap(px(8.))
            .w(px(360.))
            .flex_none()
            .child(stage(image, if self.encode { "Choose an image to encode" } else { "The decoded image appears here" }, 280., false, cx))
            .when_some(self.err.clone(), |d, e| d.child(ui::err_box(e, &pal)))
            .when_some(info_rows, |d, rows| {
                let n = rows.len();
                let mut card = ui::kv_card(&pal);
                for (i, (k, v)) in rows.into_iter().enumerate() {
                    card = card.child(ui::kv_row(("b64-i", i), i + 1 == n, &pal).child(ui::kv_label(k, 100., &pal)).child(ui::kv_val(v, false)));
                }
                d.child(card)
            })
            .when(!self.encode && self.decoded.is_some(), |d| {
                d.child(ui::btn("b64-save", Some("install"), "Save image…", BtnKind::Normal, &pal, cx.listener(|this, _, w, cx| {
                    if let Some((info, _)) = &this.decoded {
                        let name = format!("image.{}", info.ext);
                        save_bytes(info.bytes.clone(), name, w, cx, |v: &mut Self, r, cx| {
                            v.note = Some(match r {
                                Ok(p) => format!("Saved to {}", p.display()),
                                Err(e) => format!("Could not save: {e}"),
                            });
                            cx.notify();
                        });
                    }
                })))
            })
            .when_some(self.note.clone(), |d, n| d.child(div().text_size(px(12.)).text_color(pal.text3).child(n)));

        div()
            .flex()
            .flex_col()
            .gap(px(6.))
            .flex_1()
            .child(ui::section_label("Configuration", &pal))
            .child(ui::setting(
                "b64i-mode",
                ui::setting_icon("conv", &pal),
                "Conversion",
                Some("Base64 → image, or an image file → Base64".into()),
                ui::seg("b64i-seg", &["Decode", "Encode"], self.encode as usize, &pal, on_mode),
                &pal,
            ))
            .child(div().flex().gap(px(16.)).mt(px(8.)).flex_1().child(left).child(right))
    }
}

// ------------------------------------------------------------ QR code

pub struct QrView {
    input: Entity<TextareaState>,
    level: usize,
    svg: Option<(String, usize, Arc<Image>)>,
    err: Option<String>,
    note: Option<String>,
    wrap: bool,
    _subs: Vec<Subscription>,
}

impl QrView {
    pub fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let input = editor("https://github.com/pratik-chakravorty/sidekit", "Text or URL to encode", window, cx);
        input.update(cx, |s, cx| s.set_soft_wrap(true, window, cx));
        let subs = vec![watch(&input, window, cx, |this: &mut Self, _, cx| this.generate(cx))];
        let mut this = Self { input, level: 1, svg: None, err: None, note: None, wrap: true, _subs: subs };
        this.generate(cx);
        this
    }

    fn generate(&mut self, cx: &mut Context<Self>) {
        let text = text_of(&self.input, cx);
        self.note = None;
        if text.is_empty() {
            (self.svg, self.err) = (None, None);
        } else {
            match media::qr_svg(&text, self.level) {
                Ok((svg, modules)) => {
                    let image = Arc::new(Image::from_bytes(ImageFormat::Svg, svg.clone().into_bytes()));
                    (self.svg, self.err) = (Some((svg, modules, image)), None);
                }
                Err(e) => (self.svg, self.err) = (None, Some(e)),
            }
        }
        cx.notify();
    }
}

impl Render for QrView {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let pal = Pal::get(cx);
        sync_wrap(&[&self.input], &mut self.wrap, window, cx);
        let text = text_of(&self.input, cx);
        let [paste, clear] = paste_clear("qr", &self.input, &pal, cx, |this: &mut Self, _, cx| this.generate(cx));
        let on_level = on_index(cx, |this: &mut Self, i, _, cx| {
            this.level = i;
            this.generate(cx);
        });
        let image = self.svg.as_ref().map(|s| s.2.clone());
        let svg_text: SharedString = self.svg.as_ref().map(|s| s.0.clone()).unwrap_or_default().into();

        div()
            .flex()
            .flex_col()
            .gap(px(6.))
            .child(ui::section_label("Configuration", &pal))
            .child(ui::setting(
                "qr-level",
                ui::setting_icon("shield", &pal),
                "Error correction",
                Some("Higher levels survive more damage but hold less text".into()),
                ui::dropdown("qr-level-dd", media::QR_LEVELS, self.level, &pal, window, cx, on_level),
                &pal,
            ))
            .child(
                div()
                    .flex()
                    .gap(px(16.))
                    .mt(px(8.))
                    .child(
                        ui::pane(is_focused(&self.input, window, cx), &pal)
                            .flex_1()
                            .h(px(320.))
                            .child(ui::pane_head("Content", None, &pal).child(paste).child(clear))
                            .child(editor_el(&self.input, false, cx))
                            .child(ui::pane_foot(&pal).child(format!("{} · {}", plural(text.chars().count(), "character"), plural(text.len(), "byte")))),
                    )
                    .child(
                        div()
                            .flex()
                            .flex_col()
                            .gap(px(8.))
                            .w(px(320.))
                            .flex_none()
                            .child(stage(image, "Type something to encode", 320., true, cx))
                            .when_some(self.err.clone(), |d, e| d.child(ui::err_box(e, &pal)))
                            .when_some(self.svg.as_ref().map(|s| s.1), |d, m| {
                                d.child(div().flex().child(ui::badge(format!("{m} × {m} modules"), Tone::Neutral, &pal)))
                                .child(
                                    div()
                                        .flex()
                                        .items_center()
                                        .gap(px(8.))
                                        .child(ui::copy_btn_labeled("qr-copy", "Copy SVG", svg_text.clone(), &pal, window, cx))
                                        .child(ui::btn("qr-save", Some("install"), "Save…", BtnKind::Normal, &pal, cx.listener(|this, _, w, cx| {
                                            if let Some((svg, _, _)) = &this.svg {
                                                save_bytes(svg.clone().into_bytes(), "qr-code.svg".into(), w, cx, |v: &mut Self, r, cx| {
                                                    v.note = Some(match r {
                                                        Ok(p) => format!("Saved to {}", p.display()),
                                                        Err(e) => format!("Could not save: {e}"),
                                                    });
                                                    cx.notify();
                                                });
                                            }
                                        }))),
                                )
                            })
                            .when_some(self.note.clone(), |d, n| d.child(div().text_size(px(12.)).text_color(pal.text3).child(n))),
                    ),
            )
    }
}

