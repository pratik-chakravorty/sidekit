use gpui_kit::component::color_picker::{ColorPicker, ColorPickerEvent, ColorPickerState};
use gpui_kit::component::input::InputState;
use gpui_kit::{
    AppContext, Context, Entity, FontWeight, Hsla, IntoElement, ParentElement, Render, Rgba,
    SharedString, StatefulInteractiveElement, Styled, Subscription, Window, div,
    prelude::FluentBuilder, px, rgb, InteractiveElement,
};

use super::*;
use crate::logic::{contrast, grade, hex, hsl_to_rgb, parse_color, rgb_to_hsl};
use crate::theme::MONO_FONT;
use crate::ui::{self, Tone};

fn to_hsla(c: [u8; 3]) -> Hsla {
    rgb(((c[0] as u32) << 16) | ((c[1] as u32) << 8) | c[2] as u32).into()
}

fn from_hsla(h: Hsla) -> [u8; 3] {
    let r = Rgba::from(h);
    [(r.r * 255.).round() as u8, (r.g * 255.).round() as u8, (r.b * 255.).round() as u8]
}

pub struct ColorView {
    input: Entity<InputState>,
    picker: Entity<ColorPickerState>,
    _subs: Vec<Subscription>,
}

impl ColorView {
    pub fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let input = line("#0067C0", "#RRGGBB, rgb(), hsl()", window, cx);
        let picker = cx.new(|cx| ColorPickerState::new(window, cx).default_value(to_hsla([0, 103, 192])));
        let subs = vec![
            watch(&input, window, cx, |this: &mut Self, window, cx| {
                if let Some(c) = parse_color(&line_text(&this.input, cx)) {
                    this.picker.update(cx, |p, cx| p.set_value(to_hsla(c), window, cx));
                }
                cx.notify();
            }),
            cx.subscribe_in(&picker, window, |this: &mut Self, _, ev: &ColorPickerEvent, window, cx| {
                if let ColorPickerEvent::Change(Some(c)) = ev {
                    set_line(&this.input, &hex(from_hsla(*c)), window, cx);
                    cx.notify();
                }
            }),
        ];
        Self { input, picker, _subs: subs }
    }

    pub fn set_input(&mut self, text: &str, window: &mut Window, cx: &mut Context<Self>) {
        self.pick(text, window, cx);
    }

    fn pick(&mut self, value: &str, window: &mut Window, cx: &mut Context<Self>) {
        set_line(&self.input, value, window, cx);
        if let Some(c) = parse_color(value) {
            self.picker.update(cx, |p, cx| p.set_value(to_hsla(c), window, cx));
        }
        cx.notify();
    }
}

impl Render for ColorView {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let pal = Pal::get(cx);
        let parsed = parse_color(&line_text(&self.input, cx));
        let c = parsed.unwrap_or([0, 103, 192]);
        let (h, s, l) = rgb_to_hsl(c);
        let mx = *c.iter().max().unwrap() as f64 / 255.;
        let mn = *c.iter().min().unwrap() as f64 / 255.;
        let hsv_s = if mx == 0. { 0 } else { ((mx - mn) / mx * 100.).round() as i32 };
        let hsv_v = (mx * 100.).round() as i32;
        let hex_s = hex(c);
        let rows: [(&str, SharedString); 4] = [
            ("HEX", hex_s.clone().into()),
            ("RGB", format!("rgb({}, {}, {})", c[0], c[1], c[2]).into()),
            ("HSL", format!("hsl({h}, {s}%, {l}%)").into()),
            ("HSV", format!("hsv({h}, {hsv_s}%, {hsv_v}%)").into()),
        ];
        let w = contrast(c, [255, 255, 255]);
        let b = contrast(c, [0, 0, 0]);
        let tone = |r: f64| if r >= 4.5 { Tone::Ok } else { Tone::Err };
        let swatch = to_hsla(c);

        let mut card = ui::kv_card(&pal);
        for (i, (label, value)) in rows.into_iter().enumerate() {
            card = card.child(ui::kv_copy_row(("c", i), label, 90., value, false, false, &pal, window, cx));
        }
        card = card.child(
            ui::kv_row("c-contrast", true, &pal)
                .child(ui::kv_label("Contrast", 90., &pal))
                .child(
                    div()
                        .flex()
                        .flex_wrap()
                        .gap(px(8.))
                        .child(ui::badge(format!("White text · {:.2}:1 · {}", w, grade(w)), tone(w), &pal))
                        .child(ui::badge(format!("Black text · {:.2}:1 · {}", b, grade(b)), tone(b), &pal)),
                ),
        );

        let sample = |fg: Hsla, ratio: f64| {
            div()
                .flex()
                .justify_between()
                .items_center()
                .px(px(12.))
                .py(px(8.))
                .rounded(px(6.))
                .text_color(fg)
                .text_size(px(20.))
                .font_weight(FontWeight::SEMIBOLD)
                .child("Aa")
                .child(div().text_size(px(12.)).child(format!("{ratio:.1}:1")))
        };

        let shades = [95., 85., 72., 60., 48., 38., 28., 18., 10.].map(|lv| {
            let x = hsl_to_rgb(h as f64, s as f64, lv);
            let hx = hex(x);
            let fg = if lv > 55. { rgb(0x1a1a1a) } else { rgb(0xffffff) };
            let value = hx.clone();
            div()
                .id(crate::id!("shade-{hx}"))
                .flex_1()
                .h(px(56.))
                .rounded(px(6.))
                .border_1()
                .border_color(pal.stroke)
                .bg(to_hsla(x))
                .cursor_pointer()
                .flex()
                .items_end()
                .justify_center()
                .pb(px(6.))
                .font_family(MONO_FONT)
                .text_size(px(10.))
                .text_color(fg)
                .hover(|s| s.opacity(0.9))
                .on_click(cx.listener(move |this, _, window, cx| this.pick(&value, window, cx)))
                .child(hx)
        });

        div()
            .flex()
            .flex_col()
            .gap(px(6.))
            .child(ui::section_label("Input", &pal))
            .child(
                div()
                    .flex()
                    .gap(px(8.))
                    .items_center()
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .justify_center()
                            .w(px(40.))
                            .h(px(40.))
                            .rounded(px(6.))
                            .border_1()
                            .border_color(pal.stroke_strong)
                            .bg(pal.editor)
                            .child(ColorPicker::new(&self.picker)),
                    )
                    .child(field_el(&self.input, true, 40., 15., window, cx).flex_1()),
            )
            .when(parsed.is_none(), |d| {
                d.child(ui::err_box("Enter a color like #0067C0, rgb(0, 103, 192) or hsl(208, 100%, 38%).", &pal).mt(px(4.)))
            })
            .child(
                div()
                    .flex()
                    .gap(px(12.))
                    .mt(px(14.))
                    .child(
                        ui::kv_card(&pal)
                            .w(px(300.))
                            .flex_none()
                            .min_h(px(238.))
                            .bg(swatch)
                            .justify_end()
                            .p(px(16.))
                            .gap(px(8.))
                            .child(sample(rgb(0xffffff).into(), w))
                            .child(sample(rgb(0x000000).into(), b)),
                    )
                    .child(card.flex_1()),
            )
            .child(ui::section_label("Shades", &pal).mt(px(14.)))
            .child(div().flex().gap(px(6.)).children(shades))
    }
}
