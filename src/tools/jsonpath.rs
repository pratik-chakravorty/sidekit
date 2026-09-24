//! Try JSONPath queries (RFC 9535) against a document.

use gpui_kit::component::input::{EditorState, InputState};
use gpui_kit::{
    Context, Entity, InteractiveElement, IntoElement, ParentElement, Render, SharedString,
    StatefulInteractiveElement, Styled, Subscription, Window, div, prelude::FluentBuilder, px,
};

use super::*;
use crate::logic::{self, Indent, plural};
use crate::theme::MONO_FONT;
use crate::ui::{self, Tone};

const EXAMPLES: &[&str] = &["$.store.book[*].title", "$..price", "$.store.book[?@.price < 10]", "$.store.book[-1]", "$..book[0,1].author"];

const SAMPLE: &str = r#"{
  "store": {
    "book": [
      { "category": "reference", "author": "Nigel Rees", "title": "Sayings of the Century", "price": 8.95 },
      { "category": "fiction", "author": "Evelyn Waugh", "title": "Sword of Honour", "price": 12.99 },
      { "category": "fiction", "author": "Herman Melville", "title": "Moby Dick", "isbn": "0-553-21311-3", "price": 8.99 }
    ],
    "bicycle": { "color": "red", "price": 399 }
  }
}"#;

pub struct JsonPathView {
    doc: Entity<EditorState>,
    path: Entity<InputState>,
    output: Entity<EditorState>,
    /// Show where each match was found rather than just its value.
    locations: bool,
    out: SharedString,
    status: (String, Option<Tone>),
    err: Option<String>,
    _subs: Vec<Subscription>,
}

impl JsonPathView {
    pub fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let doc = code_editor(SAMPLE, "Paste JSON", "json", window, cx);
        let path = line("$.store.book[?@.price < 10].title", "$.path.to[*].value", window, cx);
        let output = code_editor("", "", "json", window, cx);
        let subs = vec![watch(&doc, window, cx, Self::run), watch(&path, window, cx, Self::run), watch(&output, window, cx, |_, _, _| {})];
        let mut this = Self { doc, path, output, locations: false, out: SharedString::default(), status: (String::new(), None), err: None, _subs: subs };
        this.run(window, cx);
        this
    }

    fn run(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let doc = text_of(&self.doc, cx);
        let path = line_text(&self.path, cx);
        let (out, status, err) = if doc.trim().is_empty() || path.trim().is_empty() {
            (String::new(), ("Waiting for a document and a path".into(), None), None)
        } else {
            match logic::jsonpath(&doc, &path) {
                Ok((values, locs)) => {
                    let n = locs.len();
                    let v = if self.locations {
                        serde_json::Value::Array(locs.into_iter().map(serde_json::Value::String).collect())
                    } else {
                        values
                    };
                    (logic::to_json(&v, &Indent::Spaces(2)), (plural(n, "match"), Some(if n > 0 { Tone::Ok } else { Tone::Neutral })), None)
                }
                Err(e) => (String::new(), ("Error".into(), Some(Tone::Err)), Some(e)),
            }
        };
        self.out = out.into();
        self.status = status;
        self.err = err;
        set_text(&self.output, &self.out, window, cx);
        cx.notify();
    }
}

impl Render for JsonPathView {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let pal = Pal::get(cx);
        let [paste, clear] = paste_clear("jp", &self.doc, &pal, cx, Self::run);
        let on_show = on_index(cx, |this: &mut Self, i, w, cx| {
            this.locations = i == 1;
            this.run(w, cx);
        });
        let examples = EXAMPLES.iter().enumerate().map(|(i, ex)| {
            let ex = *ex;
            div()
                .id(("jp-ex", i))
                .px(px(10.))
                .h(px(26.))
                .flex()
                .items_center()
                .rounded(px(13.))
                .border_1()
                .border_color(pal.stroke)
                .cursor_pointer()
                .font_family(MONO_FONT)
                .text_size(px(12.))
                .text_color(pal.text2)
                .hover(move |s| s.bg(pal.subtle).text_color(pal.text))
                .child(ex)
                .on_click(cx.listener(move |this, _, w, cx| {
                    set_line(&this.path, ex, w, cx);
                    this.run(w, cx);
                }))
        }).collect::<Vec<_>>();
        let zoom = ui::PaneZoom::new("jp-out", window, cx);

        div()
            .flex()
            .flex_col()
            .gap(px(6.))
            .flex_1()
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(10.))
                    .child(ui::section_label("Query", &pal).flex_1())
                    .child(ui::seg("jp-show", &["Values", "Paths"], self.locations as usize, &pal, on_show)),
            )
            .child(field_el(&self.path, true, 40., 14., window, cx))
            .child(div().flex().flex_wrap().gap(px(6.)).mt(px(4.)).children(examples))
            .child(
                div()
                    .flex()
                    .gap(px(12.))
                    .flex_1()
                    .min_h(px(360.))
                    .mt(px(10.))
                    .child(
                        ui::pane(is_focused(&self.doc, window, cx), &pal)
                            .flex_1()
                            .child(ui::pane_head("JSON", None, &pal).child(paste).child(clear))
                            .child(code_editor_el(&self.doc, false, cx)),
                    )
                    .child(zoom.wrap(
                        ui::pane(is_focused(&self.output, window, cx), &pal)
                            .flex_1()
                            .child(ui::pane_head("Result", None, &pal).child(ui::copy_btn("jp-copy", self.out.clone(), &pal, window, cx)).child(zoom.button(&pal)))
                            .when_some(self.err.clone(), |d, e| d.child(ui::err_box(e, &pal).m(px(12.))))
                            .child(code_editor_el(&self.output, true, cx))
                            .child(ui::pane_foot(&pal).child(ui::dot(self.status.1, &pal)).child(self.status.0.clone())),
                        &pal,
                        window,
                    )),
            )
    }
}
