//! Base64, URL, HTML and string-escape encoders share one view.

use gpui_kit::component::input::TextareaState;
use gpui_kit::{
    Context, Entity, IntoElement, ParentElement, Render, SharedString, Styled, Subscription,
    Window, div, prelude::FluentBuilder, px,
};

use super::*;
use crate::logic::{self, plural};
use crate::registry::ToolId;
use crate::ui::{self, Tone};

struct Spec {
    labels: [&'static str; 2],
    hint: &'static str,
    sample: &'static str,
    enc: fn(&str) -> Result<String, ()>,
    dec: fn(&str) -> Result<String, ()>,
}

fn spec(id: ToolId) -> Spec {
    match id {
        ToolId::Base64 => Spec {
            labels: ["Encode", "Decode"],
            hint: "Text ↔ Base64 using UTF-8",
            sample: "Hello, SideKit! 👋",
            enc: |s| Ok(logic::b64_enc(s)),
            dec: logic::b64_dec,
        },
        ToolId::Url => Spec {
            labels: ["Encode", "Decode"],
            hint: "Percent-encoding for query strings and paths",
            sample: "https://example.com/search?q=dev toys&lang=en",
            enc: |s| Ok(logic::url_enc(s)),
            dec: logic::url_dec,
        },
        ToolId::Html => Spec {
            labels: ["Encode", "Decode"],
            hint: "Named and numeric HTML character references",
            sample: "<a href=\"/docs\">Read the \"docs\" & more</a>",
            enc: |s| Ok(logic::html_enc(s)),
            dec: |s| Ok(logic::html_dec(s)),
        },
        _ => Spec {
            labels: ["Escape", "Unescape"],
            hint: "Backslash escapes for newlines, tabs and quotes",
            sample: "Line one\nIndented\tcolumn with \"quotes\"",
            enc: |s| Ok(logic::esc_enc(s)),
            dec: logic::esc_dec,
        },
    }
}

pub struct CodecView {
    spec: Spec,
    input: Entity<TextareaState>,
    output: Entity<TextareaState>,
    decode: bool,
    out: SharedString,
    err: Option<&'static str>,
    _subs: Vec<Subscription>,
}

impl CodecView {
    pub fn new(id: ToolId, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let spec = spec(id);
        let input = editor(spec.sample, "Type or paste text", window, cx);
        let output = editor("", "", window, cx);
        for s in [&input, &output] {
            s.update(cx, |s, cx| s.set_soft_wrap(true, window, cx));
        }
        let subs = vec![
            watch(&input, window, cx, Self::recompute),
            watch(&output, window, cx, |_, _, _| {}),
        ];
        let mut this = Self { spec, input, output, decode: false, out: SharedString::default(), err: None, _subs: subs };
        this.recompute(window, cx);
        this
    }

    fn recompute(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let input = text_of(&self.input, cx);
        let (out, err) = if input.is_empty() {
            (String::new(), None)
        } else {
            let f = if self.decode { self.spec.dec } else { self.spec.enc };
            match f(&input) {
                Ok(o) => (o, None),
                Err(()) => (
                    String::new(),
                    Some(if self.decode { "This input is not valid for decoding." } else { "Could not encode this input." }),
                ),
            }
        };
        self.out = out.into();
        self.err = err;
        set_text(&self.output, &self.out, window, cx);
        cx.notify();
    }

    /// Load clipboard content; encoded input implies decoding.
    pub fn set_input(&mut self, text: &str, decode: bool, window: &mut Window, cx: &mut Context<Self>) {
        set_text(&self.input, text, window, cx);
        self.decode = decode;
        self.recompute(window, cx);
    }

    /// "Use output as input": feed the result back and flip direction.
    fn swap(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.err.is_some() {
            return;
        }
        let out = self.out.to_string();
        set_text(&self.input, &out, window, cx);
        self.decode = !self.decode;
        self.recompute(window, cx);
    }
}

impl Render for CodecView {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let pal = Pal::get(cx);
        let input = text_of(&self.input, cx);
        let in_stat = format!("{} · {}", plural(input.encode_utf16().count(), "character"), plural(input.len(), "byte"));
        let out_stat = if self.err.is_some() {
            "Error".to_string()
        } else {
            format!("{} · {}", plural(self.out.encode_utf16().count(), "character"), plural(self.out.len(), "byte"))
        };
        let tone = if self.err.is_some() { Some(Tone::Err) } else if !self.out.is_empty() { Some(Tone::Ok) } else { None };
        let on_mode = on_index(cx, |this: &mut Self, i, w, cx| {
            this.decode = i == 1;
            this.recompute(w, cx);
        });
        let [paste, clear] = paste_clear("codec", &self.input, &pal, cx, Self::recompute);

        div()
            .flex()
            .flex_col()
            .gap(px(6.))
            .flex_1()
            .child(ui::section_label("Configuration", &pal))
            .child(ui::setting(
                "codec-mode",
                ui::setting_icon("conv", &pal),
                "Conversion",
                Some(self.spec.hint.into()),
                ui::seg("codec-seg", &self.spec.labels, self.decode as usize, &pal, on_mode),
                &pal,
            ))
            .child(
                div()
                    .flex()
                    .gap(px(12.))
                    .flex_1()
                    .min_h(px(340.))
                    .mt(px(8.))
                    .child(
                        ui::pane(is_focused(&self.input, window, cx), &pal)
                            .flex_1()
                            .child(ui::pane_head("Input", None, &pal).child(paste).child(clear))
                            .child(editor_el(&self.input, false, cx))
                            .child(ui::pane_foot(&pal).child(in_stat)),
                    )
                    .child(
                        ui::pane(is_focused(&self.output, window, cx), &pal)
                            .flex_1()
                            .child(
                                ui::pane_head("Output", None, &pal)
                                    .child(ui::icon_btn("codec-swap", "swap", &pal, cx.listener(|this, _, w, cx| this.swap(w, cx))))
                                    .child(ui::copy_btn("codec-copy", self.out.clone(), &pal, window, cx)),
                            )
                            .when_some(self.err, |d, e| d.child(ui::err_box(e, &pal).m(px(12.))))
                            .child(editor_el(&self.output, true, cx))
                            .child(ui::pane_foot(&pal).child(ui::dot(tone, &pal)).child(out_stat)),
                    ),
            )
    }
}
