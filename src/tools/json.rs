use gpui_kit::component::input::TextareaState;
use gpui_kit::{
    Context, Entity, IntoElement, ParentElement, Render, SharedString, Styled, Subscription,
    Window, div, prelude::FluentBuilder, px,
};

use super::*;
use crate::logic::{self, Indent, plural};
use crate::ui::{self, Tone};

/// Output state shared by the two JSON tools.
#[derive(Default)]
struct Out {
    text: SharedString,
    err: Option<String>,
    status: String,
    tone: Option<Tone>,
}

fn panes<V: 'static>(
    id: &'static str,
    (in_title, out_title): (&'static str, &'static str),
    input: &Entity<TextareaState>,
    output: &Entity<TextareaState>,
    out: &Out,
    window: &mut Window,
    cx: &mut Context<V>,
    recompute: impl Fn(&mut V, &mut Window, &mut Context<V>) + Clone + 'static,
) -> gpui_kit::AnyElement {
    let pal = Pal::get(cx);
    let [paste, clear] = paste_clear(id, input, &pal, cx, recompute);
    div()
        .flex()
        .gap(px(12.))
        .flex_1()
        .min_h(px(340.))
        .mt(px(8.))
        .child(
            ui::pane(is_focused(input, window, cx), &pal)
                .flex_1()
                .child(ui::pane_head(in_title, None, &pal).child(paste).child(clear))
                .child(editor_el(input, false, cx)),
        )
        .child(
            ui::pane(is_focused(output, window, cx), &pal)
                .flex_1()
                .child(ui::pane_head(out_title, None, &pal).child(ui::copy_btn(
                    crate::id!("{id}-copy"),
                    out.text.clone(),
                    &pal,
                    window,
                    cx,
                )))
                .when_some(out.err.clone(), |d, e| d.child(ui::err_box(e, &pal).m(px(12.))))
                .child(editor_el(output, true, cx))
                .child(ui::pane_foot(&pal).child(ui::dot(out.tone, &pal)).child(out.status.clone())),
        )
        .into_any_element()
}

// ------------------------------------------------------------ JSON formatter

const INDENTS: &[&str] = &["2 spaces", "4 spaces", "1 tab", "Minified"];

pub struct JsonFmtView {
    input: Entity<TextareaState>,
    output: Entity<TextareaState>,
    indent: usize,
    sort: bool,
    out: Out,
    wrap: bool,
    _subs: Vec<Subscription>,
}

impl JsonFmtView {
    pub fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let input = editor(
            r#"{"name":"SideKit","version":2,"offline":true,"tags":["developer","utilities","open-source"],"platforms":{"windows":true,"macos":true,"linux":true}}"#,
            "Paste or type JSON",
            window,
            cx,
        );
        let output = editor("", "", window, cx);
        let subs = vec![
            watch(&input, window, cx, Self::recompute),
            watch(&output, window, cx, |_, _, _| {}),
        ];
        let mut this = Self { input, output, indent: 0, sort: false, out: Out::default(), wrap: false, _subs: subs };
        this.recompute(window, cx);
        this
    }

    pub fn set_input(&mut self, text: &str, window: &mut Window, cx: &mut Context<Self>) {
        set_text(&self.input, text, window, cx);
        self.recompute(window, cx);
    }

    fn recompute(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let src = text_of(&self.input, cx);
        self.out = if src.trim().is_empty() {
            Out { status: "Waiting for input".into(), ..Default::default() }
        } else {
            let indent = match self.indent {
                0 => Indent::Spaces(2),
                1 => Indent::Spaces(4),
                2 => Indent::Tab,
                _ => Indent::Minified,
            };
            match logic::format_json(&src, indent, self.sort) {
                Ok(r) => Out { text: r.out.into(), err: None, status: r.status, tone: Some(Tone::Ok) },
                Err(e) => Out { err: Some(e), status: "Invalid JSON".into(), tone: Some(Tone::Err), ..Default::default() },
            }
        };
        set_text(&self.output, &self.out.text, window, cx);
        cx.notify();
    }
}

impl Render for JsonFmtView {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let pal = Pal::get(cx);
        sync_wrap(&[&self.input, &self.output], &mut self.wrap, window, cx);
        let on_indent = on_index(cx, |this: &mut Self, i, w, cx| {
            this.indent = i;
            this.recompute(w, cx);
        });
        let sort = self.sort;
        div()
            .flex()
            .flex_col()
            .gap(px(6.))
            .flex_1()
            .child(ui::section_label("Configuration", &pal))
            .child(ui::setting(
                "indent",
                ui::setting_icon("indent", &pal),
                "Indentation",
                None,
                ui::dropdown("indent-dd", INDENTS, self.indent, &pal, window, cx, on_indent),
                &pal,
            ))
            .child(ui::setting(
                "sort",
                ui::setting_icon("sort", &pal),
                "Sort JSON properties alphabetically",
                None,
                ui::toggle_labeled("sort-tg", sort, &pal, cx.listener(|this, _, w, cx| {
                    this.sort = !this.sort;
                    this.recompute(w, cx);
                })),
                &pal,
            ))
            .child(panes("jsonfmt", ("Input", "Output"), &self.input, &self.output, &self.out, window, cx, Self::recompute))
    }
}

// ------------------------------------------------------------ JSON → YAML

const YAML_INDENTS: &[&str] = &["2 spaces", "4 spaces"];

pub struct JsonYamlView {
    input: Entity<TextareaState>,
    output: Entity<TextareaState>,
    indent: usize,
    out: Out,
    wrap: bool,
    _subs: Vec<Subscription>,
}

impl JsonYamlView {
    pub fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let input = editor(
            "{\n  \"service\": \"api-gateway\",\n  \"replicas\": 3,\n  \"ports\": [80, 443],\n  \"env\": { \"LOG_LEVEL\": \"info\", \"CACHE\": true },\n  \"routes\": [\n    { \"path\": \"/users\", \"timeout\": 30 },\n    { \"path\": \"/orders\", \"timeout\": 45 }\n  ]\n}",
            "Paste or type JSON",
            window,
            cx,
        );
        let output = editor("", "", window, cx);
        let subs = vec![
            watch(&input, window, cx, Self::recompute),
            watch(&output, window, cx, |_, _, _| {}),
        ];
        let mut this = Self { input, output, indent: 0, out: Out::default(), wrap: false, _subs: subs };
        this.recompute(window, cx);
        this
    }

    fn recompute(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let src = text_of(&self.input, cx);
        self.out = if src.trim().is_empty() {
            Out { status: "Waiting for input".into(), ..Default::default() }
        } else {
            match serde_json::from_str::<serde_json::Value>(&src) {
                Ok(v) => {
                    let y = logic::to_yaml(&v, 0, if self.indent == 0 { 2 } else { 4 });
                    let status = format!("Converted · {}", plural(y.split('\n').count(), "line"));
                    Out { text: y.into(), err: None, status, tone: Some(Tone::Ok) }
                }
                Err(e) => Out {
                    err: Some(logic::json_err(&e)),
                    status: "Invalid JSON".into(),
                    tone: Some(Tone::Err),
                    ..Default::default()
                },
            }
        };
        set_text(&self.output, &self.out.text, window, cx);
        cx.notify();
    }
}

impl Render for JsonYamlView {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let pal = Pal::get(cx);
        sync_wrap(&[&self.input, &self.output], &mut self.wrap, window, cx);
        let on_indent = on_index(cx, |this: &mut Self, i, w, cx| {
            this.indent = i;
            this.recompute(w, cx);
        });
        div()
            .flex()
            .flex_col()
            .gap(px(6.))
            .flex_1()
            .child(ui::section_label("Configuration", &pal))
            .child(ui::setting(
                "indent",
                ui::setting_icon("indent", &pal),
                "Indentation",
                Some("Spaces used for each nesting level in YAML".into()),
                ui::dropdown("yindent-dd", YAML_INDENTS, self.indent, &pal, window, cx, on_indent),
                &pal,
            ))
            .child(panes("jsonyaml", ("JSON", "YAML"), &self.input, &self.output, &self.out, window, cx, Self::recompute))
    }
}
