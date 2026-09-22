use gpui_kit::component::slider::{Slider, SliderEvent, SliderState};
use gpui_kit::{
    AnimationExt, AppContext, Context, Entity, FontWeight, IntoElement, ParentElement, Render,
    SpringAnimation, Styled, Subscription, Window, div, prelude::FluentBuilder, px, relative,
};

use super::*;
use crate::logic::{PW_SETS, entropy_bits, make_passwords};
use crate::theme::MONO_FONT;
use crate::ui;

pub struct PasswordView {
    slider: Entity<SliderState>,
    len: usize,
    sets: [bool; 4],
    pwds: Vec<String>,
    batch: usize,
    _subs: Vec<Subscription>,
}

impl PasswordView {
    pub fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let slider = cx.new(|_| SliderState::new().min(8.).max(64.).step(1.).default_value(20.));
        let sub = cx.subscribe_in(&slider, window, |this: &mut Self, _, ev: &SliderEvent, _, cx| {
            if let SliderEvent::Change(v) = ev {
                let len = v.end().round() as usize;
                if len != this.len {
                    this.len = len;
                    this.regenerate(cx);
                }
            }
        });
        let sets = [true; 4];
        Self { slider, len: 20, sets, pwds: make_passwords(20, sets), batch: 0, _subs: vec![sub] }
    }

    fn regenerate(&mut self, cx: &mut Context<Self>) {
        self.pwds = make_passwords(self.len, self.sets);
        self.batch += 1;
        cx.notify();
    }
}

impl Render for PasswordView {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let pal = Pal::get(cx);
        let bits = entropy_bits(self.len, self.sets);
        let none = !self.sets.iter().any(|s| *s);
        let (label, color) = match bits {
            b if b < 40 => ("Weak", pal.danger),
            b if b < 64 => ("Fair", pal.warn),
            b if b < 100 => ("Strong", pal.accent),
            _ => ("Very strong", pal.ok),
        };
        let fill = (bits as f32 / 1.3).min(100.) / 100.;

        let sets = PW_SETS.iter().enumerate().map(|(i, s)| {
            ui::setting(
                ("pw-set", i),
                div().w(px(44.)).font_family(MONO_FONT).text_size(px(13.)).text_color(pal.text2).child(s.sample),
                s.label,
                None,
                ui::toggle(("pw-tg", i), self.sets[i], &pal, cx.listener(move |this, _, _, cx| {
                    this.sets[i] = !this.sets[i];
                    this.regenerate(cx);
                })),
                &pal,
            )
        })
        .collect::<Vec<_>>();

        let n = self.pwds.len();
        let batch = self.batch;
        let mut card = ui::kv_card(&pal);
        for (i, p) in self.pwds.iter().enumerate() {
            let row = ui::kv_row(("pw", i), i + 1 == n, &pal)
                .child(ui::kv_val(p.clone(), false).text_size(px(14.)))
                .child(ui::copy_btn(crate::id!("pw-copy-{i}"), p.clone().into(), &pal, window, cx));
            card = card.child(ui::enter(crate::id!("pw-in-{batch}-{i}"), i as u64 * 25, row));
        }

        div()
            .flex()
            .flex_col()
            .gap(px(6.))
            .child(ui::section_label("Configuration", &pal))
            .child(ui::setting(
                "pw-len",
                ui::setting_icon("length", &pal),
                "Length",
                Some("Number of characters per password".into()),
                div()
                    .flex()
                    .items_center()
                    .gap(px(16.))
                    .child(div().w(px(220.)).child(Slider::new(&self.slider)))
                    .child(
                        div()
                            .w(px(28.))
                            .flex()
                            .justify_end()
                            .font_family(MONO_FONT)
                            .font_weight(FontWeight::MEDIUM)
                            .child(self.len.to_string()),
                    ),
                &pal,
            ))
            .child(div().grid().grid_cols(2).gap(px(6.)).children(sets))
            .child(
                ui::kv_card(&pal)
                    .px(px(18.))
                    .py(px(14.))
                    .gap(px(10.))
                    .mt(px(8.))
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .justify_between()
                            .child(div().text_size(px(13.)).font_weight(FontWeight::SEMIBOLD).child("Strength"))
                            .child(
                                div()
                                    .text_size(px(12.5))
                                    .text_color(pal.text2)
                                    .child(format!("{label} · ≈ {bits} bits of entropy")),
                            ),
                    )
                    .child(
                        div().h(px(6.)).rounded(px(3.)).bg(pal.subtle2).overflow_hidden().child(
                            div().h_full().rounded(px(3.)).bg(color).with_spring(
                                "pw-meter",
                                SpringAnimation::new(ui::spring_soft()).to(fill),
                                |el, w| el.w(relative(w.max(0.))),
                            ),
                        ),
                    ),
            )
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(10.))
                    .mt(px(14.))
                    .child(ui::section_label("Passwords", &pal).flex_1())
                    .child(ui::btn("pw-gen", Some("refresh"), "Regenerate", ui::BtnKind::Accent, &pal, cx.listener(|this, _, _, cx| {
                        this.regenerate(cx)
                    }))),
            )
            .when(none, |d| d.child(ui::err_box("Turn on at least one character set.", &pal)))
            .child(card)
    }
}
