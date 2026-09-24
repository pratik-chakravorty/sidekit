use gpui_kit::component::input::TextareaState;
use gpui_kit::{
    Context, Entity, IntoElement, ParentElement, Render, SharedString, Styled, Subscription, Task, Window, div, px,
};

use super::*;
use super::big;
use crate::logic::{case_rows, text_stats};
use crate::ui;

/// How much of each converted text a row shows; Copy takes all of it.
const PREVIEW: usize = 400;

pub struct TextCaseView {
    input: Entity<TextareaState>,
    stats: [(String, &'static str); 5],
    rows: Vec<(&'static str, SharedString)>,
    task: Option<Task<()>>,
    _subs: Vec<Subscription>,
}

impl TextCaseView {
    pub fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let input = editor("The quick brown fox jumps over the lazy dog", "Type or paste text", window, cx);
        input.update(cx, |s, cx| s.set_soft_wrap(true, window, cx));
        let subs = vec![watch(&input, window, cx, Self::recompute)];
        let mut this = Self { input, stats: text_stats(""), rows: Vec::new(), task: None, _subs: subs };
        this.recompute(window, cx);
        this
    }

    /// Converting and counting megabytes of text is work for when it changes,
    /// not for every repaint.
    fn recompute(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let t = text_of(&self.input, cx);
        let work = move || {
            let rows: Vec<(&'static str, SharedString)> = case_rows(&t).into_iter().map(|(l, v)| (l, v.into())).collect();
            (text_stats(&t), rows)
        };
        big::run(text_len(&self.input, cx), self, |v| &mut v.task, window, cx, work, |this, (stats, rows), _, cx| {
            this.stats = stats;
            this.rows = rows;
            cx.notify();
        });
    }
}

impl Render for TextCaseView {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let pal = Pal::get(cx);
        let [paste, clear] = paste_clear("tc", &self.input, &pal, cx, Self::recompute);
        let stats = self.stats.clone().map(|(v, l)| ui::stat_tile(v, l, &pal));
        let n = self.rows.len();
        let mut card = ui::kv_card(&pal);
        for (i, (label, value)) in self.rows.iter().enumerate() {
            let id = crate::id!("tc-{i}");
            card = card.child(
                ui::kv_row(id.clone(), i + 1 == n, &pal)
                    .child(ui::kv_label(*label, 150., &pal))
                    .child(ui::kv_val(big::preview(value, PREVIEW), false))
                    .child(ui::copy_btn(ui::child(&id, "copy"), value.clone(), &pal, window, cx)),
            );
        }

        div()
            .flex()
            .flex_col()
            .gap(px(6.))
            .child(
                ui::pane(is_focused(&self.input, window, cx), &pal)
                    .h(px(170.))
                    .child(ui::pane_head("Text", None, &pal).child(paste).child(clear))
                    .child(editor_el(&self.input, false, cx)),
            )
            .child(div().grid().grid_cols(5).gap(px(8.)).mt(px(8.)).children(stats))
            .child(ui::section_label("Convert case", &pal).mt(px(14.)))
            .child(card)
    }
}
