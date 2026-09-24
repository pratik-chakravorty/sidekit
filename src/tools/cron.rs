//! Read a cron expression in plain English and list its next runs.

use gpui_kit::component::input::InputState;
use gpui_kit::{
    Context, Entity, FontWeight, InteractiveElement, IntoElement, ParentElement, Render,
    StatefulInteractiveElement, Styled, Subscription, Window, div, prelude::FluentBuilder, px,
};

use super::*;
use crate::logic::cron;
use crate::theme::MONO_FONT;
use crate::ui;

const EXAMPLES: &[(&str, &str)] = &[
    ("*/15 * * * *", "every 15 min"),
    ("0 9 * * 1-5", "weekdays 9:00"),
    ("0 0 * * *", "midnight"),
    ("0 0 1 * *", "monthly"),
    ("30 2 * * sun", "Sundays 2:30"),
];

pub struct CronView {
    input: Entity<InputState>,
    _subs: Vec<Subscription>,
}

impl CronView {
    pub fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let input = line("0 9 * * 1-5", "minute hour day month weekday", window, cx);
        let subs = vec![watch(&input, window, cx, |_, _, cx| cx.notify())];
        Self { input, _subs: subs }
    }
}

fn until(secs: i64) -> String {
    match secs {
        ..60 => "in under a minute".into(),
        60..3600 => format!("in {} min", secs / 60),
        3600..86_400 => format!("in {} h {} min", secs / 3600, secs % 3600 / 60),
        _ => format!("in {} days", secs / 86_400),
    }
}

impl Render for CronView {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let pal = Pal::get(cx);
        let raw = line_text(&self.input, cx);
        let parsed = (!raw.trim().is_empty()).then(|| cron::parse(&raw));

        let examples = EXAMPLES.iter().enumerate().map(|(i, (expr, label))| {
            let expr = *expr;
            div()
                .id(("cron-ex", i))
                .flex()
                .gap(px(6.))
                .px(px(10.))
                .h(px(26.))
                .items_center()
                .rounded(px(13.))
                .border_1()
                .border_color(pal.stroke)
                .cursor_pointer()
                .text_size(px(12.))
                .text_color(pal.text2)
                .hover(move |s| s.bg(pal.subtle).text_color(pal.text))
                .child(div().font_family(MONO_FONT).child(expr))
                .child(div().text_color(pal.text3).child(*label))
                .on_click(cx.listener(move |this, _, w, cx| {
                    set_line(&this.input, expr, w, cx);
                    cx.notify();
                }))
        });

        let mut body = div().flex().flex_col().gap(px(6.));
        match parsed {
            Some(Ok(c)) => {
                let now = chrono::Local::now();
                let runs = c.next_runs(&now, 8);
                let fields = [
                    ("Minute", &c.minute),
                    ("Hour", &c.hour),
                    ("Day of month", &c.dom),
                    ("Month", &c.month),
                    ("Day of week", &c.dow),
                ];
                let mut card = ui::kv_card(&pal);
                let n = fields.len();
                for (i, (label, f)) in fields.into_iter().enumerate() {
                    let values = if f.any {
                        "any".to_string()
                    } else {
                        let v: Vec<String> = f.values.iter().map(|x| x.to_string()).collect();
                        if v.len() > 12 { format!("{} … ({} values)", v[..12].join(", "), v.len()) } else { v.join(", ") }
                    };
                    card = card.child(
                        ui::kv_row(("cron-f", i), i + 1 == n, &pal)
                            .child(ui::kv_label(label, 130., &pal))
                            .child(div().w(px(90.)).flex_none().font_family(MONO_FONT).text_size(px(13.)).child(f.raw.clone()))
                            .child(ui::kv_val(values, false).text_color(pal.text2)),
                    );
                }
                let mut next = ui::kv_card(&pal);
                let m = runs.len();
                for (i, t) in runs.iter().enumerate() {
                    let secs = (*t - now).num_seconds();
                    next = next.child(
                        ui::kv_row(("cron-n", i), i + 1 == m, &pal)
                            .child(ui::kv_val(t.format("%a %d %b %Y  %H:%M").to_string(), false))
                            .child(div().text_size(px(12.)).text_color(pal.text3).child(until(secs))),
                    );
                }
                body = body
                    .child(
                        div()
                            .mt(px(10.))
                            .px(px(18.))
                            .py(px(16.))
                            .rounded(px(8.))
                            .bg(pal.accent_soft)
                            .text_size(px(18.))
                            .font_weight(FontWeight::SEMIBOLD)
                            .text_color(pal.text)
                            .child(c.describe()),
                    )
                    .child(ui::section_label("Next runs · local time", &pal).mt(px(14.)))
                    .child(if m == 0 {
                        div().text_size(px(13.)).text_color(pal.text3).child("This schedule never runs in the next five years.")
                    } else {
                        next
                    })
                    .child(ui::section_label("Fields", &pal).mt(px(14.)))
                    .child(card);
            }
            Some(Err(e)) => body = body.child(ui::err_box(e, &pal).mt(px(4.))),
            None => {}
        }

        div()
            .flex()
            .flex_col()
            .gap(px(6.))
            .child(ui::section_label("Expression", &pal))
            .child(field_el(&self.input, true, 44., 16., window, cx))
            .child(div().flex().flex_wrap().gap(px(6.)).mt(px(4.)).children(examples))
            .child(body)
            .when(raw.trim().is_empty(), |d| {
                d.child(div().mt(px(8.)).text_size(px(13.)).text_color(pal.text3).child("Five fields: minute, hour, day of month, month, day of week. @daily, @hourly and friends work too."))
            })
    }
}
