use gpui_kit::component::input::TextareaState;
use gpui_kit::{
    Context, Entity, FontWeight, InteractiveElement, IntoElement, ParentElement, Render, Styled, Subscription,
    Window, div, px,
};

use super::*;
use crate::logic::{case_rows, text_stats};
use crate::ui;

pub struct TextCaseView {
    input: Entity<TextareaState>,
    _subs: Vec<Subscription>,
}

impl TextCaseView {
    pub fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let input = editor("The quick brown fox jumps over the lazy dog", "Type or paste text", window, cx);
        input.update(cx, |s, cx| s.set_soft_wrap(true, window, cx));
        let subs = vec![watch(&input, window, cx, |_, _, cx| cx.notify())];
        Self { input, _subs: subs }
    }
}

impl Render for TextCaseView {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let pal = Pal::get(cx);
        let p = pal;
        let t = text_of(&self.input, cx);
        let [paste, clear] = paste_clear("tc", &self.input, &pal, cx, |_: &mut Self, _, cx| cx.notify());
        let stats = text_stats(&t).map(|(v, l)| {
            div()
                .id(l)
                .flex()
                .flex_col()
                .gap(px(2.))
                .px(px(16.))
                .py(px(12.))
                .min_w_0()
                .rounded(px(8.))
                .bg(p.card)
                .border_1()
                .border_color(p.stroke)
                .hover(move |s| s.border_color(p.accent))
                .child(div().text_size(px(22.)).font_weight(FontWeight::SEMIBOLD).child(v))
                .child(div().text_size(px(12.)).text_color(p.text3).child(l))
        });
        let rows = case_rows(&t);
        let n = rows.len();
        let mut card = ui::kv_card(&pal);
        for (i, (label, value)) in rows.into_iter().enumerate() {
            card = card.child(ui::kv_copy_row(("tc", i), label, 150., value.into(), false, i + 1 == n, &pal, window, cx));
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
