use chrono::{
    DateTime, Datelike, FixedOffset, Local, NaiveDate, NaiveDateTime, NaiveTime, TimeZone, Utc,
};
use gpui_kit::component::input::InputState;
use gpui_kit::{
    Context, Entity, IntoElement, ParentElement, Render, SharedString, Styled, Subscription,
    Window, div, prelude::FluentBuilder, px,
};

use super::*;
use crate::ui::{self, BtnKind, Tone};

const TIMESTAMP_HINT: &str = "Unix timestamp (seconds or milliseconds) or ISO date";
const DATE_HINT: &str = "2026-09-23 14:30, 23 Sep 2026 2:30 PM, 23/09/2026…";
/// How "Now" and mode switches write a date back into the input.
const DATE_OUT: &str = "%Y-%m-%d %H:%M:%S";

pub struct DateView {
    input: Entity<InputState>,
    utc: bool,
    /// `false`: Unix timestamp → date. `true`: date → Unix timestamp.
    to_unix: bool,
    tz_name: String,
    _subs: Vec<Subscription>,
}

impl DateView {
    pub fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let now = Utc::now().timestamp().to_string();
        let input = line(&now, TIMESTAMP_HINT, window, cx);
        let subs = vec![watch(&input, window, cx, |_, _, cx| cx.notify())];
        let tz_name = iana_time_zone::get_timezone().unwrap_or_else(|_| "Local".into());
        Self { input, utc: false, to_unix: false, tz_name, _subs: subs }
    }

    pub fn set_input(&mut self, text: &str, window: &mut Window, cx: &mut Context<Self>) {
        let is_number = text.trim().parse::<f64>().is_ok();
        if self.to_unix == is_number {
            self.set_mode(!is_number, window, cx);
        }
        set_line(&self.input, text, window, cx);
        cx.notify();
    }

    /// Switch direction, carrying the current value across so nothing is lost.
    pub fn set_mode(&mut self, to_unix: bool, window: &mut Window, cx: &mut Context<Self>) {
        if to_unix == self.to_unix {
            return;
        }
        let current = parse(&line_text(&self.input, cx), self.utc);
        self.to_unix = to_unix;
        self.input.update(cx, |s, cx| {
            s.set_placeholder(if to_unix { DATE_HINT } else { TIMESTAMP_HINT }, window, cx)
        });
        if let Some(d) = current {
            let text = if to_unix { self.format_in_zone(&d, DATE_OUT) } else { d.timestamp().to_string() };
            set_line(&self.input, &text, window, cx);
        }
        cx.notify();
    }

    fn format_in_zone(&self, d: &DateTime<Utc>, fmt: &str) -> String {
        if self.utc { d.format(fmt).to_string() } else { d.with_timezone(&Local).format(fmt).to_string() }
    }

    fn now(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let now = Utc::now();
        let text = if self.to_unix { self.format_in_zone(&now, DATE_OUT) } else { now.timestamp().to_string() };
        set_line(&self.input, &text, window, cx);
        cx.notify();
    }
}

// ------------------------------------------------------------ parsing

/// Parse a Unix timestamp (seconds or milliseconds) or a human-readable date.
/// Dates without an explicit offset are read in UTC when `utc`, else local time.
pub fn parse(raw: &str, utc: bool) -> Option<DateTime<Utc>> {
    let raw = raw.trim();
    if raw.is_empty() {
        return None;
    }
    if let Ok(num) = raw.parse::<f64>() {
        let ms = if num.abs() >= 1e11 { num } else { num * 1000. };
        return Utc.timestamp_millis_opt(ms.round() as i64).single();
    }
    parse_date(raw, utc)
}

/// Accepts ISO 8601 / RFC 3339, RFC 2822, numeric dates (day-first, falling back
/// to month-first when the day-first reading is impossible), month names, 12/24h
/// times and a trailing offset such as `Z`, `UTC`, `GMT+1`, `+05:30`.
pub fn parse_date(raw: &str, utc: bool) -> Option<DateTime<Utc>> {
    if let Ok(d) = DateTime::parse_from_rfc3339(raw) {
        return Some(d.with_timezone(&Utc));
    }
    if let Ok(d) = DateTime::parse_from_rfc2822(raw) {
        return Some(d.with_timezone(&Utc));
    }
    let (body, offset) = split_offset(raw.trim());
    let naive = parse_naive(&normalize(body))?;
    let resolved = match offset {
        Some(secs) => FixedOffset::east_opt(secs)?.from_local_datetime(&naive).single()?.with_timezone(&Utc),
        None if utc => Utc.from_utc_datetime(&naive),
        None => Local.from_local_datetime(&naive).earliest()?.with_timezone(&Utc),
    };
    Some(resolved)
}

/// Commas, " at ", ordinal suffixes and an ISO `T` separator all become spaces.
fn normalize(s: &str) -> String {
    let mut s = s.replace(',', " ");
    // "2026-09-23T14:30" → "2026-09-23 14:30"
    let b = s.as_bytes();
    if b.len() > 11 && b[10] == b'T' && b[..10].iter().filter(|c| c.is_ascii_digit()).count() == 8 {
        s.replace_range(10..11, " ");
    }
    let words: Vec<String> = s
        .split_whitespace()
        .filter(|w| !w.eq_ignore_ascii_case("at") && !w.eq_ignore_ascii_case("on"))
        .map(|w| {
            // "23rd" → "23"
            let lower = w.to_ascii_lowercase();
            for suffix in ["st", "nd", "rd", "th"] {
                if let Some(num) = lower.strip_suffix(suffix) {
                    if !num.is_empty() && num.chars().all(|c| c.is_ascii_digit()) {
                        return num.to_string();
                    }
                }
            }
            w.to_string()
        })
        .collect();
    words.join(" ")
}

const DATE_FORMATS: &[&str] = &[
    "%Y-%m-%d", "%Y/%m/%d", "%Y.%m.%d",
    "%d/%m/%Y", "%m/%d/%Y", "%d-%m-%Y", "%m-%d-%Y", "%d.%m.%Y",
    "%d %b %Y", "%d %B %Y", "%b %d %Y", "%B %d %Y",
    "%a %d %b %Y", "%A %d %B %Y", "%a %b %d %Y", "%A %B %d %Y",
];

const TIME_FORMATS: &[&str] = &[
    "%H:%M:%S%.f", "%H:%M", "%I:%M:%S %p", "%I:%M %p", "%I %p", "%I:%M:%S%p", "%I:%M%p", "%I%p",
];

fn parse_naive(s: &str) -> Option<NaiveDateTime> {
    for d in DATE_FORMATS {
        if let Ok(date) = NaiveDate::parse_from_str(s, d) {
            return Some(date.and_time(NaiveTime::MIN));
        }
        for t in TIME_FORMATS {
            if let Ok(dt) = NaiveDateTime::parse_from_str(s, &format!("{d} {t}")) {
                return Some(dt);
            }
        }
    }
    None
}

/// Split a trailing offset off the text: `Z`, `UTC`, `GMT`, `GMT+5:30`, `+05:30`, `-0800`.
fn split_offset(s: &str) -> (&str, Option<i32>) {
    // A separate last word: "… UTC", "… GMT+1", "… +05:30".
    if let Some((head, last)) = s.rsplit_once(char::is_whitespace) {
        let upper = last.to_ascii_uppercase();
        let rest = upper.strip_prefix("UTC").or_else(|| upper.strip_prefix("GMT"));
        match rest {
            Some("") => return (head, Some(0)),
            Some(r) => {
                if let Some(secs) = offset_secs(r) {
                    return (head, Some(secs));
                }
            }
            None if last.starts_with(['+', '-']) => {
                if let Some(secs) = offset_secs(last) {
                    return (head, Some(secs));
                }
            }
            None => {}
        }
    }
    // Attached to the time: "14:30Z", "14:30:00+05:30", "14:30-0800".
    if let Some(head) = s.strip_suffix(['Z', 'z']) {
        if head.ends_with(|c: char| c.is_ascii_digit()) {
            return (head, Some(0));
        }
    }
    for len in [6, 5, 3] {
        if s.len() > len && s.is_char_boundary(s.len() - len) {
            let (head, tail) = s.split_at(s.len() - len);
            if tail.starts_with(['+', '-']) && head.ends_with(|c: char| c.is_ascii_digit()) && head.contains(':') {
                if let Some(secs) = offset_secs(tail) {
                    return (head, Some(secs));
                }
            }
        }
    }
    (s, None)
}

/// "+5", "+05:30", "-0800" → seconds east of UTC.
fn offset_secs(s: &str) -> Option<i32> {
    let (sign, digits) = match s.as_bytes().first()? {
        b'+' => (1, &s[1..]),
        b'-' => (-1, &s[1..]),
        _ => return None,
    };
    let (h, m) = match digits.split_once(':') {
        Some((h, m)) => (h, m),
        None if digits.len() == 4 => digits.split_at(2),
        None => (digits, "0"),
    };
    if h.is_empty() || h.len() > 2 || !h.chars().all(|c| c.is_ascii_digit()) || !m.chars().all(|c| c.is_ascii_digit()) {
        return None;
    }
    let (h, m): (i32, i32) = (h.parse().ok()?, m.parse().ok()?);
    (h <= 14 && m < 60).then_some(sign * (h * 3600 + m * 60))
}

// ------------------------------------------------------------ formatting

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

// ------------------------------------------------------------ view

impl Render for DateView {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let pal = Pal::get(cx);
        let raw = line_text(&self.input, cx);
        let utc = self.utc;
        let to_unix = self.to_unix;
        let parsed = parse(&raw, utc);
        let err = (parsed.is_none() && !raw.trim().is_empty()).then(|| {
            if to_unix {
                "Could not read this as a date. Try 2026-09-23 14:30, 23 Sep 2026 2:30 PM or 23/09/2026."
            } else {
                "Could not read this as a timestamp or date."
            }
        });

        let mut rows: Vec<(&str, SharedString)> = Vec::new();
        let mut rel = String::new();
        if let Some(d) = parsed {
            let zoned: DateTime<FixedOffset> = if utc { d.fixed_offset() } else { d.with_timezone(&Local).fixed_offset() };
            let iso = if utc {
                d.format("%Y-%m-%dT%H:%M:%S%.3fZ").to_string()
            } else {
                zoned.format("%Y-%m-%dT%H:%M:%S%:z").to_string()
            };
            let secs: SharedString = d.timestamp().to_string().into();
            let millis: SharedString = d.timestamp_millis().to_string().into();
            rows = if to_unix {
                vec![
                    ("Unix seconds", secs),
                    ("Unix milliseconds", millis),
                    ("Read as", human(&zoned, utc).into()),
                    ("ISO 8601", iso.into()),
                    ("RFC 7231", d.format("%a, %d %b %Y %H:%M:%S GMT").to_string().into()),
                ]
            } else {
                vec![
                    ("Date & time", human(&zoned, utc).into()),
                    ("ISO 8601", iso.into()),
                    ("RFC 7231", d.format("%a, %d %b %Y %H:%M:%S GMT").to_string().into()),
                    ("Unix seconds", secs),
                    ("Unix milliseconds", millis),
                    ("Day of year", format!("Day {}", zoned.ordinal()).into()),
                ]
            };
            rel = relative(d.timestamp_millis());
        }
        let zone_name: SharedString = match (to_unix, utc) {
            (false, true) => "Coordinated Universal Time".into(),
            (false, false) => format!("System time zone · {}", self.tz_name).into(),
            (true, true) => "Dates without an offset are read as UTC".into(),
            (true, false) => format!("Dates without an offset are read in {}", self.tz_name).into(),
        };

        let on_mode = on_index(cx, |this: &mut Self, i, w, cx| this.set_mode(i == 1, w, cx));
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
                "dt-mode",
                ui::setting_icon("conv", &pal),
                "Conversion",
                Some(if to_unix { "Readable date to Unix timestamp" } else { "Unix timestamp to readable date" }.into()),
                ui::seg("dt-mode-seg", &["Timestamp → Date", "Date → Timestamp"], to_unix as usize, &pal, on_mode),
                &pal,
            ))
            .child(ui::setting(
                "dt-zone",
                ui::setting_icon("globe", &pal),
                "Time zone",
                Some(zone_name),
                ui::seg("dt-seg", &["Local", "UTC"], utc as usize, &pal, on_zone),
                &pal,
            ))
            .child(ui::section_label(if to_unix { "Date" } else { "Timestamp" }, &pal).mt(px(14.)))
            .child(
                div()
                    .flex()
                    .gap(px(8.))
                    .child(field_el(&self.input, true, 44., 16., window, cx).flex_1())
                    .child(
                        ui::btn("dt-now", Some("clock"), "Now", BtnKind::Accent, &pal, cx.listener(|this, _, window, cx| {
                            this.now(window, cx)
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

    fn utc(s: &str) -> Option<String> {
        parse(s, true).map(|d| d.format("%Y-%m-%d %H:%M:%S").to_string())
    }

    #[test]
    fn timestamps() {
        assert_eq!(parse("0", true).unwrap().timestamp(), 0);
        assert_eq!(parse("1700000000000", true).unwrap().timestamp(), 1_700_000_000);
        assert!(parse("not a date", true).is_none());
    }

    #[test]
    fn readable_dates_to_unix() {
        assert_eq!(parse("2026-09-23 14:30", true).unwrap().timestamp(), 1_790_173_800);
        assert_eq!(utc("2026-09-23").as_deref(), Some("2026-09-23 00:00:00"));
        assert_eq!(utc("2026-09-23T14:30:15").as_deref(), Some("2026-09-23 14:30:15"));
        assert_eq!(utc("2026/09/23 14:30").as_deref(), Some("2026-09-23 14:30:00"));
        assert_eq!(utc("23 Sep 2026 2:30 PM").as_deref(), Some("2026-09-23 14:30:00"));
        assert_eq!(utc("September 23rd, 2026 at 2:30pm").as_deref(), Some("2026-09-23 14:30:00"));
        assert_eq!(utc("Sep 23, 2026, 2:30:05 PM").as_deref(), Some("2026-09-23 14:30:05"));
        assert_eq!(utc("Wednesday, September 23, 2026 at 2:30:05 PM").as_deref(), Some("2026-09-23 14:30:05"));
        assert_eq!(utc("23.09.2026 14:30").as_deref(), Some("2026-09-23 14:30:00"));
        // Numeric dates are day-first unless that is impossible.
        assert_eq!(utc("03/09/2026").as_deref(), Some("2026-09-03 00:00:00"));
        assert_eq!(utc("09/23/2026").as_deref(), Some("2026-09-23 00:00:00"));
    }

    #[test]
    fn explicit_offsets_win_over_the_zone_setting() {
        let want = Some("2026-09-23 09:00:00".to_string());
        for s in [
            "2026-09-23T14:30:00+05:30",
            "2026-09-23 14:30 +05:30",
            "2026-09-23 14:30+0530",
            "2026-09-23 14:30 GMT+5:30",
            "Wed, 23 Sep 2026 14:30:00 +0530",
        ] {
            assert_eq!(parse(s, false).map(|d| d.format("%Y-%m-%d %H:%M:%S").to_string()), want, "{s}");
        }
        assert_eq!(utc("2026-09-23 14:30 UTC").as_deref(), Some("2026-09-23 14:30:00"));
        assert_eq!(utc("2026-09-23 14:30Z").as_deref(), Some("2026-09-23 14:30:00"));
        // The app's own "Date & time" output parses back.
        assert_eq!(utc("Wednesday, September 23, 2026 at 3:30:00 PM GMT+1").as_deref(), Some("2026-09-23 14:30:00"));
    }

    #[test]
    fn zone_setting_applies_to_naive_dates() {
        let local = parse("2026-09-23 14:30", false).unwrap();
        let expected = Local.from_local_datetime(&NaiveDate::from_ymd_opt(2026, 9, 23).unwrap().and_hms_opt(14, 30, 0).unwrap()).earliest().unwrap();
        assert_eq!(local, expected.with_timezone(&Utc));
    }

    #[test]
    fn relative_words() {
        let now = Utc::now().timestamp_millis();
        assert_eq!(relative(now), "now");
        assert_eq!(relative(now - 2 * 3600 * 1000), "2 hours ago");
        assert_eq!(relative(now + 86400 * 1000), "tomorrow");
    }
}
