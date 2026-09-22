use chrono::{DateTime, Datelike, FixedOffset, Local, NaiveDate, NaiveDateTime, TimeZone, Utc};
use gpui_kit::component::input::InputState;
use gpui_kit::{
    Context, Entity, IntoElement, ParentElement, Render, SharedString, Styled, Subscription,
    Window, div, prelude::FluentBuilder, px,
};

use super::*;
use crate::ui::{self, BtnKind, Tone};

pub struct DateView {
    input: Entity<InputState>,
    utc: bool,
    tz_name: String,
    _subs: Vec<Subscription>,
}

impl DateView {
    pub fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let now = Utc::now().timestamp().to_string();
        let input = line(&now, "Unix timestamp or ISO date", window, cx);
        let subs = vec![watch(&input, window, cx, |_, _, cx| cx.notify())];
        let tz_name = iana_time_zone::get_timezone().unwrap_or_else(|_| "Local".into());
        Self { input, utc: false, tz_name, _subs: subs }
    }
}

/// Parse the design's inputs: seconds or milliseconds, or common date strings.
pub fn parse(raw: &str) -> Option<DateTime<Utc>> {
    let raw = raw.trim();
    if raw.is_empty() {
        return None;
    }
    if let Ok(num) = raw.parse::<f64>() {
        let ms = if num.abs() >= 1e11 { num } else { num * 1000. };
        return Utc.timestamp_millis_opt(ms.round() as i64).single();
    }
    if let Ok(d) = DateTime::parse_from_rfc3339(raw) {
        return Some(d.with_timezone(&Utc));
    }
    if let Ok(d) = DateTime::parse_from_rfc2822(raw) {
        return Some(d.with_timezone(&Utc));
    }
    for f in ["%Y-%m-%dT%H:%M:%S%.f", "%Y-%m-%dT%H:%M", "%Y-%m-%d %H:%M:%S%.f", "%Y-%m-%d %H:%M", "%Y/%m/%d %H:%M:%S"] {
        if let Ok(n) = NaiveDateTime::parse_from_str(raw, f) {
            return Local.from_local_datetime(&n).single().map(|d| d.with_timezone(&Utc));
        }
    }
    // JavaScript treats a bare date as UTC midnight.
    if let Ok(d) = NaiveDate::parse_from_str(raw, "%Y-%m-%d") {
        return Some(Utc.from_utc_datetime(&d.and_hms_opt(0, 0, 0)?));
    }
    None
}

fn offset_label(off: &FixedOffset) -> String {
    let secs = off.local_minus_utc();
    if secs == 0 {
        return "GMT".into();
    }
    let sign = if secs < 0 { '-' } else { '+' };
    let (h, m) = (secs.abs() / 3600, secs.abs() % 3600 / 60);
    if m == 0 { format!("GMT{sign}{h}") } else { format!("GMT{sign}{h}:{m:02}") }
}

/// `toLocaleString(undefined, { dateStyle: 'full', timeStyle: 'long' })` in en-US.
fn human(d: &DateTime<FixedOffset>, utc: bool) -> String {
    let zone = if utc { "UTC".to_string() } else { offset_label(d.offset()) };
    format!("{} at {} {}", d.format("%A, %B %-d, %Y"), d.format("%-I:%M:%S %p"), zone)
}

/// `dateStyle: 'medium', timeStyle: 'medium'`, used by the JWT claims.
pub fn medium(secs: f64) -> String {
    match Local.timestamp_millis_opt((secs * 1000.) as i64).single() {
        Some(d) => d.format("%b %-d, %Y, %-I:%M:%S %p").to_string(),
        None => secs.to_string(),
    }
}

/// `Intl.RelativeTimeFormat('en', { numeric: 'auto' })`.
pub fn relative(ms: i64) -> String {
    let diff = (ms - Utc::now().timestamp_millis()) as f64 / 1000.;
    let abs = diff.abs();
    let units: [(&str, f64); 7] = [
        ("year", 31536000.),
        ("month", 2592000.),
        ("week", 604800.),
        ("day", 86400.),
        ("hour", 3600.),
        ("minute", 60.),
        ("second", 1.),
    ];
    for (u, sec) in units {
        if abs >= sec || u == "second" {
            let n = (diff / sec).round() as i64;
            return match (u, n) {
                ("second", 0) => "now".into(),
                ("day", 1) => "tomorrow".into(),
                ("day", -1) => "yesterday".into(),
                ("day", 0) => "today".into(),
                ("week" | "month" | "year", 1) => format!("next {u}"),
                ("week" | "month" | "year", -1) => format!("last {u}"),
                (_, 0) => format!("this {u}"),
                (_, n) => {
                    let unit = if n.abs() == 1 { u.to_string() } else { format!("{u}s") };
                    if n > 0 { format!("in {} {unit}", n.abs()) } else { format!("{} {unit} ago", n.abs()) }
                }
            };
        }
    }
    String::new()
}

impl DateView {
    pub fn set_input(&mut self, text: &str, window: &mut Window, cx: &mut Context<Self>) {
        set_line(&self.input, text, window, cx);
        cx.notify();
    }
}

impl Render for DateView {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let pal = Pal::get(cx);
        let raw = line_text(&self.input, cx);
        let parsed = parse(&raw);
        let utc = self.utc;
        let err = if parsed.is_none() && !raw.trim().is_empty() {
            Some("Could not read this as a timestamp or date.")
        } else {
            None
        };

        let mut rows: Vec<(&str, SharedString)> = Vec::new();
        let mut rel = String::new();
        if let Some(d) = parsed {
            let zoned: DateTime<FixedOffset> = if utc { d.fixed_offset() } else { d.with_timezone(&Local).fixed_offset() };
            let iso = if utc {
                d.format("%Y-%m-%dT%H:%M:%S%.3fZ").to_string()
            } else {
                zoned.format("%Y-%m-%dT%H:%M:%S%:z").to_string()
            };
            rows = vec![
                ("Date & time", human(&zoned, utc).into()),
                ("ISO 8601", iso.into()),
                ("RFC 7231", d.format("%a, %d %b %Y %H:%M:%S GMT").to_string().into()),
                ("Unix seconds", d.timestamp().to_string().into()),
                ("Unix milliseconds", d.timestamp_millis().to_string().into()),
                ("Day of year", format!("Day {}", zoned.ordinal()).into()),
            ];
            rel = relative(d.timestamp_millis());
        }
        let zone_name: SharedString = if utc {
            "Coordinated Universal Time".into()
        } else {
            format!("System time zone · {}", self.tz_name).into()
        };

        let on_zone = on_index(cx, |this: &mut Self, i, _, cx| {
            this.utc = i == 1;
            cx.notify();
        });
        let n = rows.len();
        let mut card = ui::kv_card(&pal);
        for (i, (label, value)) in rows.into_iter().enumerate() {
            card = card.child(ui::kv_copy_row(("dt", i), label, 150., value, false, i + 1 == n, &pal, window, cx));
        }

        div()
            .flex()
            .flex_col()
            .gap(px(6.))
            .child(ui::section_label("Configuration", &pal))
            .child(ui::setting(
                "dt-zone",
                ui::setting_icon("globe", &pal),
                "Time zone",
                Some(zone_name),
                ui::seg("dt-seg", &["Local", "UTC"], utc as usize, &pal, on_zone),
                &pal,
            ))
            .child(ui::section_label("Input", &pal).mt(px(14.)))
            .child(
                div()
                    .flex()
                    .gap(px(8.))
                    .child(field_el(&self.input, true, 44., 16., window, cx).flex_1())
                    .child(
                        ui::btn("dt-now", Some("clock"), "Now", BtnKind::Accent, &pal, cx.listener(|this, _, window, cx| {
                            set_line(&this.input, &Utc::now().timestamp().to_string(), window, cx);
                            cx.notify();
                        }))
                        .h(px(44.))
                        .px(px(18.)),
                    ),
            )
            .when_some(err, |d, e| d.child(ui::err_box(e, &pal).mt(px(4.))))
            .child(
                div()
                    .flex()
                    .items_center()
                    .justify_between()
                    .mt(px(14.))
                    .child(ui::section_label("Output", &pal))
                    .when(!rel.is_empty(), |d| d.child(ui::badge(rel, Tone::Info, &pal))),
            )
            .child(card)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_inputs() {
        assert_eq!(parse("0").unwrap().timestamp(), 0);
        assert_eq!(parse("1700000000000").unwrap().timestamp(), 1_700_000_000);
        assert_eq!(parse("2024-01-02").unwrap().to_rfc3339(), "2024-01-02T00:00:00+00:00");
        assert!(parse("2024-01-02T03:04:05Z").is_some());
        assert!(parse("not a date").is_none());
    }

    #[test]
    fn relative_words() {
        let now = Utc::now().timestamp_millis();
        assert_eq!(relative(now), "now");
        assert_eq!(relative(now - 2 * 3600 * 1000), "2 hours ago");
        assert_eq!(relative(now + 86400 * 1000), "tomorrow");
    }
}
