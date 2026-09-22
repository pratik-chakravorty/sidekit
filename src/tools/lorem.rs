use gpui_kit::component::input::InputState;
use gpui_kit::{
    Context, Entity, InteractiveElement, IntoElement, ParentElement, Render, SharedString, StatefulInteractiveElement, Styled, Subscription,
    Window, div, px, relative,
};

use super::*;
use crate::logic::{LoremKind, lorem, plural};
use crate::theme::UI_FONT;
use crate::ui;

pub struct LoremView {
    count: Entity<InputState>,
    kind: LoremKind,
    seed: u32,
    _subs: Vec<Subscription>,
}

impl LoremView {
    pub fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let count = line("3", "", window, cx);
        let subs = vec![watch(&count, window, cx, |_, _, cx| cx.notify())];
        Self { count, kind: LoremKind::Paragraphs, seed: 1, _subs: subs }
    }
}

impl Render for LoremView {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let pal = Pal::get(cx);
        let n = line_text(&self.count, cx).trim().parse::<usize>().unwrap_or(1).clamp(1, 200);
        let out: SharedString = lorem(self.kind, n, self.seed).into();
        let stat = format!(
            "{} · {}",
            plural(out.split_whitespace().count(), "word"),
            plural(out.chars().count(), "character")
        );
        let kinds = [LoremKind::Words, LoremKind::Sentences, LoremKind::Paragraphs];
        let sel = kinds.iter().position(|k| *k == self.kind).unwrap_or(2);
        let on_kind = on_index(cx, move |this: &mut Self, i, _, cx| {
            this.kind = kinds[i];
            cx.notify();
        });
        let seed = self.seed;

        div()
            .flex()
            .flex_col()
            .gap(px(6.))
            .flex_1()
            .child(ui::section_label("Configuration", &pal))
            .child(ui::setting(
                "lo-type",
                ui::setting_icon("lines", &pal),
                "Type",
                Some("Unit of text to generate".into()),
                ui::seg("lo-seg", &["Words", "Sentences", "Paragraphs"], sel, &pal, on_kind),
                &pal,
            ))
            .child(ui::setting(
                "lo-len",
                ui::setting_icon("hash", &pal),
                "Length",
                Some("How many to generate".into()),
                field_el(&self.count, false, 32., 13., window, cx).w(px(90.)),
                &pal,
            ))
            .child(
                ui::pane(false, &pal)
                    .flex_1()
                    .min_h(px(280.))
                    .mt(px(8.))
                    .child(
                        ui::pane_head("Output", None, &pal)
                            .child(ui::icon_btn("lo-regen", "refresh", &pal, cx.listener(|this, _, _, cx| {
                                this.seed += 1;
                                cx.notify();
                            })))
                            .child(ui::copy_btn("lo-copy", out.clone(), &pal, window, cx)),
                    )
                    .child(
                        div()
                            .id("lo-out")
                            .flex_1()
                            .min_h_0()
                            .overflow_y_scroll()
                            .px(px(14.))
                            .py(px(12.))
                            .font_family(UI_FONT)
                            .text_size(px(14.))
                            .line_height(relative(1.7))
                            .child(ui::enter(crate::id!("lo-text-{seed}"), 0, div().whitespace_normal().child(out))),
                    )
                    .child(ui::pane_foot(&pal).child(stat)),
            )
    }
}
