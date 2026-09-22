use gpui_kit::component::input::TextareaState;
use gpui_kit::{
    Context, Entity, IntoElement, ParentElement, Render, SharedString, Styled, Subscription,
    Window, div, px,
};

use super::*;
use crate::logic;
use crate::ui;

pub struct HashView {
    input: Entity<TextareaState>,
    upper: bool,
    hashes: [(&'static str, String); 5],
    wrap: bool,
    _subs: Vec<Subscription>,
}

impl HashView {
    pub fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let input = editor("SideKit", "Text to hash", window, cx);
        let subs = vec![watch(&input, window, cx, Self::recompute)];
        let hashes = logic::hashes("SideKit");
        Self { input, upper: false, hashes, wrap: false, _subs: subs }
    }

    fn recompute(&mut self, _: &mut Window, cx: &mut Context<Self>) {
        self.hashes = logic::hashes(&text_of(&self.input, cx));
        cx.notify();
    }
}

impl Render for HashView {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let pal = Pal::get(cx);
        sync_wrap(&[&self.input], &mut self.wrap, window, cx);
        let [paste, clear] = paste_clear("hash", &self.input, &pal, cx, Self::recompute);
        let up = self.upper;
        let mut card = ui::kv_card(&pal);
        for (i, (label, value)) in self.hashes.iter().enumerate() {
            let v: SharedString = if up { value.to_uppercase().into() } else { value.clone().into() };
            card = card.child(ui::kv_copy_row(("h", i), *label, 90., v, true, i == 4, &pal, window, cx));
        }
        div()
            .flex()
            .flex_col()
            .gap(px(6.))
            .child(ui::section_label("Configuration", &pal))
            .child(ui::setting(
                "h-upper",
                ui::setting_icon("upper", &pal),
                "Uppercase",
                Some("Display hexadecimal digits in capitals".into()),
                ui::toggle_labeled("h-upper-tg", up, &pal, cx.listener(|this, _, _, cx| {
                    this.upper = !this.upper;
                    cx.notify();
                })),
                &pal,
            ))
            .child(
                ui::pane(is_focused(&self.input, window, cx), &pal)
                    .h(px(160.))
                    .mt(px(10.))
                    .child(ui::pane_head("Input", None, &pal).child(paste).child(clear))
                    .child(editor_el(&self.input, false, cx)),
            )
            .child(ui::section_label("Output", &pal).mt(px(14.)))
            .child(card)
    }
}
