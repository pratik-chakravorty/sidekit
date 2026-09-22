use gpui_kit::component::input::InputState;
use gpui_kit::{
    AnimationExt, Context, Entity, IntoElement, ParentElement, Render, SharedString,
    SpringAnimation, Styled, Subscription, Window, div, prelude::FluentBuilder, px,
};

use super::*;
use crate::logic::{group, parse_base, to_radix};
use crate::theme::MONO_FONT;
use crate::ui;

const BASES: [u32; 4] = [16, 10, 8, 2];

pub struct NumBaseView {
    input: Entity<InputState>,
    base: usize,
    format: bool,
    _subs: Vec<Subscription>,
}

impl NumBaseView {
    pub fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let input = line("48879", "Type a number", window, cx);
        let subs = vec![watch(&input, window, cx, |_, _, cx| cx.notify())];
        Self { input, base: 1, format: true, _subs: subs }
    }
}

impl Render for NumBaseView {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let pal = Pal::get(cx);
        let raw = line_text(&self.input, cx);
        let (n, err) = match parse_base(&raw, BASES[self.base]) {
            Ok(n) => (n, None),
            Err(e) => (None, Some(e)),
        };
        let f = self.format;
        let fmt = |radix: u32, size: usize, sep: &str| -> SharedString {
            match n {
                Some(n) => {
                    let s = to_radix(n, radix);
                    if f { group(&s, size, sep).into() } else { s.into() }
                }
                None => SharedString::default(),
            }
        };
        let rows = [
            ("Hexadecimal", fmt(16, 4, " ")),
            ("Decimal", fmt(10, 3, ",")),
            ("Octal", fmt(8, 3, " ")),
            ("Binary", fmt(2, 4, " ")),
        ];
        let big = n.is_some_and(|n| n > 0xFFFF_FFFF);
        let low = n.map(|n| (n & 0xFFFF_FFFF) as u32).unwrap_or(0);

        let on_base = on_index(cx, |this: &mut Self, i, _, cx| {
            this.base = i;
            cx.notify();
        });
        let mut card = ui::kv_card(&pal);
        for (i, (label, value)) in rows.into_iter().enumerate() {
            card = card.child(ui::kv_copy_row(("nb", i), label, 150., value, false, i == 3, &pal, window, cx));
        }

        let bytes = (0..4).rev().map(|by: u32| {
            div()
                .flex()
                .flex_col()
                .items_center()
                .gap(px(6.))
                .child(div().flex().gap(px(3.)).children((0..8).rev().map(|b: u32| {
                    let bit = by * 8 + b;
                    let on = (low >> bit) & 1 == 1;
                    div()
                        .w(px(18.))
                        .h(px(24.))
                        .rounded(px(4.))
                        .flex()
                        .items_center()
                        .justify_center()
                        .font_family(MONO_FONT)
                        .text_size(px(11.))
                        .relative()
                        .child(if on { "1" } else { "0" })
                        .with_spring(("bit", bit as usize), SpringAnimation::new(ui::spring_bouncy()).to(on), move |el, ph| {
                            let v = ph.0.clamp(0., 1.);
                            el.bg(ui::mix(pal.subtle, pal.accent, v))
                                .text_color(ui::mix(pal.text3, pal.accent_text, v))
                                .top(px(-2. * ph.0))
                        })
                })))
                .child(div().text_size(px(11.)).text_color(pal.text3).child(format!("{} – {}", by * 8 + 7, by * 8)))
        });

        div()
            .flex()
            .flex_col()
            .gap(px(6.))
            .child(ui::section_label("Configuration", &pal))
            .child(ui::setting(
                "nb-base",
                ui::setting_icon("hash", &pal),
                "Input type",
                Some("Base of the number you type below".into()),
                ui::seg("nb-seg", &["Hex", "Decimal", "Octal", "Binary"], self.base, &pal, on_base),
                &pal,
            ))
            .child(ui::setting(
                "nb-fmt",
                ui::setting_icon("group", &pal),
                "Format numbers",
                Some("Group digits for readability".into()),
                ui::toggle_labeled("nb-fmt-tg", f, &pal, cx.listener(|this, _, _, cx| {
                    this.format = !this.format;
                    cx.notify();
                })),
                &pal,
            ))
            .child(ui::section_label("Input", &pal).mt(px(14.)))
            .child(field_el(&self.input, true, 44., 16., window, cx))
            .when_some(err, |d, e| d.child(ui::err_box(e, &pal).mt(px(4.))))
            .child(ui::section_label("Output", &pal).mt(px(14.)))
            .child(card)
            .child(
                ui::section_label(if big { "Bits (lowest 32 shown)" } else { "Bits (32-bit view)" }, &pal).mt(px(14.)),
            )
            .child(ui::kv_card(&pal).flex_row().justify_between().gap(px(12.)).px(px(18.)).py(px(16.)).children(bytes))
    }
}
