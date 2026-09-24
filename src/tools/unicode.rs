//! Character-by-character view of a string: code points, bytes, and the
//! invisible characters that make two identical-looking strings differ.

use gpui_kit::component::input::TextareaState;
use gpui_kit::{
    Context, Entity, IntoElement, ParentElement, Render, SharedString, Styled, Subscription, Task, Window,
    div, prelude::FluentBuilder, px,
};

use super::*;
use super::big;
use crate::logic::{CharInfo, char_info, plural, unicode_escapes, unicode_stats};
use crate::theme::MONO_FONT;
use crate::ui::{self, Tone};

/// Rows beyond this are summarised rather than drawn.
const MAX_ROWS: usize = 400;
/// How much of each escaped form a row shows; Copy takes all of it.
const PREVIEW: usize = 1200;

/// Everything shown about the text, worked out once per edit.
struct Analysis {
    stats: [(String, &'static str); 5],
    escapes: Vec<(&'static str, SharedString)>,
    chars: Vec<CharInfo>,
    total: usize,
}

fn analyse(t: &str) -> Analysis {
    Analysis {
        stats: unicode_stats(t),
        escapes: unicode_escapes(t).into_iter().map(|(l, v)| (l, v.into())).collect(),
        chars: t.chars().take(MAX_ROWS).map(char_info).collect(),
        total: t.chars().count(),
    }
}

pub struct UnicodeView {
    input: Entity<TextareaState>,
    wrap: bool,
    a: Analysis,
    task: Option<Task<()>>,
    _subs: Vec<Subscription>,
}

impl UnicodeView {
    pub fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let input = editor("Café 👋\u{200B}naïve", "Type or paste text", window, cx);
        let subs = vec![watch(&input, window, cx, Self::recompute)];
        let mut this = Self { input, wrap: false, a: analyse(""), task: None, _subs: subs };
        this.recompute(window, cx);
        this
    }

    fn recompute(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let t = text_of(&self.input, cx);
        big::run(text_len(&self.input, cx), self, |v| &mut v.task, window, cx, move || analyse(&t), |this, a, _, cx| {
            this.a = a;
            cx.notify();
        });
    }
}

impl Render for UnicodeView {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let pal = Pal::get(cx);
        sync_wrap(&[&self.input], &mut self.wrap, window, cx);
        let [paste, clear] = paste_clear("uni", &self.input, &pal, cx, Self::recompute);
        let stats = self.a.stats.clone().map(|(v, l)| ui::stat_tile(v, l, &pal));
        let total = self.a.total;

        let shown = self.a.chars.len();
        let mut chars = ui::kv_card(&pal);
        for (i, info) in self.a.chars.iter().enumerate() {
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
                        .child(info.shown.clone()),
                )
                .child(div().w(px(90.)).flex_none().font_family(MONO_FONT).text_size(px(13.)).child(info.code.clone()))
                .child(div().flex_1().min_w_0().text_size(px(13.)).text_color(pal.text2).child(info.kind))
                .when(info.flagged, |d| d.child(ui::badge("Invisible", Tone::Err, &pal)))
                .child(div().w(px(110.)).flex_none().font_family(MONO_FONT).text_size(px(12.)).text_color(pal.text3).child(info.utf8.clone()))
                .child(ui::copy_btn(crate::id!("uc-copy-{i}"), info.code.clone().into(), &pal, window, cx));
            chars = chars.child(row);
        }

        let n = self.a.escapes.len();
        let mut esc = ui::kv_card(&pal);
        for (i, (label, value)) in self.a.escapes.iter().enumerate() {
            let id = crate::id!("ue-{i}");
            esc = esc.child(
                ui::kv_row(id.clone(), i + 1 == n, &pal)
                    .child(ui::kv_label(*label, 150., &pal))
                    .child(ui::kv_val(big::preview(value, PREVIEW), true))
                    .child(ui::copy_btn(ui::child(&id, "copy"), value.clone(), &pal, window, cx)),
            );
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
