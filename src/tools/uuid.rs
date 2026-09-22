use gpui_kit::component::input::InputState;
use gpui_kit::{
    Context, Entity, IntoElement, ParentElement, Render, SharedString, Styled, Subscription,
    Window, div, px,
};

use super::*;
use crate::logic::uuid_v4;
use crate::ui::{self, BtnKind, Tone};

pub struct UuidView {
    count: Entity<InputState>,
    hyphens: bool,
    upper: bool,
    uuids: Vec<String>,
    /// Bumped per generation so new rows animate in.
    batch: usize,
    _subs: Vec<Subscription>,
}

impl UuidView {
    pub fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let count = line("5", "", window, cx);
        let subs = vec![watch(&count, window, cx, |this: &mut Self, _, cx| this.generate(cx))];
        let mut this = Self { count, hyphens: true, upper: false, uuids: Vec::new(), batch: 0, _subs: subs };
        this.uuids = (0..5).map(|_| uuid_v4()).collect();
        this
    }

    fn generate(&mut self, cx: &mut Context<Self>) {
        let n = line_text(&self.count, cx).trim().parse::<usize>().unwrap_or(1).clamp(1, 50);
        self.uuids = (0..n).map(|_| uuid_v4()).collect();
        self.batch += 1;
        cx.notify();
    }
}

impl Render for UuidView {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let pal = Pal::get(cx);
        let list: Vec<String> = self
            .uuids
            .iter()
            .map(|u| {
                let x = if self.hyphens { u.clone() } else { u.replace('-', "") };
                if self.upper { x.to_uppercase() } else { x }
            })
            .collect();
        let all: SharedString = list.join("\n").into();
        let n = list.len();
        let batch = self.batch;
        let mut card = ui::kv_card(&pal);
        for (i, u) in list.into_iter().enumerate() {
            let row = ui::kv_row(("u", i), i + 1 == n, &pal)
                .child(ui::kv_label(format!("{:02}", i + 1), 40., &pal).text_color(pal.text3))
                .child(ui::kv_val(u.clone(), false).text_size(px(14.)))
                .child(ui::copy_btn(crate::id!("u-copy-{i}"), u.into(), &pal, window, cx));
            card = card.child(ui::enter(crate::id!("u-in-{batch}-{i}"), (i as u64 * 18).min(200), row));
        }

        div()
            .flex()
            .flex_col()
            .gap(px(6.))
            .child(ui::section_label("Configuration", &pal))
            .child(ui::setting(
                "u-hy",
                ui::setting_icon("dashes", &pal),
                "Hyphens",
                None,
                ui::toggle_labeled("u-hy-tg", self.hyphens, &pal, cx.listener(|this, _, _, cx| {
                    this.hyphens = !this.hyphens;
                    cx.notify();
                })),
                &pal,
            ))
            .child(ui::setting(
                "u-up",
                ui::setting_icon("upper", &pal),
                "Uppercase",
                None,
                ui::toggle_labeled("u-up-tg", self.upper, &pal, cx.listener(|this, _, _, cx| {
                    this.upper = !this.upper;
                    cx.notify();
                })),
                &pal,
            ))
            .child(ui::setting(
                "u-ver",
                ui::setting_icon("version", &pal),
                "Version",
                Some("Random-based identifiers".into()),
                ui::badge("UUID v4", Tone::Info, &pal),
                &pal,
            ))
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(10.))
                    .mt(px(14.))
                    .child(ui::section_label("Generate", &pal).flex_1())
                    .child(field_el(&self.count, false, 32., 13., window, cx).w(px(80.)))
                    .child(div().text_size(px(13.)).text_color(pal.text2).child("UUIDs"))
                    .child(ui::btn("u-gen", Some("refresh"), "Generate", BtnKind::Accent, &pal, cx.listener(|this, _, _, cx| this.generate(cx))))
                    .child(ui::copy_btn_labeled("u-all", "Copy all", all, &pal, window, cx)),
            )
            .child(card)
    }
}
