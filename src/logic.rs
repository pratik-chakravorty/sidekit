//! Pure conversion logic behind every tool. No UI types in here.

use base64::Engine as _;
use serde_json::Value;

// ---------------------------------------------------------------- helpers

pub fn thousands(n: u64) -> String {
    let s = n.to_string();
    group(&s, 3, ",")
}

/// `C.plural`: "1 byte", "2,048 bytes".
pub fn plural(n: usize, word: &str) -> String {
    let w = if n == 1 { word.to_string() } else if word == "match" { "matches".into() } else { format!("{word}s") };
    format!("{} {}", thousands(n as u64), w)
}

/// Group digits from the right: `group("48879", 3, ",") == "48,879"`.
pub fn group(s: &str, size: usize, sep: &str) -> String {
    let chars: Vec<char> = s.chars().collect();
    let mut parts = Vec::new();
    let mut end = chars.len();
    while end > 0 {
        let start = end.saturating_sub(size);
        parts.push(chars[start..end].iter().collect::<String>());
        end = start;
    }
    parts.reverse();
    parts.join(sep)
}

pub fn random_u32() -> u32 {
    let mut b = [0u8; 4];
    getrandom::fill(&mut b).expect("OS randomness");
    u32::from_le_bytes(b)
}

/// Uniform-ish index below `n`, as the design does (`rand % n`).
pub fn rand_below(n: usize) -> usize {
    (random_u32() as usize) % n.max(1)
}

// ---------------------------------------------------------------- JSON

pub enum Indent {
    Spaces(usize),
    Tab,
    Minified,
}

pub fn sort_keys(v: Value) -> Value {
    match v {
        Value::Object(map) => {
            let mut entries: Vec<(String, Value)> = map.into_iter().collect();
            entries.sort_by(|a, b| a.0.cmp(&b.0));
            Value::Object(entries.into_iter().map(|(k, v)| (k, sort_keys(v))).collect())
        }
        Value::Array(a) => Value::Array(a.into_iter().map(sort_keys).collect()),
        other => other,
    }
}

pub fn count_nodes(v: &Value) -> usize {
    match v {
        Value::Object(m) => 1 + m.values().map(count_nodes).sum::<usize>(),
        Value::Array(a) => 1 + a.iter().map(count_nodes).sum::<usize>(),
        _ => 1,
    }
}

pub fn to_json(v: &Value, indent: &Indent) -> String {
    match indent {
        Indent::Minified => serde_json::to_string(v).unwrap_or_default(),
        Indent::Spaces(n) => pretty(v, &" ".repeat(*n)),
        Indent::Tab => pretty(v, "\t"),
    }
}

fn pretty(v: &Value, unit: &str) -> String {
    use serde::Serialize;
    let mut buf = Vec::new();
    let fmt = serde_json::ser::PrettyFormatter::with_indent(unit.as_bytes());
    let mut ser = serde_json::Serializer::with_formatter(&mut buf, fmt);
    v.serialize(&mut ser).ok();
    String::from_utf8(buf).unwrap_or_default()
}

pub struct JsonFmt {
    pub out: String,
    pub status: String,
}

pub fn format_json(input: &str, indent: Indent, sort: bool) -> Result<JsonFmt, String> {
    let mut v: Value = serde_json::from_str(input).map_err(|e| json_err(&e))?;
    if sort {
        v = sort_keys(v);
    }
    let out = to_json(&v, &indent);
    let status = format!(
        "Valid JSON · {} · {}",
        plural(count_nodes(&v), "node"),
        plural(out.len(), "byte")
    );
    Ok(JsonFmt { out, status })
}

pub fn json_err(e: &serde_json::Error) -> String {
    let msg = e.to_string();
    // serde says "... at line 1 column 5"; keep it but capitalize the start.
    let mut c = msg.chars();
    match c.next() {
        Some(f) => f.to_uppercase().collect::<String>() + c.as_str(),
        None => msg,
    }
}

// ---------------------------------------------------------------- YAML

fn yaml_scalar(x: &Value) -> String {
    match x {
        Value::Null => "null".into(),
        Value::Array(_) => "[]".into(),
        Value::Object(_) => "{}".into(),
        Value::Bool(b) => b.to_string(),
        Value::Number(n) => n.to_string(),
        Value::String(s) => {
            let lower = s.to_lowercase();
            let reserved = matches!(lower.as_str(), "true" | "false" | "null" | "yes" | "no" | "on" | "off" | "~");
            let needs_quote = s.is_empty()
                || s.starts_with(char::is_whitespace)
                || s.ends_with(char::is_whitespace)
                || s.chars().any(|c| ":#[]{},&*!|>'\"%@`".contains(c))
                || s.starts_with('-')
                || reserved
                || s.trim_start_matches(['+', '-']).starts_with(|c: char| c.is_ascii_digit() || c == '.')
                || s.contains('\n');
            if needs_quote { serde_json::to_string(s).unwrap_or_default() } else { s.clone() }
        }
    }
}

fn complex(x: &Value) -> bool {
    match x {
        Value::Object(m) => !m.is_empty(),
        Value::Array(a) => !a.is_empty(),
        _ => false,
    }
}

pub fn to_yaml(v: &Value, ind: usize, step: usize) -> String {
    let pad = " ".repeat(ind * step);
    match v {
        Value::Array(a) => {
            if a.is_empty() {
                return format!("{pad}[]");
            }
            a.iter()
                .map(|x| {
                    if complex(x) {
                        format!("{pad}- {}", to_yaml(x, ind + 1, step).trim_start())
                    } else {
                        format!("{pad}- {}", yaml_scalar(x))
                    }
                })
                .collect::<Vec<_>>()
                .join("\n")
        }
        Value::Object(m) => {
            if m.is_empty() {
                return format!("{pad}{{}}");
            }
            m.iter()
                .map(|(k, x)| {
                    let plain = k.chars().next().is_some_and(|c| c.is_ascii_alphabetic() || c == '_')
                        && k.chars().all(|c| c.is_ascii_alphanumeric() || "_.-".contains(c));
                    let key = if plain { k.clone() } else { serde_json::to_string(k).unwrap_or_default() };
                    if complex(x) {
                        format!("{pad}{key}:\n{}", to_yaml(x, ind + 1, step))
                    } else {
                        format!("{pad}{key}: {}", yaml_scalar(x))
                    }
                })
                .collect::<Vec<_>>()
                .join("\n")
        }
        other => format!("{pad}{}", yaml_scalar(other)),
    }
}

// ---------------------------------------------------------------- number base

pub fn parse_base(s: &str, base: u32) -> Result<Option<u128>, String> {
    let mut s: String = s.trim().to_lowercase().chars().filter(|c| !c.is_whitespace() && *c != '_' && *c != ',').collect();
    let prefix = match base {
        16 => "0x",
        2 => "0b",
        8 => "0o",
        _ => "",
    };
    if !prefix.is_empty() {
        if let Some(rest) = s.strip_prefix(prefix) {
            s = rest.to_string();
        }
    }
    if s.is_empty() {
        return Ok(None);
    }
    let mut n: u128 = 0;
    for ch in s.chars() {
        let d = ch.to_digit(16).filter(|d| *d < base).ok_or_else(|| format!("\"{ch}\" is not a valid base-{base} digit"))?;
        n = n
            .checked_mul(base as u128)
            .and_then(|n| n.checked_add(d as u128))
            .ok_or_else(|| "This number is too large (max 128 bits)".to_string())?;
    }
    Ok(Some(n))
}

pub fn to_radix(mut n: u128, radix: u32) -> String {
    if n == 0 {
        return "0".into();
    }
    let digits = b"0123456789ABCDEF";
    let mut out = Vec::new();
    while n > 0 {
        out.push(digits[(n % radix as u128) as usize]);
        n /= radix as u128;
    }
    out.reverse();
    String::from_utf8(out).unwrap()
}

// ---------------------------------------------------------------- codecs

pub fn b64_enc(s: &str) -> String {
    base64::engine::general_purpose::STANDARD.encode(s.as_bytes())
}

pub fn b64_dec(s: &str) -> Result<String, ()> {
    let clean: String = s.chars().filter(|c| !c.is_whitespace()).collect();
    let bytes = base64::engine::general_purpose::STANDARD
        .decode(clean.as_bytes())
        .or_else(|_| base64::engine::general_purpose::STANDARD_NO_PAD.decode(clean.trim_end_matches('=').as_bytes()))
        .map_err(|_| ())?;
    String::from_utf8(bytes).map_err(|_| ())
}

pub fn b64url_decode(p: &str) -> Result<Vec<u8>, ()> {
    base64::engine::general_purpose::URL_SAFE_NO_PAD.decode(p.trim_end_matches('=')).map_err(|_| ())
}

pub fn b64url_json(v: &Value) -> String {
    base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(serde_json::to_string(v).unwrap_or_default())
}

/// `encodeURIComponent`.
pub fn url_enc(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for b in s.bytes() {
        if b.is_ascii_alphanumeric() || b"-_.!~*'()".contains(&b) {
            out.push(b as char);
        } else {
            out.push_str(&format!("%{b:02X}"));
        }
    }
    out
}

/// `decodeURIComponent`: rejects malformed escapes and invalid UTF-8.
pub fn url_dec(s: &str) -> Result<String, ()> {
    let bytes = s.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%' {
            let hex = s.get(i + 1..i + 3).ok_or(())?;
            out.push(u8::from_str_radix(hex, 16).map_err(|_| ())?);
            i += 3;
        } else {
            out.push(bytes[i]);
            i += 1;
        }
    }
    String::from_utf8(out).map_err(|_| ())
}

pub fn html_enc(s: &str) -> String {
    s.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;").replace('"', "&quot;").replace('\'', "&#39;")
}

pub fn html_dec(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut rest = s;
    while let Some(i) = rest.find('&') {
        out.push_str(&rest[..i]);
        let tail = &rest[i..];
        let end = tail[1..].find(';').map(|e| e + 1);
        let decoded = end.and_then(|e| {
            let ent = &tail[1..e];
            let ch = if let Some(hex) = ent.strip_prefix("#x").or_else(|| ent.strip_prefix("#X")) {
                u32::from_str_radix(hex, 16).ok().and_then(char::from_u32).map(String::from)
            } else if let Some(dec) = ent.strip_prefix('#') {
                dec.parse::<u32>().ok().and_then(char::from_u32).map(String::from)
            } else {
                match ent.to_lowercase().as_str() {
                    "amp" => Some("&"),
                    "lt" => Some("<"),
                    "gt" => Some(">"),
                    "quot" => Some("\""),
                    "apos" => Some("'"),
                    "nbsp" => Some("\u{a0}"),
                    "copy" => Some("©"),
                    "reg" => Some("®"),
                    "hellip" => Some("…"),
                    "mdash" => Some("—"),
                    "ndash" => Some("–"),
                    _ => None,
                }
                .map(String::from)
            };
            ch.map(|c| (c, e + 1))
        });
        match decoded {
            Some((c, len)) => {
                out.push_str(&c);
                rest = &tail[len..];
            }
            None => {
                out.push('&');
                rest = &tail[1..];
            }
        }
    }
    out.push_str(rest);
    out
}

/// `JSON.stringify(s).slice(1, -1)`.
pub fn esc_enc(s: &str) -> String {
    let j = serde_json::to_string(s).unwrap_or_default();
    j[1..j.len() - 1].to_string()
}

/// Parse backslash escapes the way `JSON.parse('"' + s + '"')` would,
/// tolerating bare double quotes like the design does.
pub fn esc_dec(s: &str) -> Result<String, ()> {
    let mut out = String::new();
    let mut it = s.chars().peekable();
    while let Some(c) = it.next() {
        if c == '\\' {
            match it.next().ok_or(())? {
                'n' => out.push('\n'),
                't' => out.push('\t'),
                'r' => out.push('\r'),
                'b' => out.push('\u{8}'),
                'f' => out.push('\u{c}'),
                '/' => out.push('/'),
                '\\' => out.push('\\'),
                '"' => out.push('"'),
                'u' => {
                    let hex: String = (0..4).filter_map(|_| it.next()).collect();
                    let cp = u32::from_str_radix(&hex, 16).map_err(|_| ())?;
                    if (0xD800..0xDC00).contains(&cp) {
                        // Surrogate pair: expect \uDC00..
                        if it.next() != Some('\\') || it.next() != Some('u') {
                            return Err(());
                        }
                        let hex2: String = (0..4).filter_map(|_| it.next()).collect();
                        let lo = u32::from_str_radix(&hex2, 16).map_err(|_| ())?;
                        let c = 0x10000 + ((cp - 0xD800) << 10) + (lo.wrapping_sub(0xDC00));
                        out.push(char::from_u32(c).ok_or(())?);
                    } else {
                        out.push(char::from_u32(cp).ok_or(())?);
                    }
                }
                _ => return Err(()),
            }
        } else if (c as u32) < 0x20 && c != '\n' && c != '\t' {
            return Err(());
        } else {
            out.push(c);
        }
    }
    Ok(out)
}

// ---------------------------------------------------------------- hashes

pub fn hashes(text: &str) -> [(&'static str, String); 5] {
    use md5::Digest as _;
    let b = text.as_bytes();
    let hex = |d: &[u8]| d.iter().map(|x| format!("{x:02x}")).collect::<String>();
    [
        ("MD5", hex(&md5::Md5::digest(b))),
        ("SHA-1", hex(&sha1::Sha1::digest(b))),
        ("SHA-256", hex(&sha2::Sha256::digest(b))),
        ("SHA-384", hex(&sha2::Sha384::digest(b))),
        ("SHA-512", hex(&sha2::Sha512::digest(b))),
    ]
}

// ---------------------------------------------------------------- UUID / passwords / lorem

pub fn uuid_v4() -> String {
    let mut b = [0u8; 16];
    getrandom::fill(&mut b).expect("OS randomness");
    b[6] = (b[6] & 0x0f) | 0x40;
    b[8] = (b[8] & 0x3f) | 0x80;
    let h: String = b.iter().map(|x| format!("{x:02x}")).collect();
    format!("{}-{}-{}-{}-{}", &h[0..8], &h[8..12], &h[12..16], &h[16..20], &h[20..32])
}

pub struct PwSet {
    pub label: &'static str,
    pub sample: &'static str,
    pub chars: &'static str,
}

pub const PW_SETS: [PwSet; 4] = [
    PwSet { label: "Uppercase letters", sample: "ABC", chars: "ABCDEFGHIJKLMNOPQRSTUVWXYZ" },
    PwSet { label: "Lowercase letters", sample: "abc", chars: "abcdefghijklmnopqrstuvwxyz" },
    PwSet { label: "Digits", sample: "123", chars: "0123456789" },
    PwSet { label: "Special characters", sample: "#$&", chars: "!@#$%^&*()-_=+[]{};:,.?/" },
];

/// Five passwords that each contain at least one character from every enabled set.
pub fn make_passwords(len: usize, enabled: [bool; 4]) -> Vec<String> {
    let sets: Vec<&[u8]> = PW_SETS.iter().zip(enabled).filter(|(_, on)| *on).map(|(s, _)| s.chars.as_bytes()).collect();
    if sets.is_empty() {
        return Vec::new();
    }
    let pool: Vec<u8> = sets.iter().flat_map(|s| s.iter().copied()).collect();
    let len = len.max(4);
    (0..5)
        .map(|_| {
            let mut chars: Vec<u8> = sets.iter().map(|s| s[rand_below(s.len())]).collect();
            while chars.len() < len {
                chars.push(pool[rand_below(pool.len())]);
            }
            for i in (1..chars.len()).rev() {
                chars.swap(i, rand_below(i + 1));
            }
            String::from_utf8(chars).unwrap()
        })
        .collect()
}

pub fn entropy_bits(len: usize, enabled: [bool; 4]) -> u32 {
    let pool: usize = PW_SETS.iter().zip(enabled).filter(|(_, on)| *on).map(|(s, _)| s.chars.len()).sum();
    if pool == 0 { 0 } else { (len as f64 * (pool as f64).log2()).round() as u32 }
}

const LOREM: &str = "lorem ipsum dolor sit amet consectetur adipiscing elit sed do eiusmod tempor incididunt ut labore et dolore magna aliqua enim ad minim veniam quis nostrud exercitation ullamco laboris nisi aliquip ex ea commodo consequat duis aute irure in reprehenderit voluptate velit esse cillum fugiat nulla pariatur excepteur sint occaecat cupidatat non proident sunt culpa qui officia deserunt mollit anim id est laborum";

/// mulberry32, as in the design, so a seed always yields the same text.
struct Prng(u32);
impl Prng {
    fn next(&mut self) -> f64 {
        self.0 = self.0.wrapping_add(0x6D2B79F5);
        let a = self.0;
        let mut t = (a ^ (a >> 15)).wrapping_mul(1 | a);
        t = (t.wrapping_add((t ^ (t >> 7)).wrapping_mul(61 | t))) ^ t;
        ((t ^ (t >> 14)) as f64) / 4294967296.0
    }
}

fn cap(s: &str) -> String {
    let mut c = s.chars();
    match c.next() {
        Some(f) => f.to_uppercase().collect::<String>() + c.as_str(),
        None => String::new(),
    }
}

#[derive(Clone, Copy, PartialEq)]
pub enum LoremKind {
    Words,
    Sentences,
    Paragraphs,
}

fn lorem_word(r: &mut Prng, words: &[&'static str]) -> &'static str {
    words[(r.next() * words.len() as f64) as usize]
}

fn lorem_sentence(r: &mut Prng, words: &[&'static str]) -> String {
    let n = 7 + (r.next() * 10.) as usize;
    let mut ws: Vec<String> = (0..n).map(|_| lorem_word(r, words).to_string()).collect();
    if n > 9 {
        ws[n / 2].push(',');
    }
    cap(&ws.join(" ")) + "."
}

pub fn lorem(kind: LoremKind, count: usize, seed: u32) -> String {
    let words: Vec<&'static str> = LOREM.split(' ').collect();
    let count = count.max(1);
    let mut r = Prng(seed.wrapping_mul(7919).wrapping_add(13));
    match kind {
        LoremKind::Words => {
            let mut ws: Vec<String> = (0..count).map(|_| lorem_word(&mut r, &words).to_string()).collect();
            ws[0] = "lorem".into();
            if count > 1 {
                ws[1] = "ipsum".into();
            }
            cap(&ws.join(" "))
        }
        LoremKind::Sentences | LoremKind::Paragraphs => {
            let mut items: Vec<String> = (0..count)
                .map(|_| {
                    if kind == LoremKind::Sentences {
                        lorem_sentence(&mut r, &words)
                    } else {
                        let n = 4 + (r.next() * 4.) as usize;
                        (0..n).map(|_| lorem_sentence(&mut r, &words)).collect::<Vec<_>>().join(" ")
                    }
                })
                .collect();
            let mut c = items[0].chars();
            let lowered = c.next().map(|f| f.to_lowercase().collect::<String>() + c.as_str()).unwrap_or_default();
            items[0] = format!("Lorem ipsum dolor sit amet, {lowered}");
            items.join(if kind == LoremKind::Sentences { " " } else { "

" })
        }
    }
}

// ---------------------------------------------------------------- color

pub fn parse_color(s: &str) -> Option<[u8; 3]> {
    let s = s.trim();
    let hex = s.strip_prefix('#').unwrap_or(s);
    if (hex.len() == 3 || hex.len() == 6) && hex.chars().all(|c| c.is_ascii_hexdigit()) {
        let full: String = if hex.len() == 3 { hex.chars().flat_map(|c| [c, c]).collect() } else { hex.to_string() };
        let v = u32::from_str_radix(&full, 16).ok()?;
        return Some([(v >> 16) as u8, (v >> 8) as u8, v as u8]);
    }
    let lower = s.to_lowercase();
    let nums = |body: &str| -> Vec<f64> {
        body.split(|c: char| c == ',' || c.is_whitespace() || c == '/')
            .filter(|p| !p.is_empty())
            .map(|p| p.trim_end_matches('%').trim_end_matches("deg").parse::<f64>().unwrap_or(f64::NAN))
            .collect()
    };
    if let Some(body) = lower.strip_prefix("rgba(").or_else(|| lower.strip_prefix("rgb(")) {
        let v = nums(body.trim_end_matches(')'));
        if v.len() >= 3 && v[..3].iter().all(|x| x.is_finite()) {
            return Some([v[0].min(255.) as u8, v[1].min(255.) as u8, v[2].min(255.) as u8]);
        }
    }
    if let Some(body) = lower.strip_prefix("hsla(").or_else(|| lower.strip_prefix("hsl(")) {
        let v = nums(body.trim_end_matches(')'));
        if v.len() >= 3 && v[..3].iter().all(|x| x.is_finite()) {
            return Some(hsl_to_rgb(v[0], v[1], v[2]));
        }
    }
    None
}

pub fn hsl_to_rgb(h: f64, s: f64, l: f64) -> [u8; 3] {
    let (s, l) = (s / 100., l / 100.);
    let a = s * l.min(1. - l);
    let f = |n: f64| {
        let k = (n + h / 30.) % 12.;
        let v = l - a * (-1f64).max((k - 3.).min(9. - k).min(1.));
        (v * 255.).round().clamp(0., 255.) as u8
    };
    [f(0.), f(8.), f(4.)]
}

pub fn rgb_to_hsl(c: [u8; 3]) -> (i32, i32, i32) {
    let [r, g, b] = c.map(|x| x as f64 / 255.);
    let max = r.max(g).max(b);
    let min = r.min(g).min(b);
    let l = (max + min) / 2.;
    let (mut h, mut s) = (0., 0.);
    if max != min {
        let d = max - min;
        s = if l > 0.5 { d / (2. - max - min) } else { d / (max + min) };
        h = if max == r {
            (g - b) / d + if g < b { 6. } else { 0. }
        } else if max == g {
            (b - r) / d + 2.
        } else {
            (r - g) / d + 4.
        };
        h *= 60.;
    }
    (h.round() as i32, (s * 100.).round() as i32, (l * 100.).round() as i32)
}

pub fn hex(c: [u8; 3]) -> String {
    format!("#{:02X}{:02X}{:02X}", c[0], c[1], c[2])
}

fn lum(c: [u8; 3]) -> f64 {
    let ch = c.map(|v| {
        let v = v as f64 / 255.;
        if v <= 0.03928 { v / 12.92 } else { ((v + 0.055) / 1.055).powf(2.4) }
    });
    0.2126 * ch[0] + 0.7152 * ch[1] + 0.0722 * ch[2]
}

pub fn contrast(a: [u8; 3], b: [u8; 3]) -> f64 {
    let (x, y) = (lum(a), lum(b));
    (x.max(y) + 0.05) / (x.min(y) + 0.05)
}

pub fn grade(r: f64) -> &'static str {
    if r >= 7. {
        "AAA"
    } else if r >= 4.5 {
        "AA"
    } else if r >= 3. {
        "AA Large"
    } else {
        "Fail"
    }
}

// ---------------------------------------------------------------- text

pub fn split_words(s: &str) -> Vec<String> {
    // Insert a break at lower/digit → upper boundaries, then split on non-alphanumerics.
    let mut spaced = String::with_capacity(s.len() + 8);
    let mut prev: Option<char> = None;
    for c in s.chars() {
        if let Some(p) = prev {
            if (p.is_ascii_lowercase() || p.is_ascii_digit()) && c.is_ascii_uppercase() {
                spaced.push(' ');
            }
        }
        spaced.push(c);
        prev = Some(c);
    }
    spaced.split(|c: char| !c.is_ascii_alphanumeric()).filter(|w| !w.is_empty()).map(String::from).collect()
}

fn cap_word(w: &str) -> String {
    let mut c = w.chars();
    match c.next() {
        Some(f) => f.to_uppercase().collect::<String>() + &c.as_str().to_lowercase(),
        None => String::new(),
    }
}

pub fn case_rows(t: &str) -> Vec<(&'static str, String)> {
    let ws = split_words(t);
    let lower = t.to_lowercase();
    // Sentence case: capitalize the first letter and each letter after [.!?] + whitespace.
    let mut sentence = String::with_capacity(lower.len());
    let mut at_start = true;
    let mut after_punct = false;
    for c in lower.chars() {
        if at_start && c.is_whitespace() {
            sentence.push(c);
            continue;
        }
        if at_start && c.is_ascii_lowercase() {
            sentence.extend(c.to_uppercase());
            at_start = false;
            continue;
        }
        at_start = false;
        if after_punct && c.is_whitespace() {
            sentence.push(c);
            at_start = true;
            after_punct = false;
            continue;
        }
        after_punct = matches!(c, '.' | '!' | '?');
        sentence.push(c);
    }
    let title: String = {
        let mut out = String::new();
        let mut word = String::new();
        for c in t.chars() {
            if c.is_whitespace() {
                if !word.is_empty() {
                    out.push_str(&cap_word(&word));
                    word.clear();
                }
                out.push(c);
            } else {
                word.push(c);
            }
        }
        if !word.is_empty() {
            out.push_str(&cap_word(&word));
        }
        out
    };
    vec![
        ("Sentence case", sentence),
        ("Title Case", title),
        ("lower case", lower),
        ("UPPER CASE", t.to_uppercase()),
        ("camelCase", ws.iter().enumerate().map(|(i, w)| if i == 0 { w.to_lowercase() } else { cap_word(w) }).collect()),
        ("PascalCase", ws.iter().map(|w| cap_word(w)).collect()),
        ("snake_case", ws.iter().map(|w| w.to_lowercase()).collect::<Vec<_>>().join("_")),
        ("CONSTANT_CASE", ws.iter().map(|w| w.to_uppercase()).collect::<Vec<_>>().join("_")),
        ("kebab-case", ws.iter().map(|w| w.to_lowercase()).collect::<Vec<_>>().join("-")),
    ]
}

pub fn text_stats(t: &str) -> [(String, &'static str); 5] {
    let words = if t.trim().is_empty() { 0 } else { t.split_whitespace().count() };
    let lines = if t.is_empty() { 0 } else { t.split('\n').count() };
    let sentences = {
        let mut n = 0;
        let mut cur_has_text = false;
        let mut chars = t.chars().peekable();
        while let Some(c) = chars.next() {
            if matches!(c, '.' | '!' | '?') {
                while chars.peek().is_some_and(|c| matches!(c, '.' | '!' | '?')) {
                    chars.next();
                }
                if cur_has_text {
                    n += 1;
                }
                cur_has_text = false;
            } else if !c.is_whitespace() {
                cur_has_text = true;
            }
        }
        if cur_has_text {
            n += 1;
        }
        n
    };
    [
        (thousands(t.encode_utf16().count() as u64), "Characters"),
        (thousands(words as u64), "Words"),
        (thousands(lines as u64), "Lines"),
        (thousands(sentences as u64), "Sentences"),
        (thousands(t.len() as u64), "Bytes (UTF-8)"),
    ]
}

// ---------------------------------------------------------------- smart detection

use crate::registry::ToolId;

/// Guess which tool fits a piece of clipboard text, with a short description
/// of what was recognised. Cheap checks only: this runs on window focus.
pub fn detect(text: &str) -> Option<(ToolId, &'static str)> {
    let t = text.trim();
    if t.is_empty() || t.len() > 200_000 {
        return None;
    }
    let single_line = !t.contains('\n');

    // JWT: three base64url segments whose header decodes to JSON.
    let parts: Vec<&str> = t.split('.').collect();
    if single_line && parts.len() == 3 && parts[0].starts_with("eyJ") {
        if b64url_decode(parts[0]).ok().and_then(|b| serde_json::from_slice::<Value>(&b).ok()).is_some() {
            return Some((ToolId::Jwt, "a JSON Web Token"));
        }
    }
    if (t.starts_with('{') || t.starts_with('[')) && serde_json::from_str::<Value>(t).is_ok() {
        return Some((ToolId::JsonFmt, "JSON"));
    }
    if single_line && parse_color(t).is_some() && (t.starts_with('#') || t.contains('(')) {
        return Some((ToolId::Color, "a color"));
    }
    // Unix time in seconds or milliseconds, between 2001 and 2100.
    if t.chars().all(|c| c.is_ascii_digit()) && (t.len() == 10 || t.len() == 13) {
        let n: i64 = t.parse().ok()?;
        let secs = if t.len() == 13 { n / 1000 } else { n };
        if (1_000_000_000..4_102_444_800).contains(&secs) {
            return Some((ToolId::Date, "a Unix timestamp"));
        }
    }
    if single_line && t.contains('%') && t != url_dec(t).unwrap_or_else(|_| t.to_string()) {
        if t.as_bytes().windows(3).any(|w| w[0] == b'%' && w[1].is_ascii_hexdigit() && w[2].is_ascii_hexdigit()) {
            return Some((ToolId::Url, "URL-encoded text"));
        }
    }
    // Base64 of readable UTF-8 text.
    let b64_charset = t.chars().all(|c| c.is_ascii_alphanumeric() || "+/=\r\n".contains(c));
    if t.len() >= 12 && t.len() % 4 == 0 && b64_charset && t.chars().any(|c| c.is_ascii_digit() || c.is_ascii_uppercase()) {
        if let Ok(decoded) = b64_dec(t) {
            let printable = decoded.chars().filter(|c| !c.is_control() || c.is_whitespace()).count();
            if !decoded.is_empty() && printable * 10 >= decoded.chars().count() * 9 {
                return Some((ToolId::Base64, "Base64 text"));
            }
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn grouping() {
        assert_eq!(group("48879", 3, ","), "48,879");
        assert_eq!(group("BEEF", 4, " "), "BEEF");
        assert_eq!(group("1011111011101111", 4, " "), "1011 1110 1110 1111");
    }

    #[test]
    fn bases() {
        assert_eq!(parse_base("0xBEEF", 16).unwrap(), Some(48879));
        assert_eq!(to_radix(48879, 16), "BEEF");
        assert!(parse_base("12z", 10).is_err());
    }

    #[test]
    fn codecs() {
        assert_eq!(b64_enc("Hello, DevToys! 👋"), "SGVsbG8sIERldlRveXMhIPCfkYs=");
        assert_eq!(b64_dec("SGVsbG8sIERldlRveXMhIPCfkYs=").unwrap(), "Hello, DevToys! 👋");
        assert_eq!(url_enc("a b&c"), "a%20b%26c");
        assert_eq!(url_dec("a%20b%26c").unwrap(), "a b&c");
        assert!(url_dec("%E0%A4%A").is_err());
        assert_eq!(html_dec("&lt;a&gt; &amp;amp; &#39;x&#x27; &bogus;"), "<a> &amp; 'x' &bogus;");
        assert_eq!(esc_enc("a\n\"b\"\t"), "a\\n\\\"b\\\"\\t");
        assert_eq!(esc_dec("a\\n\\\"b\\\"\\t").unwrap(), "a\n\"b\"\t");
    }

    #[test]
    fn hashing() {
        let h = hashes("DevToys");
        assert_eq!(h[0].1.len(), 32);
        assert_eq!(hashes("")[0].1, "d41d8cd98f00b204e9800998ecf8427e");
    }

    #[test]
    fn json_and_yaml() {
        let v: Value = serde_json::from_str(r#"{"b":1,"a":[1,{"x":"y: z"}],"e":{}}"#).unwrap();
        assert_eq!(to_json(&sort_keys(v.clone()), &Indent::Minified), r#"{"a":[1,{"x":"y: z"}],"b":1,"e":{}}"#);
        assert_eq!(to_yaml(&v, 0, 2), "b: 1\na:\n  - 1\n  - x: \"y: z\"\ne: {}");
    }

    #[test]
    fn colors() {
        assert_eq!(parse_color("#0067C0"), Some([0, 103, 192]));
        assert_eq!(parse_color("rgb(0, 103, 192)"), Some([0, 103, 192]));
        assert_eq!(rgb_to_hsl([0, 103, 192]), (208, 100, 38));
        assert_eq!(hex(hsl_to_rgb(208., 100., 38.)), "#0067C2");
    }

    #[test]
    fn words_and_cases() {
        assert_eq!(split_words("helloWorld foo_bar"), vec!["hello", "World", "foo", "bar"]);
        let rows = case_rows("the quick. brown fox");
        assert_eq!(rows[0].1, "The quick. Brown fox");
        assert_eq!(rows[4].1, "theQuickBrownFox");
    }

    #[test]
    fn lorem_is_deterministic() {
        assert_eq!(lorem(LoremKind::Words, 3, 1), lorem(LoremKind::Words, 3, 1));
        assert!(lorem(LoremKind::Paragraphs, 2, 1).starts_with("Lorem ipsum dolor sit amet, "));
    }

    #[test]
    fn detects_clipboard_content() {
        use crate::registry::ToolId;
        let jwt = format!("{}.{}.sig", b64url_json(&serde_json::json!({"alg": "HS256"})), b64url_json(&serde_json::json!({"sub": "1"})));
        assert_eq!(detect(&jwt).map(|d| d.0), Some(ToolId::Jwt));
        assert_eq!(detect(r#"{"a": 1}"#).map(|d| d.0), Some(ToolId::JsonFmt));
        assert_eq!(detect("#0067C0").map(|d| d.0), Some(ToolId::Color));
        assert_eq!(detect("1790110981").map(|d| d.0), Some(ToolId::Date));
        assert_eq!(detect("a%20b%26c").map(|d| d.0), Some(ToolId::Url));
        assert_eq!(detect("SGVsbG8sIERldlRveXMhIPCfkYs=").map(|d| d.0), Some(ToolId::Base64));
        assert_eq!(detect("just some words"), None);
        assert_eq!(detect("12345"), None);
        assert_eq!(detect("password"), None);
    }

    #[test]
    fn uuid_shape() {
        let u = uuid_v4();
        assert_eq!(u.len(), 36);
        assert_eq!(&u[14..15], "4");
    }
}
