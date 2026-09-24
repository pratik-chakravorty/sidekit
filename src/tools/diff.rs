//! Compare two texts line by line, or two JSON / YAML documents path by path.

use gpui_kit::component::input::EditorState;
use gpui_kit::component::scroll::ScrollableElement;
use gpui_kit::{
    Context, Entity, FontWeight, InteractiveElement, IntoElement, ParentElement, Render, Styled,
    Subscription, Task, Window, div, prelude::FluentBuilder, px, relative,
};

use super::*;
use super::big;
use crate::logic::diff::{self, Change, PathChange, Side, TextDiffOut};
use crate::theme::MONO_FONT;
use crate::ui::{self, Tone};

/// Rows beyond this are not drawn; the counts still cover everything.
const MAX_ROWS: usize = 4000;

fn two_panes<V: 'static>(
    ids: (&'static str, &'static str),
    titles: (&'static str, &'static str),
    a: &Entity<EditorState>,
    b: &Entity<EditorState>,
    window: &mut Window,
    cx: &mut Context<V>,
    after: impl Fn(&mut V, &mut Window, &mut Context<V>) + Clone + 'static,
) -> gpui_kit::Div {
    let pal = Pal::get(cx);
    let [pa, ca] = paste_clear(ids.0, a, &pal, cx, after.clone());
    let [pb, cb] = paste_clear(ids.1, b, &pal, cx, after);
    div()
        .flex()
        .gap(px(12.))
        .h(px(280.))
        .child(ui::pane(is_focused(a, window, cx), &pal).flex_1().child(ui::pane_head(titles.0, None, &pal).child(pa).child(ca)).child(code_editor_el(a, false, cx)))
        .child(ui::pane(is_focused(b, window, cx), &pal).flex_1().child(ui::pane_head(titles.1, None, &pal).child(pb).child(cb)).child(code_editor_el(b, false, cx)))
}

// ------------------------------------------------------------ text diff

pub struct TextDiffView {
    a: Entity<EditorState>,
    b: Entity<EditorState>,
    ignore_ws: bool,
    d: TextDiffOut,
    task: Option<Task<()>>,
    _subs: Vec<Subscription>,
}

impl TextDiffView {
    pub fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let a = code_editor("server:\n  host: localhost\n  port: 8080\n  debug: true\nfeatures:\n  - search\n  - export\n", "Original text", "plaintext", window, cx);
        let b = code_editor("server:\n  host: 0.0.0.0\n  port: 8080\nfeatures:\n  - search\n  - export\n  - sharing\n", "Changed text", "plaintext", window, cx);
        let subs = vec![watch(&a, window, cx, Self::recompute), watch(&b, window, cx, Self::recompute)];
        let mut this = Self { a, b, ignore_ws: false, d: diff::text_diff("", "", false), task: None, _subs: subs };
        this.recompute(window, cx);
        this
    }

    fn recompute(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let (a, b, ws) = (text_of(&self.a, cx), text_of(&self.b, cx), self.ignore_ws);
        let size = a.len() + b.len();
        big::run(size, self, |v| &mut v.task, window, cx, move || diff::text_diff(&a, &b, ws), |this, d, _, cx| {
            this.d = d;
            cx.notify();
        });
    }
}

impl Render for TextDiffView {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let pal = Pal::get(cx);
        let d = &self.d;
        let fs = crate::settings::Settings::get(cx).font_size as f32;
        let same = d.added + d.removed == 0;
        let summary = if same {
            "No differences".to_string()
        } else {
            format!("+{} added · −{} removed", d.added, d.removed)
        };

        let rows = d.lines.iter().take(MAX_ROWS).enumerate().map(|(i, l)| {
            let (bg, sign, fg) = match l.side {
                Side::Added => (Some(pal.ok_soft), "+", pal.ok),
                Side::Removed => (Some(pal.danger_soft), "−", pal.danger),
                Side::Same => (None, " ", pal.text3),
            };
            let num = |n: Option<usize>| div().w(px(44.)).flex_none().text_right().pr(px(8.)).text_color(pal.text3).child(n.map(|n| n.to_string()).unwrap_or_default());
            div()
                .id(("td-row", i))
                .flex()
                .min_h(px(fs * 1.6))
                .when_some(bg, |d, c| d.bg(c))
                .child(num(l.old))
                .child(num(l.new))
                .child(div().w(px(18.)).flex_none().text_color(fg).child(sign))
                .child(div().flex_1().min_w_0().whitespace_nowrap().child(l.text.clone()))
        });

        div()
            .flex()
            .flex_col()
            .gap(px(6.))
            .child(ui::section_label("Configuration", &pal))
            .child(ui::setting(
                "td-ws",
                ui::setting_icon("length", &pal),
                "Ignore whitespace",
                Some("Treat runs of spaces and tabs as one, and ignore them at line ends".into()),
                ui::toggle_labeled("td-ws-tg", self.ignore_ws, &pal, cx.listener(|this, _, w, cx| {
                    this.ignore_ws = !this.ignore_ws;
                    this.recompute(w, cx);
                })),
                &pal,
            ))
            .child(two_panes(("td-a", "td-b"), ("Original", "Changed"), &self.a, &self.b, window, cx, Self::recompute).mt(px(8.)))
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(10.))
                    .mt(px(14.))
                    .child(ui::section_label("Differences", &pal).flex_1())
                    .child(ui::badge(summary, if same { Tone::Ok } else { Tone::Neutral }, &pal)),
            )
            .child(
                div()
                    .id("td-out")
                    .max_h(px(520.))
                    .overflow_y_scrollbar()
                    .py(px(6.))
                    .rounded(px(8.))
                    .bg(pal.editor)
                    .border_1()
                    .border_color(pal.stroke)
                    .font_family(MONO_FONT)
                    .text_size(px(fs))
                    .line_height(relative(1.6))
                    .children(rows)
                    .when(d.lines.len() > MAX_ROWS, |el| {
                        el.child(div().px(px(14.)).py(px(6.)).text_color(pal.text3).child(format!("… {} more lines", d.lines.len() - MAX_ROWS)))
                    }),
            )
    }
}

// ------------------------------------------------------------ structured diff

pub struct DataDiffView {
    a: Entity<EditorState>,
    b: Entity<EditorState>,
    changes: Result<Vec<PathChange>, String>,
    task: Option<Task<()>>,
    _subs: Vec<Subscription>,
}

fn data_changes(a: &str, b: &str) -> Result<Vec<PathChange>, String> {
    let a = diff::parse_doc(a).map_err(|e| format!("Left: {e}"))?;
    let b = diff::parse_doc(b).map_err(|e| format!("Right: {e}"))?;
    Ok(diff::structured_diff(&a, &b))
}

impl DataDiffView {
    pub fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let a = code_editor(
            "{\n  \"name\": \"api\",\n  \"replicas\": 2,\n  \"env\": { \"LOG_LEVEL\": \"info\" },\n  \"ports\": [80, 443]\n}",
            "JSON or YAML",
            "json",
            window,
            cx,
        );
        let b = code_editor(
            "{\n  \"name\": \"api\",\n  \"replicas\": 3,\n  \"env\": { \"LOG_LEVEL\": \"debug\", \"TRACE\": true },\n  \"ports\": [80]\n}",
            "JSON or YAML",
            "json",
            window,
            cx,
        );
        let subs = vec![watch(&a, window, cx, Self::recompute), watch(&b, window, cx, Self::recompute)];
        let mut this = Self { a, b, changes: Ok(Vec::new()), task: None, _subs: subs };
        this.recompute(window, cx);
        this
    }

    fn recompute(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let (a, b) = (text_of(&self.a, cx), text_of(&self.b, cx));
        let size = a.len() + b.len();
        big::run(size, self, |v| &mut v.task, window, cx, move || data_changes(&a, &b), |this, c, _, cx| {
            this.changes = c;
            cx.notify();
        });
    }
}

impl Render for DataDiffView {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let pal = Pal::get(cx);
        let body = match &self.changes {
            Err(e) => ui::err_box(e.clone(), &pal).into_any_element(),
            Ok(changes) => {
                if changes.is_empty() {
                    div().text_size(px(13.)).text_color(pal.text3).child("Both documents hold the same data.").into_any_element()
                } else {
                    let n = changes.len();
                    let mut card = ui::kv_card(&pal);
                    for (i, c) in changes.iter().take(MAX_ROWS).enumerate() {
                        let (label, tone) = match c.change {
                            Change::Added => ("Added", Tone::Ok),
                            Change::Removed => ("Removed", Tone::Err),
                            Change::Changed => ("Changed", Tone::Info),
                        };
                        let value = match (c.old.clone(), c.new.clone()) {
                            (Some(o), Some(n)) => format!("{o}  →  {n}"),
                            (Some(o), None) => o,
                            (None, Some(n)) => n,
                            (None, None) => String::new(),
                        };
                        card = card.child(
                            ui::kv_row(("dd", i), i + 1 == n, &pal)
                                .child(div().w(px(86.)).flex_none().child(ui::badge(label, tone, &pal)))
                                .child(div().w(px(240.)).flex_none().font_family(MONO_FONT).text_size(px(13.)).font_weight(FontWeight::MEDIUM).overflow_hidden().text_ellipsis().whitespace_nowrap().child(c.path.clone()))
                                .child(ui::kv_val(value, true).text_color(pal.text2)),
                        );
                    }
                    card.into_any_element()
                }
            }
        };

        div()
            .flex()
            .flex_col()
            .gap(px(6.))
            .child(div().text_size(px(13.)).text_color(pal.text2).child("Paste JSON or YAML on each side. Key order is ignored; arrays are compared item by item."))
            .child(two_panes(("dd-a", "dd-b"), ("Before", "After"), &self.a, &self.b, window, cx, Self::recompute).mt(px(8.)))
            .child(ui::section_label("Changes", &pal).mt(px(14.)))
            .child(body)
    }
}
