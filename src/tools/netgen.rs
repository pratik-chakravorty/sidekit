//! Random network identifiers: MAC addresses and IPv6 unique local prefixes.

use gpui_kit::component::input::InputState;
use gpui_kit::{
    App, Context, Div, Entity, IntoElement, ParentElement, Render, SharedString, Styled,
    Subscription, Window, div, px,
};

use super::*;
use crate::logic::{MAC_FORMATS, mac_address, ula};
use crate::ui::{self, BtnKind};

fn count_of(state: &Entity<InputState>, cx: &App) -> usize {
    line_text(state, cx).trim().parse::<usize>().unwrap_or(1).clamp(1, 50)
}

/// The "Generate N … / Copy all" bar over a generated list.
fn generate_bar<V: 'static>(
    id: &'static str,
    count: &Entity<InputState>,
    noun: &'static str,
    all: SharedString,
    window: &mut Window,
    cx: &mut Context<V>,
    generate: fn(&mut V, &mut Context<V>),
) -> Div {
    let pal = Pal::get(cx);
    div()
        .flex()
        .items_center()
        .gap(px(10.))
        .mt(px(14.))
        .child(ui::section_label("Generate", &pal).flex_1())
        .child(field_el(count, false, 32., 13., window, cx).w(px(80.)))
        .child(div().text_size(px(13.)).text_color(pal.text2).child(noun))
        .child(ui::btn(crate::id!("{id}-gen"), Some("refresh"), "Generate", BtnKind::Accent, &pal, cx.listener(move |this, _, _, cx| generate(this, cx))))
        .child(ui::copy_btn_labeled(crate::id!("{id}-all"), "Copy all", all, &pal, window, cx))
}

/// Numbered rows that animate in with each new batch.
fn numbered(id: &'static str, rows: &[(String, Option<String>)], batch: usize, window: &mut Window, cx: &mut App) -> Div {
    let pal = Pal::get(cx);
    let n = rows.len();
    let mut card = ui::kv_card(&pal);
    for (i, (value, note)) in rows.iter().enumerate() {
        let mut row = ui::kv_row(crate::id!("{id}-{i}"), i + 1 == n, &pal)
            .child(ui::kv_label(format!("{:02}", i + 1), 40., &pal).text_color(pal.text3))
            .child(ui::kv_val(value.clone(), false).text_size(px(14.)));
        if let Some(note) = note {
            row = row.child(div().flex_none().font_family(crate::theme::MONO_FONT).text_size(px(12.)).text_color(pal.text3).child(note.clone()));
        }
        let row = row.child(ui::copy_btn(crate::id!("{id}-copy-{i}"), value.clone().into(), &pal, window, cx));
        card = card.child(ui::enter(crate::id!("{id}-in-{batch}-{i}"), (i as u64 * 18).min(200), row));
    }
    card
}

// ------------------------------------------------------------ MAC addresses

pub struct MacView {
    count: Entity<InputState>,
    format: usize,
    upper: bool,
    local: bool,
    macs: Vec<String>,
    batch: usize,
    _subs: Vec<Subscription>,
}

impl MacView {
    pub fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let count = line("5", "", window, cx);
        let subs = vec![watch(&count, window, cx, |this: &mut Self, _, cx| this.generate(cx))];
        let mut this = Self { count, format: 0, upper: true, local: true, macs: Vec::new(), batch: 0, _subs: subs };
        this.generate(cx);
        this
    }

    fn generate(&mut self, cx: &mut Context<Self>) {
        let n = count_of(&self.count, cx);
        self.macs = (0..n).map(|_| mac_address(self.format, self.upper, self.local)).collect();
        self.batch += 1;
        cx.notify();
    }
}

impl Render for MacView {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let pal = Pal::get(cx);
        let rows: Vec<(String, Option<String>)> = self.macs.iter().map(|m| (m.clone(), None)).collect();
        let all: SharedString = self.macs.join("\n").into();
        let on_format = on_index(cx, |this: &mut Self, i, _, cx| {
            this.format = i;
            this.generate(cx);
        });

        div()
            .flex()
            .flex_col()
            .gap(px(6.))
            .child(ui::section_label("Configuration", &pal))
            .child(ui::setting(
                "mac-fmt",
                ui::setting_icon("dashes", &pal),
                "Format",
                Some("Separator style".into()),
                ui::dropdown("mac-fmt-dd", MAC_FORMATS, self.format, &pal, window, cx, on_format),
                &pal,
            ))
            .child(ui::setting(
                "mac-up",
                ui::setting_icon("upper", &pal),
                "Uppercase",
                None,
                ui::toggle_labeled("mac-up-tg", self.upper, &pal, cx.listener(|this, _, _, cx| {
                    this.upper = !this.upper;
                    this.generate(cx);
                })),
                &pal,
            ))
            .child(ui::setting(
                "mac-local",
                ui::setting_icon("shield", &pal),
                "Locally administered",
                Some("Set the local bit so addresses never collide with a vendor's".into()),
                ui::toggle_labeled("mac-local-tg", self.local, &pal, cx.listener(|this, _, _, cx| {
                    this.local = !this.local;
                    this.generate(cx);
                })),
                &pal,
            ))
            .child(generate_bar("mac", &self.count, "addresses", all, window, cx, Self::generate))
            .child(numbered("mac-row", &rows, self.batch, window, cx))
    }
}

// ------------------------------------------------------------ IPv6 ULA

pub struct UlaView {
    count: Entity<InputState>,
    prefixes: Vec<(String, String)>,
    batch: usize,
    _subs: Vec<Subscription>,
}

impl UlaView {
    pub fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let count = line("5", "", window, cx);
        let subs = vec![watch(&count, window, cx, |this: &mut Self, _, cx| this.generate(cx))];
        let mut this = Self { count, prefixes: Vec::new(), batch: 0, _subs: subs };
        this.generate(cx);
        this
    }

    fn generate(&mut self, cx: &mut Context<Self>) {
        let n = count_of(&self.count, cx);
        self.prefixes = (0..n).map(|_| ula()).collect();
        self.batch += 1;
        cx.notify();
    }
}

impl Render for UlaView {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let pal = Pal::get(cx);
        let rows: Vec<(String, Option<String>)> =
            self.prefixes.iter().map(|(p, s)| (p.clone(), Some(format!("e.g. {s}")))).collect();
        let all: SharedString = self.prefixes.iter().map(|(p, _)| p.as_str()).collect::<Vec<_>>().join("\n").into();

        div()
            .flex()
            .flex_col()
            .gap(px(6.))
            .child(ui::section_label("About", &pal))
            .child(ui::setting(
                "ula-about",
                ui::setting_icon("globe", &pal),
                "Unique local addresses",
                Some("fd00::/8 with a random 40-bit global ID (RFC 4193): a private /48 with 65,536 /64 subnets".into()),
                ui::badge("/48", ui::Tone::Info, &pal),
                &pal,
            ))
            .child(generate_bar("ula", &self.count, "prefixes", all, window, cx, Self::generate))
            .child(numbered("ula-row", &rows, self.batch, window, cx))
    }
}
