//! Five-field cron expressions: validation, a plain-English reading and the
//! next run times.

use chrono::{DateTime, Datelike, Duration, NaiveDate, TimeZone};

const MONTHS: [&str; 12] = ["January", "February", "March", "April", "May", "June", "July", "August", "September", "October", "November", "December"];
const DAYS: [&str; 7] = ["Sunday", "Monday", "Tuesday", "Wednesday", "Thursday", "Friday", "Saturday"];

#[derive(Debug, Clone)]
pub struct Field {
    /// Allowed values, sorted.
    pub values: Vec<u32>,
    /// Written as `*` (or `?`): no restriction.
    pub any: bool,
    /// The expression as typed.
    pub raw: String,
}

#[derive(Debug, Clone)]
pub struct Cron {
    pub minute: Field,
    pub hour: Field,
    pub dom: Field,
    pub month: Field,
    pub dow: Field,
}

fn name_value(s: &str, names: Option<&[&str]>) -> Option<u32> {
    if let Ok(n) = s.parse() {
        return Some(n);
    }
    let names = names?;
    let s = s.to_lowercase();
    names.iter().position(|n| n[..3].to_lowercase() == s).map(|i| i as u32)
}

fn field(raw: &str, lo: u32, hi: u32, names: Option<&[&str]>, name_base: u32, what: &str) -> Result<Field, String> {
    let any = raw == "*" || raw == "?";
    let mut values = Vec::new();
    for part in raw.split(',') {
        let (range, step) = match part.split_once('/') {
            Some((r, s)) => (r, s.parse::<u32>().ok().filter(|s| *s > 0).ok_or(format!("\"{s}\" is not a valid step in the {what} field"))?),
            None => (part, 1),
        };
        let (a, b) = if range == "*" || range == "?" {
            (lo, hi)
        } else if let Some((a, b)) = range.split_once('-') {
            let a = name_value(a, names).map(|v| if names.is_some() && a.parse::<u32>().is_err() { v + name_base } else { v });
            let b = name_value(b, names).map(|v| if names.is_some() && b.parse::<u32>().is_err() { v + name_base } else { v });
            (a.ok_or(format!("\"{range}\" is not a valid range in the {what} field"))?, b.ok_or(format!("\"{range}\" is not a valid range in the {what} field"))?)
        } else {
            let v = name_value(range, names)
                .map(|v| if names.is_some() && range.parse::<u32>().is_err() { v + name_base } else { v })
                .ok_or(format!("\"{range}\" is not a valid value in the {what} field"))?;
            // "5/15" means from 5 to the end, every 15.
            (v, if part.contains('/') { hi } else { v })
        };
        if a < lo || b > hi || a > b {
            return Err(format!("\"{range}\" is outside {lo}–{hi} in the {what} field"));
        }
        values.extend((a..=b).step_by(step as usize));
    }
    values.sort_unstable();
    values.dedup();
    Ok(Field { values, any, raw: raw.to_string() })
}

pub fn parse(expr: &str) -> Result<Cron, String> {
    let expr = expr.trim();
    let expanded = match expr.to_lowercase().as_str() {
        "@yearly" | "@annually" => "0 0 1 1 *".to_string(),
        "@monthly" => "0 0 1 * *".to_string(),
        "@weekly" => "0 0 * * 0".to_string(),
        "@daily" | "@midnight" => "0 0 * * *".to_string(),
        "@hourly" => "0 * * * *".to_string(),
        _ => expr.to_string(),
    };
    let parts: Vec<&str> = expanded.split_whitespace().collect();
    if parts.len() != 5 {
        return Err(format!("Expected 5 fields (minute hour day month weekday), found {}", parts.len()));
    }
    let mut dow = field(parts[4], 0, 7, Some(&DAYS), 0, "weekday")?;
    // 7 is Sunday too.
    if dow.values.contains(&7) {
        dow.values.retain(|v| *v != 7);
        if !dow.values.contains(&0) {
            dow.values.insert(0, 0);
        }
    }
    Ok(Cron {
        minute: field(parts[0], 0, 59, None, 0, "minute")?,
        hour: field(parts[1], 0, 23, None, 0, "hour")?,
        dom: field(parts[2], 1, 31, None, 0, "day-of-month")?,
        month: field(parts[3], 1, 12, Some(&MONTHS), 1, "month")?,
        dow,
    })
}

impl Cron {
    fn day_matches(&self, d: NaiveDate) -> bool {
        if !self.month.values.contains(&d.month()) {
            return false;
        }
        let dom = self.dom.values.contains(&d.day());
        let dow = self.dow.values.contains(&d.weekday().num_days_from_sunday());
        // Classic cron: when both are restricted, either may match.
        match (self.dom.any, self.dow.any) {
            (true, true) => true,
            (false, true) => dom,
            (true, false) => dow,
            (false, false) => dom || dow,
        }
    }

    /// The next `n` run times strictly after `after`, looking up to five years ahead.
    pub fn next_runs<Tz: TimeZone>(&self, after: &DateTime<Tz>, n: usize) -> Vec<DateTime<Tz>> {
        let tz = after.timezone();
        let mut out = Vec::new();
        let start = after.naive_local();
        let mut day = start.date();
        for _ in 0..(366 * 5) {
            if self.day_matches(day) {
                for &h in &self.hour.values {
                    for &m in &self.minute.values {
                        let Some(t) = day.and_hms_opt(h, m, 0) else { continue };
                        if t <= start {
                            continue;
                        }
                        // Skip times that do not exist locally (DST gaps).
                        if let Some(dt) = tz.from_local_datetime(&t).earliest() {
                            out.push(dt);
                            if out.len() == n {
                                return out;
                            }
                        }
                    }
                }
            }
            day += Duration::days(1);
        }
        out
    }

    /// A plain-English reading, e.g. "At 09:30, Monday through Friday".
    pub fn describe(&self) -> String {
        let mut s = time_phrase(&self.minute, &self.hour);
        if !self.dom.any {
            s.push_str(&format!(", on day {} of the month", list(&self.dom, |v| v.to_string())));
        }
        if !self.dow.any {
            s.push_str(if self.dom.any { ", " } else { " and " });
            s.push_str(&list(&self.dow, |v| DAYS[v as usize].to_string()));
        }
        if !self.month.any {
            s.push_str(&format!(", in {}", list(&self.month, |v| MONTHS[v as usize - 1].to_string())));
        }
        s
    }
}

/// "1, 2 and 5", "Monday through Friday", or "every 15th" style lists.
fn list(f: &Field, name: impl Fn(u32) -> String) -> String {
    let v = &f.values;
    if v.len() > 2 && v.windows(2).all(|w| w[1] == w[0] + 1) {
        return format!("{} through {}", name(v[0]), name(*v.last().unwrap()));
    }
    let names: Vec<String> = v.iter().map(|x| name(*x)).collect();
    match names.len() {
        0 => String::new(),
        1 => names[0].clone(),
        n => format!("{} and {}", names[..n - 1].join(", "), names[n - 1]),
    }
}

fn step_of(f: &Field) -> Option<u32> {
    f.raw.strip_prefix("*/").and_then(|s| s.parse().ok())
}

fn time_phrase(m: &Field, h: &Field) -> String {
    let single = |f: &Field| (f.values.len() == 1).then(|| f.values[0]);
    match (single(m), single(h)) {
        (Some(mm), Some(hh)) => format!("At {hh:02}:{mm:02}"),
        (Some(mm), None) if h.any => if mm == 0 { "Every hour, on the hour".into() } else { format!("At minute {mm} past every hour") },
        (Some(mm), None) => match step_of(h) {
            Some(s) => format!("At minute {mm} past every {s} hours"),
            None => {
                if m.values == [0] {
                    format!("At {}", list(h, |v| format!("{v:02}:00")))
                } else {
                    format!("At minute {mm} past hour {}", list(h, |v| v.to_string()))
                }
            }
        },
        _ if m.any && h.any => "Every minute".into(),
        _ if h.any => match step_of(m) {
            Some(s) => format!("Every {s} minutes"),
            None => format!("At minutes {} past every hour", list(m, |v| v.to_string())),
        },
        _ => {
            let mins = match step_of(m) {
                Some(s) => format!("Every {s} minutes"),
                None if m.any => "Every minute".into(),
                None => format!("At minutes {}", list(m, |v| v.to_string())),
            };
            format!("{mins} during hour {}", list(h, |v| format!("{v:02}")))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;

    #[test]
    fn descriptions() {
        let d = |e: &str| parse(e).unwrap().describe();
        assert_eq!(d("30 9 * * 1-5"), "At 09:30, Monday through Friday");
        assert_eq!(d("*/15 * * * *"), "Every 15 minutes");
        assert_eq!(d("0 * * * *"), "Every hour, on the hour");
        assert_eq!(d("0 0 1 jan *"), "At 00:00, on day 1 of the month, in January");
        assert_eq!(d("@weekly"), "At 00:00, Sunday");
        assert_eq!(d("0 9,17 * * sat,sun"), "At 09:00 and 17:00, Sunday and Saturday");
        assert_eq!(d("5 */2 * * *"), "At minute 5 past every 2 hours");
    }

    #[test]
    fn next_runs_and_errors() {
        let c = parse("30 9 * * 1-5").unwrap();
        let from = Utc.with_ymd_and_hms(2026, 9, 25, 10, 0, 0).unwrap(); // a Friday
        let runs = c.next_runs(&from, 3);
        assert_eq!(runs[0], Utc.with_ymd_and_hms(2026, 9, 28, 9, 30, 0).unwrap());
        assert_eq!(runs[2], Utc.with_ymd_and_hms(2026, 9, 30, 9, 30, 0).unwrap());
        let leap = parse("0 0 29 2 *").unwrap().next_runs(&from, 1);
        assert_eq!(leap[0].year(), 2028);
        assert!(parse("60 * * * *").is_err());
        assert!(parse("* * *").is_err());
        assert!(parse("*/0 * * * *").is_err());
        assert_eq!(parse("0 0 * * 7").unwrap().dow.values, [0]);
    }
}
