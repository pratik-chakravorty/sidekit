//! Character-by-character view of a string: code points, bytes, and the
//! invisible characters that make two identical-looking strings differ.

use gpui_kit::component::input::TextareaState;
use gpui_kit::{
    Context, Entity, IntoElement, ParentElement, Render, Styled, Subscription, Window, div,
    prelude::FluentBuilder, px,
};

use super::*;
use crate::logic::{char_info, plural, unicode_escapes, unicode_stats};
use crate::theme::MONO_FONT;
use crate::ui::{self, Tone};

/// Rows beyond this are summarised rather than drawn.
const MAX_ROWS: usize = 400;

pub struct UnicodeView {
    input: Entity<TextareaState>,
    wrap: bool,
    _subs: Vec<Subscription>,
}

impl UnicodeView {
    pub fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let input = editor("Café 👋\u{200B}naïve", "Type or paste text", window, cx);
        let subs = vec![watch(&input, window, cx, |_, _, cx| cx.notify())];
        Self { input, wrap: false, _subs: subs }
    }
}

impl Render for UnicodeView {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let pal = Pal::get(cx);
        sync_wrap(&[&self.input], &mut self.wrap, window, cx);
        let t = text_of(&self.input, cx);
        let [paste, clear] = paste_clear("uni", &self.input, &pal, cx, |_: &mut Self, _, cx| cx.notify());
        let stats = unicode_stats(&t).map(|(v, l)| ui::stat_tile(v, l, &pal));

        let total = t.chars().count();
        let shown = total.min(MAX_ROWS);
        let mut chars = ui::kv_card(&pal);
        for (i, c) in t.chars().take(MAX_ROWS).enumerate() {
            let info = char_info(c);
            let abbrev = info.shown.len() > 1 && info.shown.is_ascii();
            let row = ui::kv_row(("uc", i), i + 1 == shown, &pal)
                .when(info.flagged, |d| d.bg(pal.danger_soft))
                .child(
                    div()
                        .w(px(44.))
                        .flex_none()
                        .flex()
                        .justify_center()
                        .text_size(px(if abbrev { 11. } else { 18. }))
                        .text_color(if info.flagged { pal.danger } else { pal.text })
                        .child(info.shown),
                )
                .child(div().w(px(90.)).flex_none().font_family(MONO_FONT).text_size(px(13.)).child(info.code.clone()))
                .child(div().flex_1().min_w_0().text_size(px(13.)).text_color(pal.text2).child(info.kind))
                .when(info.flagged, |d| d.child(ui::badge("Invisible", Tone::Err, &pal)))
                .child(div().w(px(110.)).flex_none().font_family(MONO_FONT).text_size(px(12.)).text_color(pal.text3).child(info.utf8))
                .child(ui::copy_btn(crate::id!("uc-copy-{i}"), info.code.into(), &pal, window, cx));
            chars = chars.child(row);
        }

        let escapes = unicode_escapes(&t);
        let n = escapes.len();
        let mut esc = ui::kv_card(&pal);
        for (i, (label, value)) in escapes.into_iter().enumerate() {
            esc = esc.child(ui::kv_copy_row(("ue", i), label, 150., value.into(), true, i + 1 == n, &pal, window, cx));
        }

        let chars_label = if total > MAX_ROWS {
            format!("Characters · first {MAX_ROWS} of {}", plural(total, "code point"))
        } else {
            "Characters".to_string()
        };

        div()
            .flex()
            .flex_col()
            .gap(px(6.))
            .child(
                ui::pane(is_focused(&self.input, window, cx), &pal)
                    .h(px(150.))
                    .child(ui::pane_head("Text", None, &pal).child(paste).child(clear))
                    .child(editor_el(&self.input, false, cx)),
            )
            .child(div().grid().grid_cols(5).gap(px(8.)).mt(px(8.)).children(stats))
            .when(total > 0, |d| {
                d.child(ui::section_label("Escaped", &pal).mt(px(14.)))
                    .child(esc)
                    .child(ui::section_label(chars_label, &pal).mt(px(14.)))
                    .child(chars)
            })
    }
}
