use gpui_kit::component::input::{InputState, TextareaState};
use gpui_kit::{
    Context, Entity, IntoElement, ParentElement, Render, SharedString, Styled, Subscription,
    Window, div, px,
};

use super::*;
use crate::logic;
use crate::ui;

pub struct HashView {
    input: Entity<TextareaState>,
    /// HMAC secret; plain digests while it is empty.
    key: Entity<InputState>,
    upper: bool,
    hashes: Vec<(&'static str, String)>,
    wrap: bool,
    _subs: Vec<Subscription>,
}

impl HashView {
    pub fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let input = editor("SideKit", "Text to hash", window, cx);
        let key = line("", "Optional secret key", window, cx);
        let subs = vec![watch(&input, window, cx, Self::recompute), watch(&key, window, cx, Self::recompute)];
        let hashes = logic::hashes("SideKit", None);
        Self { input, key, upper: false, hashes, wrap: false, _subs: subs }
    }

    fn recompute(&mut self, _: &mut Window, cx: &mut Context<Self>) {
        let key = line_text(&self.key, cx);
        self.hashes = logic::hashes(&text_of(&self.input, cx), Some(key.as_str()).filter(|k| !k.is_empty()));
        cx.notify();
    }
}

impl Render for HashView {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let pal = Pal::get(cx);
        sync_wrap(&[&self.input], &mut self.wrap, window, cx);
        let [paste, clear] = paste_clear("hash", &self.input, &pal, cx, Self::recompute);
        let up = self.upper;
        let n = self.hashes.len();
        let mut card = ui::kv_card(&pal);
        for (i, (label, value)) in self.hashes.iter().enumerate() {
            let v: SharedString = if up { value.to_uppercase().into() } else { value.clone().into() };
            card = card.child(ui::kv_copy_row(("h", i), *label, 110., v, true, i + 1 == n, &pal, window, cx));
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
            .child(ui::setting(
                "h-key",
                ui::setting_icon("key", &pal),
                "HMAC key",
                Some("Sign the text with a secret instead of hashing it".into()),
                field_el(&self.key, true, 32., 13., window, cx).w(px(240.)),
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
