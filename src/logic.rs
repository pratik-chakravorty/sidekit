//! Pure conversion logic behind every tool. No UI types in here.

use base64::Engine as _;
use serde_json::Value;

pub mod cert;
pub mod cron;
pub mod csv;
pub mod diff;
pub mod iprange;
pub mod media;
pub mod mock;
pub mod xml;

// ---------------------------------------------------------------- helpers

pub fn thousands(n: u64) -> String {
    let s = n.to_string();
    group(&s, 3, ",")
}

/// `C.plural`: "1 byte", "2,048 bytes".
pub fn plural(n: usize, word: &str) -> String {
    let es = ["s", "x", "ch", "sh"].iter().any(|e| word.ends_with(e));
    let w = if n == 1 { word.to_string() } else if es { format!("{word}es") } else { format!("{word}s") };
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

/// UTF-8 bytes as space-separated lowercase hex pairs.
pub fn hex_enc(s: &str) -> String {
    s.bytes().map(|b| format!("{b:02x}")).collect::<Vec<_>>().join(" ")
}

/// Hex bytes back to UTF-8 text. Takes contiguous or separated pairs
/// (spaces, `:`, `-`, `,`) with optional `0x` or `\x` prefixes.
pub fn hex_dec(s: &str) -> Result<String, ()> {
    let s = s.replace("\\x", " ");
    let mut digits = String::new();
    for tok in s.split(|c: char| c.is_whitespace() || ",:-;".contains(c)) {
        let tok = tok.strip_prefix("0x").or_else(|| tok.strip_prefix("0X")).unwrap_or(tok);
        if tok.len() % 2 == 1 || !tok.chars().all(|c| c.is_ascii_hexdigit()) {
            return Err(());
        }
        digits.push_str(tok);
    }
    let bytes = (0..digits.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(&digits[i..i + 2], 16))
        .collect::<Result<Vec<u8>, _>>()
        .map_err(|_| ())?;
    String::from_utf8(bytes).map_err(|_| ())
}

// ---------------------------------------------------------------- hashes

fn to_hex(d: &[u8]) -> String {
    d.iter().map(|x| format!("{x:02x}")).collect()
}

/// Plain digests of `text`, or HMACs of it when a `key` is given.
pub fn hashes(text: &str, key: Option<&str>) -> Vec<(&'static str, String)> {
    use md5::Digest as _;
    let b = text.as_bytes();
    match key {
        None => vec![
            ("MD5", to_hex(&md5::Md5::digest(b))),
            ("SHA-1", to_hex(&sha1::Sha1::digest(b))),
            ("SHA-224", to_hex(&sha2::Sha224::digest(b))),
            ("SHA-256", to_hex(&sha2::Sha256::digest(b))),
            ("SHA-384", to_hex(&sha2::Sha384::digest(b))),
            ("SHA-512", to_hex(&sha2::Sha512::digest(b))),
            ("CRC-32", format!("{:08x}", crc32(b))),
        ],
        Some(k) => {
            let k = k.as_bytes();
            vec![
                ("HMAC-MD5", to_hex(&hmac::<md5::Md5>(k, b, 64))),
                ("HMAC-SHA1", to_hex(&hmac::<sha1::Sha1>(k, b, 64))),
                ("HMAC-SHA224", to_hex(&hmac::<sha2::Sha224>(k, b, 64))),
                ("HMAC-SHA256", to_hex(&hmac::<sha2::Sha256>(k, b, 64))),
                ("HMAC-SHA384", to_hex(&hmac::<sha2::Sha384>(k, b, 128))),
                ("HMAC-SHA512", to_hex(&hmac::<sha2::Sha512>(k, b, 128))),
            ]
        }
    }
}

/// RFC 2104 HMAC over a digest with the given block size in bytes.
fn hmac<D: md5::Digest>(key: &[u8], msg: &[u8], block: usize) -> Vec<u8> {
    let mut k = if key.len() > block { D::digest(key).to_vec() } else { key.to_vec() };
    k.resize(block, 0);
    let pad = |x: u8| k.iter().map(|b| b ^ x).collect::<Vec<u8>>();
    let inner = D::new().chain_update(pad(0x36)).chain_update(msg).finalize();
    D::new().chain_update(pad(0x5c)).chain_update(inner).finalize().to_vec()
}

/// CRC-32 (IEEE, as used by zip and PNG).
pub fn crc32(b: &[u8]) -> u32 {
    let mut c = !0u32;
    for &x in b {
        c ^= x as u32;
        for _ in 0..8 {
            c = if c & 1 != 0 { (c >> 1) ^ 0xEDB8_8320 } else { c >> 1 };
        }
    }
    !c
}

// ---------------------------------------------------------------- UUID / passwords / lorem

fn fmt_uuid(b: &[u8; 16]) -> String {
    let h = to_hex(b);
    format!("{}-{}-{}-{}-{}", &h[0..8], &h[8..12], &h[12..16], &h[16..20], &h[20..32])
}

fn now_ms() -> u64 {
    chrono::Utc::now().timestamp_millis().max(0) as u64
}

pub fn uuid_v4() -> String {
    let mut b = [0u8; 16];
    getrandom::fill(&mut b).expect("OS randomness");
    b[6] = (b[6] & 0x0f) | 0x40;
    b[8] = (b[8] & 0x3f) | 0x80;
    fmt_uuid(&b)
}

/// RFC 9562 version 7: a 48-bit millisecond timestamp, then random bits.
pub fn uuid_v7() -> String {
    let mut b = [0u8; 16];
    getrandom::fill(&mut b).expect("OS randomness");
    b[..6].copy_from_slice(&now_ms().to_be_bytes()[2..]);
    b[6] = (b[6] & 0x0f) | 0x70;
    b[8] = (b[8] & 0x3f) | 0x80;
    fmt_uuid(&b)
}

const CROCKFORD: &[u8; 32] = b"0123456789ABCDEFGHJKMNPQRSTVWXYZ";

/// A ULID: 48-bit millisecond timestamp and 80 random bits in Crockford Base32.
pub fn ulid() -> String {
    let mut r = [0u8; 10];
    getrandom::fill(&mut r).expect("OS randomness");
    let mut v = ((now_ms() & 0xFFFF_FFFF_FFFF) as u128) << 80;
    for (i, x) in r.iter().enumerate() {
        v |= (*x as u128) << (72 - 8 * i);
    }
    (0..26).map(|i| CROCKFORD[((v >> ((25 - i) * 5)) & 31) as usize] as char).collect()
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum IdKind {
    UuidV4,
    UuidV7,
    Ulid,
}

/// `n` identifiers; time-ordered kinds come back sorted, so a batch made
/// within one millisecond still lists in creation order.
pub fn make_ids(kind: IdKind, n: usize) -> Vec<String> {
    let make = match kind {
        IdKind::UuidV4 => uuid_v4,
        IdKind::UuidV7 => uuid_v7,
        IdKind::Ulid => ulid,
    };
    let mut ids: Vec<String> = (0..n).map(|_| make()).collect();
    if kind != IdKind::UuidV4 {
        ids.sort();
    }
    ids
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

// ---------------------------------------------------------------- lines

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum LineSort {
    Keep,
    Asc,
    Desc,
    Natural,
    Length,
    Reverse,
    Shuffle,
}

#[derive(Clone, Copy)]
pub struct LineOpts {
    pub sort: LineSort,
    pub dedupe: bool,
    pub ignore_case: bool,
    pub trim: bool,
    pub drop_empty: bool,
    /// Seed for `LineSort::Shuffle`, so a shuffle holds still until asked again.
    pub seed: u32,
}

pub struct LinesOut {
    pub text: String,
    pub lines: usize,
    pub dupes: usize,
    pub empties: usize,
}

fn digit_run(it: &mut std::iter::Peekable<std::str::Chars>) -> String {
    let mut s = String::new();
    while let Some(&c) = it.peek().filter(|c| c.is_ascii_digit()) {
        s.push(c);
        it.next();
    }
    s
}

/// Compare with runs of digits taken as numbers, so "file2" sorts before "file10".
pub fn natural_cmp(a: &str, b: &str) -> std::cmp::Ordering {
    use std::cmp::Ordering::*;
    let (mut x, mut y) = (a.chars().peekable(), b.chars().peekable());
    loop {
        match (x.peek().copied(), y.peek().copied()) {
            (None, None) => return Equal,
            (None, _) => return Less,
            (_, None) => return Greater,
            (Some(c), Some(d)) if c.is_ascii_digit() && d.is_ascii_digit() => {
                let (m, n) = (digit_run(&mut x), digit_run(&mut y));
                let (m, n) = (m.trim_start_matches('0'), n.trim_start_matches('0'));
                let ord = m.len().cmp(&n.len()).then_with(|| m.cmp(n));
                if ord != Equal {
                    return ord;
                }
            }
            (Some(c), Some(d)) => {
                if c != d {
                    return c.cmp(&d);
                }
                x.next();
                y.next();
            }
        }
    }
}

pub fn process_lines(t: &str, o: &LineOpts) -> LinesOut {
    let key = |s: &str| if o.ignore_case { s.to_lowercase() } else { s.to_string() };
    let mut lines: Vec<&str> = t.lines().map(|l| if o.trim { l.trim() } else { l }).collect();
    let total = lines.len();
    if o.drop_empty {
        lines.retain(|l| !l.trim().is_empty());
    }
    let empties = total - lines.len();
    let before = lines.len();
    if o.dedupe {
        let mut seen = std::collections::HashSet::new();
        lines.retain(|l| seen.insert(key(l)));
    }
    let dupes = before - lines.len();
    // Ties fall back to the raw text so case-insensitive sorts stay deterministic.
    match o.sort {
        LineSort::Keep => {}
        LineSort::Asc => lines.sort_by(|a, b| key(a).cmp(&key(b)).then_with(|| a.cmp(b))),
        LineSort::Desc => lines.sort_by(|a, b| key(b).cmp(&key(a)).then_with(|| b.cmp(a))),
        LineSort::Natural => lines.sort_by(|a, b| natural_cmp(&key(a), &key(b)).then_with(|| a.cmp(b))),
        LineSort::Length => lines.sort_by_key(|l| l.chars().count()),
        LineSort::Reverse => lines.reverse(),
        LineSort::Shuffle => {
            let mut r = Prng(o.seed);
            for i in (1..lines.len()).rev() {
                lines.swap(i, ((r.next() * (i + 1) as f64) as usize).min(i));
            }
        }
    }
    LinesOut { text: lines.join("\n"), lines: lines.len(), dupes, empties }
}

// ---------------------------------------------------------------- URL parser

pub struct UrlParts {
    pub rows: Vec<(&'static str, String)>,
    pub params: Vec<(String, String)>,
}

fn default_port(scheme: &str) -> Option<u16> {
    Some(match scheme {
        "http" | "ws" => 80,
        "https" | "wss" => 443,
        "ftp" => 21,
        "ssh" | "sftp" => 22,
        "postgres" | "postgresql" => 5432,
        "mysql" => 3306,
        "redis" => 6379,
        "mongodb" => 27017,
        "amqp" => 5672,
        _ => return None,
    })
}

/// Decode a query-string component: `+` is a space, bad escapes are left as typed.
fn form_dec(s: &str) -> String {
    let s = s.replace('+', " ");
    url_dec(&s).unwrap_or(s)
}

/// Split a URL into its parts (RFC 3986 layout) and decode its query parameters.
pub fn parse_url(s: &str) -> Result<UrlParts, String> {
    let dec = |s: &str| url_dec(s).unwrap_or_else(|_| s.to_string());
    let s = s.trim();
    let (scheme, rest) = s
        .split_once(':')
        .filter(|(sc, _)| {
            sc.starts_with(|c: char| c.is_ascii_alphabetic()) && sc.chars().all(|c| c.is_ascii_alphanumeric() || "+-.".contains(c))
        })
        .ok_or("Not a URL: it should start with a scheme such as https://")?;
    let scheme = scheme.to_lowercase();
    let (rest, fragment) = match rest.split_once('#') {
        Some((r, f)) => (r, Some(f)),
        None => (rest, None),
    };
    let (rest, query) = match rest.split_once('?') {
        Some((r, q)) => (r, Some(q)),
        None => (rest, None),
    };
    let (authority, path) = match rest.strip_prefix("//") {
        Some(r) => match r.find('/') {
            Some(i) => (Some(&r[..i]), &r[i..]),
            None => (Some(r), ""),
        },
        None => (None, rest),
    };

    let mut rows = vec![("Scheme", scheme.clone())];
    if let Some(a) = authority {
        let (userinfo, hostport) = match a.rsplit_once('@') {
            Some((u, h)) => (Some(u), h),
            None => (None, a),
        };
        if let Some(u) = userinfo {
            let (user, pass) = match u.split_once(':') {
                Some((u, p)) => (u, Some(p)),
                None => (u, None),
            };
            rows.push(("Username", dec(user)));
            if let Some(p) = pass {
                rows.push(("Password", dec(p)));
            }
        }
        let (host, port) = if hostport.starts_with('[') {
            let end = hostport.find(']').ok_or("The IPv6 host is missing its closing ]")?;
            let tail = &hostport[end + 1..];
            if !tail.is_empty() && !tail.starts_with(':') {
                return Err("Unexpected text after the IPv6 host".into());
            }
            (&hostport[..=end], tail.strip_prefix(':'))
        } else {
            match hostport.rsplit_once(':') {
                Some((h, p)) => (h, Some(p)),
                None => (hostport, None),
            }
        };
        if host.is_empty() && scheme != "file" {
            return Err("The URL has no host".into());
        }
        let host = dec(host).to_lowercase();
        rows.push(("Host", host.clone()));
        let port = port.filter(|p| !p.is_empty());
        match port {
            Some(p) => {
                p.parse::<u16>().map_err(|_| format!("\"{p}\" is not a valid port"))?;
                rows.push(("Port", p.to_string()));
            }
            None => {
                if let Some(d) = default_port(&scheme) {
                    rows.push(("Port", format!("{d} (default)")));
                }
            }
        }
        let origin_port = port.map(|p| format!(":{p}")).unwrap_or_default();
        rows.push(("Origin", format!("{scheme}://{host}{origin_port}")));
    }
    rows.push(("Path", if path.is_empty() && authority.is_some() { "/".into() } else { dec(path) }));
    if let Some(q) = query {
        rows.push(("Query", q.to_string()));
    }
    if let Some(f) = fragment {
        rows.push(("Fragment", dec(f)));
    }
    let params = query
        .unwrap_or("")
        .split('&')
        .filter(|kv| !kv.is_empty())
        .map(|kv| match kv.split_once('=') {
            Some((k, v)) => (form_dec(k), form_dec(v)),
            None => (form_dec(kv), String::new()),
        })
        .collect();
    Ok(UrlParts { rows, params })
}

// ---------------------------------------------------------------- Unicode inspector

/// Names for the characters people most often need to spot: whitespace,
/// invisible formatting marks and bidi controls.
fn special_name(c: char) -> Option<&'static str> {
    Some(match c {
        '\0' => "NULL",
        '\t' => "TAB",
        '\n' => "LINE FEED",
        '\r' => "CARRIAGE RETURN",
        ' ' => "SPACE",
        '\u{A0}' => "NO-BREAK SPACE",
        '\u{AD}' => "SOFT HYPHEN",
        '\u{2000}'..='\u{200A}' => "TYPOGRAPHIC SPACE",
        '\u{200B}' => "ZERO WIDTH SPACE",
        '\u{200C}' => "ZERO WIDTH NON-JOINER",
        '\u{200D}' => "ZERO WIDTH JOINER",
        '\u{200E}' => "LEFT-TO-RIGHT MARK",
        '\u{200F}' => "RIGHT-TO-LEFT MARK",
        '\u{2028}' => "LINE SEPARATOR",
        '\u{2029}' => "PARAGRAPH SEPARATOR",
        '\u{202A}'..='\u{202E}' => "BIDI EMBEDDING / OVERRIDE",
        '\u{202F}' => "NARROW NO-BREAK SPACE",
        '\u{2060}' => "WORD JOINER",
        '\u{2066}'..='\u{2069}' => "BIDI ISOLATE",
        '\u{3000}' => "IDEOGRAPHIC SPACE",
        '\u{FE0E}' => "VARIATION SELECTOR-15 (text)",
        '\u{FE0F}' => "VARIATION SELECTOR-16 (emoji)",
        '\u{FEFF}' => "BYTE ORDER MARK",
        '\u{FFFD}' => "REPLACEMENT CHARACTER",
        _ => return None,
    })
}

fn is_combining(c: char) -> bool {
    matches!(c as u32, 0x300..=0x36F | 0x1AB0..=0x1AFF | 0x1DC0..=0x1DFF | 0x20D0..=0x20FF | 0xFE20..=0xFE2F)
}

/// Characters that render as nothing (or like a plain space) but are not one.
pub fn is_invisible(c: char) -> bool {
    !matches!(c, ' ' | '\t' | '\n' | '\r') && (special_name(c).is_some() || c.is_control())
}

pub struct CharInfo {
    /// What to draw for the character; invisible ones get a visible stand-in.
    pub shown: String,
    pub code: String,
    pub kind: &'static str,
    pub utf8: String,
    pub flagged: bool,
}

pub fn char_info(c: char) -> CharInfo {
    let n = c as u32;
    // Control pictures (U+2400 on) are missing from most fonts, so use short names.
    let shown = match c {
        ' ' => "SP".to_string(),
        '\t' => "TAB".to_string(),
        '\n' => "LF".to_string(),
        '\r' => "CR".to_string(),
        '\u{7F}' => "DEL".to_string(),
        _ if n < 0x20 => format!("^{}", char::from(n as u8 + 64)),
        _ if is_invisible(c) => "◌".to_string(),
        _ if is_combining(c) => format!("◌{c}"),
        _ => c.to_string(),
    };
    let kind = special_name(c).unwrap_or_else(|| {
        if c.is_control() {
            "Control"
        } else if is_combining(c) {
            "Combining mark"
        } else if c.is_ascii_digit() || c.is_numeric() {
            "Number"
        } else if c.is_alphabetic() {
            if c.is_ascii() { "Latin letter" } else { "Letter" }
        } else if c.is_whitespace() {
            "Whitespace"
        } else if c.is_ascii_punctuation() {
            "Punctuation"
        } else if matches!(n, 0x1F000..=0x1FAFF | 0x2600..=0x27BF | 0x2B00..=0x2BFF) {
            "Emoji / symbol"
        } else {
            "Symbol / other"
        }
    });
    let mut buf = [0u8; 4];
    let utf8 = c.encode_utf8(&mut buf).bytes().map(|b| format!("{b:02X}")).collect::<Vec<_>>().join(" ");
    CharInfo { shown, code: format!("U+{n:04X}"), kind, utf8, flagged: is_invisible(c) }
}

pub fn unicode_stats(t: &str) -> [(String, &'static str); 5] {
    [
        (thousands(t.chars().count() as u64), "Code points"),
        (thousands(t.len() as u64), "UTF-8 bytes"),
        (thousands(t.encode_utf16().count() as u64), "UTF-16 units"),
        (thousands(t.chars().filter(|c| !c.is_ascii()).count() as u64), "Non-ASCII"),
        (thousands(t.chars().filter(|c| is_invisible(*c)).count() as u64), "Invisible"),
    ]
}

/// The text escaped for a few common targets.
pub fn unicode_escapes(t: &str) -> Vec<(&'static str, String)> {
    let mut js = String::new();
    let mut rust = String::new();
    let mut py = String::new();
    let mut html = String::new();
    for c in t.chars() {
        let n = c as u32;
        let simple = match c {
            '\n' => Some("\\n"),
            '\t' => Some("\\t"),
            '\r' => Some("\\r"),
            '\\' => Some("\\\\"),
            '"' => Some("\\\""),
            _ => None,
        };
        if let Some(s) = simple {
            js.push_str(s);
            rust.push_str(s);
            py.push_str(s);
        } else if (0x20..0x7F).contains(&n) {
            js.push(c);
            rust.push(c);
            py.push(c);
        } else {
            for u in c.encode_utf16(&mut [0u16; 2]) {
                js.push_str(&format!("\\u{u:04X}"));
            }
            rust.push_str(&format!("\\u{{{n:X}}}"));
            py.push_str(&if n <= 0xFFFF { format!("\\u{n:04X}") } else { format!("\\U{n:08X}") });
        }
        match c {
            '&' => html.push_str("&amp;"),
            '<' => html.push_str("&lt;"),
            '>' => html.push_str("&gt;"),
            '"' => html.push_str("&quot;"),
            _ if c.is_ascii() && (!c.is_control() || c == '\n' || c == '\t') => html.push(c),
            _ => html.push_str(&format!("&#x{n:X};")),
        }
    }
    let points = t.chars().map(|c| format!("U+{:04X}", c as u32)).collect::<Vec<_>>().join(" ");
    vec![("JavaScript / JSON", js), ("Rust", rust), ("Python", py), ("HTML entities", html), ("Code points", points)]
}

// ---------------------------------------------------------------- network

use std::net::{IpAddr, Ipv4Addr, Ipv6Addr};

fn ipv4_kind(a: Ipv4Addr) -> &'static str {
    let o = a.octets();
    if a.is_unspecified() {
        "Unspecified"
    } else if a.is_loopback() {
        "Loopback"
    } else if a.is_private() {
        "Private (RFC 1918)"
    } else if o[0] == 100 && (64..128).contains(&o[1]) {
        "Shared / CGNAT (RFC 6598)"
    } else if a.is_link_local() {
        "Link-local"
    } else if a.is_documentation() {
        "Documentation (RFC 5737)"
    } else if a.is_multicast() {
        "Multicast"
    } else if a.is_broadcast() {
        "Broadcast"
    } else if o[0] >= 240 {
        "Reserved"
    } else {
        "Public"
    }
}

fn ipv6_kind(a: Ipv6Addr) -> &'static str {
    let s = a.segments();
    if a.is_unspecified() {
        "Unspecified"
    } else if a.is_loopback() {
        "Loopback"
    } else if a.to_ipv4_mapped().is_some() {
        "IPv4-mapped"
    } else if s[0] & 0xffc0 == 0xfe80 {
        "Link-local"
    } else if s[0] & 0xfe00 == 0xfc00 {
        "Unique local (ULA)"
    } else if a.is_multicast() {
        "Multicast"
    } else if s[0] == 0x2001 && s[1] == 0x0db8 {
        "Documentation (RFC 3849)"
    } else if s[0] & 0xe000 == 0x2000 {
        "Global unicast"
    } else {
        "Reserved"
    }
}

/// Everything about an address and its network: `10.1.2.3/24`,
/// `10.1.2.3 255.255.255.0`, `2001:db8::1/64` or a bare address.
pub fn subnet(input: &str) -> Result<Vec<(&'static str, String)>, String> {
    let t = input.trim();
    if t.is_empty() {
        return Err("Type an IPv4 or IPv6 address, optionally with a /prefix".into());
    }
    let (addr, prefix) = match t.split_once(['/', ' ']) {
        Some((a, p)) => (a.trim(), Some(p.trim())),
        None => (t, None),
    };
    let ip: IpAddr = addr.parse().map_err(|_| format!("\"{addr}\" is not an IPv4 or IPv6 address"))?;
    match ip {
        IpAddr::V4(a) => {
            let p = match prefix {
                None => 32,
                Some(p) if p.contains('.') => {
                    let m = u32::from(p.parse::<Ipv4Addr>().map_err(|_| format!("\"{p}\" is not a netmask"))?);
                    if m.leading_ones() + m.trailing_zeros() != 32 {
                        return Err(format!("{p} is not a contiguous netmask"));
                    }
                    m.leading_ones()
                }
                Some(p) => p.parse::<u32>().ok().filter(|p| *p <= 32).ok_or(format!("/{p} is not a prefix between 0 and 32"))?,
            };
            let ip = u32::from(a);
            let mask = if p == 0 { 0 } else { u32::MAX << (32 - p) };
            let net = ip & mask;
            let bc = net | !mask;
            let (first, last, usable) = match p {
                32 => (net, net, 1u64),
                31 => (net, bc, 2),
                _ => (net + 1, bc - 1, (1u64 << (32 - p)) - 2),
            };
            let v4 = |n: u32| Ipv4Addr::from(n).to_string();
            let o = a.octets();
            Ok(vec![
                ("Address", format!("{a}/{p}")),
                ("Network", format!("{}/{p}", v4(net))),
                ("Netmask", v4(mask)),
                ("Wildcard mask", v4(!mask)),
                ("Broadcast", if p >= 31 { "—".into() } else { v4(bc) }),
                ("Host range", if first == last { v4(first) } else { format!("{} – {}", v4(first), v4(last)) }),
                ("Usable hosts", thousands(usable)),
                ("Total addresses", thousands(1u64 << (32 - p))),
                ("Type", ipv4_kind(a).into()),
                ("Integer", ip.to_string()),
                ("Hex", format!("0x{ip:08X}")),
                ("Binary", o.iter().map(|b| format!("{b:08b}")).collect::<Vec<_>>().join(".")),
                ("IPv4-mapped IPv6", format!("::ffff:{a}")),
                ("Reverse DNS", format!("{}.{}.{}.{}.in-addr.arpa", o[3], o[2], o[1], o[0])),
            ])
        }
        IpAddr::V6(a) => {
            let p = match prefix {
                None => 128,
                Some(p) => p.parse::<u32>().ok().filter(|p| *p <= 128).ok_or(format!("/{p} is not a prefix between 0 and 128"))?,
            };
            let ip = u128::from(a);
            let mask = if p == 0 { 0 } else { u128::MAX << (128 - p) };
            let net = ip & mask;
            let last = net | !mask;
            let v6 = |n: u128| Ipv6Addr::from(n).to_string();
            let total = if p == 0 { "2^128".to_string() } else { format!("{} (2^{})", group(&(1u128 << (128 - p)).to_string(), 3, ","), 128 - p) };
            let expanded = a.segments().iter().map(|s| format!("{s:04x}")).collect::<Vec<_>>().join(":");
            let nibbles: String = expanded.chars().rev().filter(|c| *c != ':').flat_map(|c| [c, '.']).collect();
            Ok(vec![
                ("Address", format!("{a}/{p}")),
                ("Expanded", expanded),
                ("Network", format!("{}/{p}", v6(net))),
                ("First address", v6(net)),
                ("Last address", v6(last)),
                ("Total addresses", total),
                ("Type", ipv6_kind(a).into()),
                ("Integer", group(&ip.to_string(), 3, ",")),
                ("Hex", format!("0x{ip:032X}")),
                ("Reverse DNS", format!("{nibbles}ip6.arpa")),
            ])
        }
    }
}

pub const MAC_FORMATS: &[&str] = &["00:1A:2B:3C:4D:5E", "00-1A-2B-3C-4D-5E", "001A.2B3C.4D5E", "001A2B3C4D5E"];

/// A random unicast MAC address in one of `MAC_FORMATS`. `local` sets the
/// locally-administered bit, so it can never clash with a vendor's address.
pub fn mac_address(format: usize, upper: bool, local: bool) -> String {
    let mut b = [0u8; 6];
    getrandom::fill(&mut b).expect("OS randomness");
    b[0] &= 0xFE;
    if local {
        b[0] |= 0x02;
    } else {
        b[0] &= 0xFD;
    }
    let h = to_hex(&b);
    let pairs: Vec<&str> = (0..6).map(|i| &h[i * 2..i * 2 + 2]).collect();
    let s = match format {
        0 => pairs.join(":"),
        1 => pairs.join("-"),
        2 => format!("{}.{}.{}", &h[0..4], &h[4..8], &h[8..12]),
        _ => h.clone(),
    };
    if upper { s.to_uppercase() } else { s }
}

/// An RFC 4193 unique local /48 prefix, with one random /64 inside it.
pub fn ula() -> (String, String) {
    let mut g = [0u8; 7];
    getrandom::fill(&mut g).expect("OS randomness");
    let seg = |a: u8, b: u8| (a as u16) << 8 | b as u16;
    let (s0, s1, s2, sub) = (0xfd00 | g[0] as u16, seg(g[1], g[2]), seg(g[3], g[4]), seg(g[5], g[6]));
    let prefix = Ipv6Addr::new(s0, s1, s2, 0, 0, 0, 0, 0);
    let subnet = Ipv6Addr::new(s0, s1, s2, sub, 0, 0, 0, 0);
    (format!("{prefix}/48"), format!("{subnet}/64"))
}

// ---------------------------------------------------------------- YAML / TOML

fn yaml_key(k: serde_yaml::Value) -> String {
    match k {
        serde_yaml::Value::String(s) => s,
        serde_yaml::Value::Null => "null".into(),
        serde_yaml::Value::Bool(b) => b.to_string(),
        serde_yaml::Value::Number(n) => n.to_string(),
        other => serde_json::to_string(&yaml_json(other)).unwrap_or_default(),
    }
}

/// YAML allows non-string keys and tags that JSON has no room for; keys are
/// stringified and tags dropped.
fn yaml_json(v: serde_yaml::Value) -> Value {
    use serde_yaml::Value as Y;
    match v {
        Y::Null => Value::Null,
        Y::Bool(b) => b.into(),
        Y::Number(n) => {
            if let Some(i) = n.as_i64() {
                i.into()
            } else if let Some(u) = n.as_u64() {
                u.into()
            } else {
                n.as_f64().and_then(serde_json::Number::from_f64).map(Value::Number).unwrap_or_else(|| Value::String(n.to_string()))
            }
        }
        Y::String(s) => s.into(),
        Y::Sequence(a) => Value::Array(a.into_iter().map(yaml_json).collect()),
        Y::Mapping(m) => Value::Object(m.into_iter().map(|(k, v)| (yaml_key(k), yaml_json(v))).collect()),
        Y::Tagged(t) => yaml_json(t.value),
    }
}

pub fn parse_yaml(src: &str) -> Result<Value, String> {
    let mut v: serde_yaml::Value = serde_yaml::from_str(src).map_err(|e| cap(&e.to_string()))?;
    v.apply_merge().map_err(|e| cap(&e.to_string()))?;
    Ok(yaml_json(v))
}

fn toml_json(v: toml::Value) -> Value {
    use toml::Value as T;
    match v {
        T::String(s) => s.into(),
        T::Integer(i) => i.into(),
        T::Float(f) => serde_json::Number::from_f64(f).map(Value::Number).unwrap_or_else(|| Value::String(f.to_string())),
        T::Boolean(b) => b.into(),
        T::Datetime(d) => d.to_string().into(),
        T::Array(a) => Value::Array(a.into_iter().map(toml_json).collect()),
        T::Table(t) => Value::Object(t.into_iter().map(|(k, v)| (k, toml_json(v))).collect()),
    }
}

pub fn parse_toml(src: &str) -> Result<Value, String> {
    let t: toml::Table = toml::from_str(src).map_err(|e| e.to_string().trim().to_string())?;
    Ok(toml_json(toml::Value::Table(t)))
}

fn json_toml(v: &Value, path: &str) -> Result<toml::Value, String> {
    Ok(match v {
        Value::Null => return Err(format!("TOML has no null, found one at {}", if path.is_empty() { "the top level" } else { path })),
        Value::Bool(b) => toml::Value::Boolean(*b),
        Value::Number(n) => match n.as_i64() {
            Some(i) => toml::Value::Integer(i),
            None => toml::Value::Float(n.as_f64().unwrap_or(f64::NAN)),
        },
        Value::String(s) => toml::Value::String(s.clone()),
        Value::Array(a) => toml::Value::Array(
            a.iter().enumerate().map(|(i, x)| json_toml(x, &format!("{path}[{i}]"))).collect::<Result<_, _>>()?,
        ),
        Value::Object(m) => toml::Value::Table(
            m.iter()
                .map(|(k, x)| {
                    let p = if path.is_empty() { k.clone() } else { format!("{path}.{k}") };
                    Ok((k.clone(), json_toml(x, &p)?))
                })
                .collect::<Result<_, String>>()?,
        ),
    })
}

pub fn to_toml(v: &Value) -> Result<String, String> {
    if !v.is_object() {
        return Err("TOML documents are tables: the JSON must be an object at the top level".into());
    }
    let t = json_toml(v, "")?;
    toml::to_string_pretty(&t).map(|s| s.trim_end().to_string()).map_err(|e| cap(&e.to_string()))
}

// ---------------------------------------------------------------- SQL / JSONPath

pub const SQL_MODES: &[&str] = &["Format", "UPPERCASE keywords", "One line"];

pub fn format_sql(src: &str, mode: usize, indent: u8) -> String {
    let opts = sqlformat::FormatOptions {
        indent: sqlformat::Indent::Spaces(indent),
        uppercase: (mode == 1).then_some(true),
        inline: mode == 2,
        lines_between_queries: 2,
        ..Default::default()
    };
    sqlformat::format(src, &sqlformat::QueryParams::None, &opts).trim().to_string()
}

/// Run a JSONPath query (RFC 9535) and return the matches as a JSON array,
/// each with its normalized location.
pub fn jsonpath(doc: &str, path: &str) -> Result<(Value, Vec<String>), String> {
    let v: Value = serde_json::from_str(doc).map_err(|e| format!("Invalid JSON: {}", json_err(&e)))?;
    let p = serde_json_path::JsonPath::parse(path.trim()).map_err(|e| cap(&e.to_string()))?;
    let nodes = p.query_located(&v);
    let locations = nodes.iter().map(|n| n.location().to_string()).collect();
    let values = nodes.iter().map(|n| n.node().clone()).collect();
    Ok((Value::Array(values), locations))
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
    if t.starts_with("-----BEGIN CERTIFICATE-----") {
        return Some((ToolId::Cert, "a certificate"));
    }
    if single_line && t.starts_with("data:image/") && t.contains(";base64,") {
        return Some((ToolId::B64Image, "an image data URI"));
    }
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
    // An IP network such as 10.0.0.0/8 or 2001:db8::/32.
    if let Some((a, p)) = t.split_once('/').filter(|_| single_line) {
        if a.parse::<IpAddr>().is_ok() && p.parse::<u8>().is_ok() {
            return Some((ToolId::Subnet, "an IP network"));
        }
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
    // A web address with a query string, worth taking apart.
    if single_line && (t.starts_with("http://") || t.starts_with("https://")) && t.contains('?') && !t.contains(' ') {
        return Some((ToolId::UrlParse, "a URL with query parameters"));
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
    fn plurals() {
        assert_eq!(plural(1, "address"), "1 address");
        assert_eq!(plural(264, "address"), "264 addresses");
        assert_eq!(plural(2, "match"), "2 matches");
        assert_eq!(plural(0, "line"), "0 lines");
    }

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
        let h = hashes("DevToys", None);
        assert_eq!(h[0].1.len(), 32);
        assert_eq!(hashes("", None)[0].1, "d41d8cd98f00b204e9800998ecf8427e");
        assert_eq!(crc32(b"123456789"), 0xCBF4_3926);
        let fox = "The quick brown fox jumps over the lazy dog";
        let mac = hashes(fox, Some("key"));
        assert_eq!(mac[0].1, "80070713463e7749b90c2dc24911e275");
        assert_eq!(mac[3].1, "f7bc83f430538424b13298e6aa6fb143ef4d59a14946175997479dbc2d1a3cd8");
        assert_eq!(&mac[5].1[..16], "b42af09057bac1e2");
    }

    #[test]
    fn hex_codec() {
        assert_eq!(hex_enc("Hi é"), "48 69 20 c3 a9");
        assert_eq!(hex_dec("48 69 20 c3 a9").unwrap(), "Hi é");
        assert_eq!(hex_dec("0x48:0x69").unwrap(), "Hi");
        assert_eq!(hex_dec(r"\x48\x69").unwrap(), "Hi");
        assert_eq!(hex_dec("4869").unwrap(), "Hi");
        assert!(hex_dec("486").is_err());
        assert!(hex_dec("zz").is_err());
        assert!(hex_dec("ff").is_err());
    }

    #[test]
    fn line_tools() {
        let o = |sort, dedupe| LineOpts { sort, dedupe, ignore_case: true, trim: true, drop_empty: true, seed: 7 };
        let r = process_lines("b\nfile10\n\n  B \nfile2\na", &o(LineSort::Natural, true));
        assert_eq!(r.text, "a\nb\nfile2\nfile10");
        assert_eq!((r.lines, r.dupes, r.empties), (4, 1, 1));
        assert_eq!(process_lines("b\na\nc", &o(LineSort::Desc, false)).text, "c\nb\na");
        assert_eq!(process_lines("bb\na\nccc", &o(LineSort::Length, false)).text, "a\nbb\nccc");
        let s = process_lines("1\n2\n3\n4\n5", &o(LineSort::Shuffle, false)).text;
        assert_eq!(s, process_lines("1\n2\n3\n4\n5", &o(LineSort::Shuffle, false)).text);
        let mut sorted: Vec<&str> = s.lines().collect();
        sorted.sort();
        assert_eq!(sorted, ["1", "2", "3", "4", "5"]);
    }

    #[test]
    fn url_parts() {
        let u = parse_url("https://user:p%40ss@Example.com:8443/a%20b/c?q=dev+toys&x=%26&flag#top").unwrap();
        let get = |k: &str| u.rows.iter().find(|r| r.0 == k).map(|r| r.1.clone());
        assert_eq!(get("Host").as_deref(), Some("example.com"));
        assert_eq!(get("Password").as_deref(), Some("p@ss"));
        assert_eq!(get("Port").as_deref(), Some("8443"));
        assert_eq!(get("Origin").as_deref(), Some("https://example.com:8443"));
        assert_eq!(get("Path").as_deref(), Some("/a b/c"));
        assert_eq!(get("Fragment").as_deref(), Some("top"));
        let params: Vec<(String, String)> =
            [("q", "dev toys"), ("x", "&"), ("flag", "")].iter().map(|(k, v)| (k.to_string(), v.to_string())).collect();
        assert_eq!(u.params, params);
        let v6 = parse_url("http://[::1]/").unwrap();
        assert_eq!(v6.rows.iter().find(|r| r.0 == "Port").unwrap().1, "80 (default)");
        assert!(parse_url("mailto:me@example.com").is_ok());
        assert!(parse_url("example.com/path").is_err());
        assert!(parse_url("http://host:99999/").is_err());
    }

    #[test]
    fn unicode() {
        let i = char_info('\u{200B}');
        assert!(i.flagged);
        assert_eq!((i.code.as_str(), i.kind, i.utf8.as_str()), ("U+200B", "ZERO WIDTH SPACE", "E2 80 8B"));
        assert_eq!(char_info('\n').shown, "LF");
        assert_eq!(char_info('\u{1}').shown, "^A");
        let e = unicode_escapes("é👋\"");
        assert_eq!(e[0].1, r#"\u00E9\uD83D\uDC4B\""#);
        assert_eq!(e[1].1, r#"\u{E9}\u{1F44B}\""#);
        assert_eq!(e[2].1, r#"\u00E9\U0001F44B\""#);
        assert_eq!(e[3].1, "&#xE9;&#x1F44B;&quot;");
        assert_eq!(unicode_stats("a\u{FEFF}é")[4].0, "1");
    }

    #[test]
    fn subnets() {
        let get = |input: &str, k: &str| subnet(input).unwrap().into_iter().find(|r| r.0 == k).unwrap().1;
        assert_eq!(get("192.168.1.130/26", "Network"), "192.168.1.128/26");
        assert_eq!(get("192.168.1.130/26", "Broadcast"), "192.168.1.191");
        assert_eq!(get("192.168.1.130/26", "Host range"), "192.168.1.129 – 192.168.1.190");
        assert_eq!(get("192.168.1.130/26", "Usable hosts"), "62");
        assert_eq!(get("10.0.0.1 255.255.0.0", "Network"), "10.0.0.0/16");
        assert_eq!(get("10.0.0.1/31", "Usable hosts"), "2");
        assert_eq!(get("8.8.8.8", "Type"), "Public");
        assert_eq!(get("0.0.0.0/0", "Total addresses"), "4,294,967,296");
        assert_eq!(get("2001:db8::1/64", "Last address"), "2001:db8::ffff:ffff:ffff:ffff");
        assert_eq!(get("2001:db8::1/64", "Type"), "Documentation (RFC 3849)");
        assert!(get("::1", "Reverse DNS").starts_with("1.0.0.0."));
        assert!(subnet("10.0.0.1/33").is_err());
        assert!(subnet("10.0.0.1 255.0.255.0").is_err());
        assert!(subnet("nope").is_err());
    }

    #[test]
    fn network_ids() {
        let m = mac_address(0, false, true);
        assert_eq!(m.len(), 17);
        let first = u8::from_str_radix(&m[..2], 16).unwrap();
        assert_eq!(first & 0b11, 0b10);
        assert_eq!(mac_address(2, true, true).len(), 14);
        let (p48, p64) = ula();
        assert!(p48.starts_with("fd") && p48.ends_with("::/48") && p64.ends_with("/64"));
    }

    #[test]
    fn sql_and_jsonpath() {
        assert_eq!(format_sql("select a, b from t where x = 1", 1, 2), "SELECT
  a,
  b
FROM
  t
WHERE
  x = 1");
        assert_eq!(format_sql("select a,
 b from t", 2, 2), "select a, b from t");
        let (v, locs) = jsonpath(r#"{"store":{"book":[{"title":"A","price":8},{"title":"B","price":12}]}}"#, "$.store.book[?@.price > 10].title").unwrap();
        assert_eq!(v, serde_json::json!(["B"]));
        assert_eq!(locs, ["$['store']['book'][1]['title']"]);
        assert!(jsonpath("{}", "$..[").is_err());
        assert!(jsonpath("{", "$").is_err());
    }

    #[test]
    fn yaml_and_toml() {
        let v = parse_yaml("base: &b {x: 1}\nitem:\n  <<: *b\n  y: [true, null]\n1: one").unwrap();
        assert_eq!(v, serde_json::json!({"base": {"x": 1}, "item": {"x": 1, "y": [true, null]}, "1": "one"}));
        assert!(parse_yaml("a: [1").is_err());
        let t = parse_toml("title = \"x\"\n[owner]\ndob = 1979-05-27T07:32:00Z\n").unwrap();
        assert_eq!(t, serde_json::json!({"title": "x", "owner": {"dob": "1979-05-27T07:32:00Z"}}));
        let back = to_toml(&serde_json::json!({"name": "a", "server": {"port": 80}})).unwrap();
        assert_eq!(back, "name = \"a\"\n\n[server]\nport = 80");
        assert!(to_toml(&serde_json::json!([1])).is_err());
        assert!(to_toml(&serde_json::json!({"a": null})).unwrap_err().contains("at a"));
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
        assert_eq!(detect("10.0.0.0/8").map(|d| d.0), Some(ToolId::Subnet));
        assert_eq!(detect("-----BEGIN CERTIFICATE-----\nMIIB\n-----END CERTIFICATE-----").map(|d| d.0), Some(ToolId::Cert));
        assert_eq!(detect("data:image/png;base64,iVBORw0KGgo=").map(|d| d.0), Some(ToolId::B64Image));
        assert_eq!(detect("https://example.com/search?q=a").map(|d| d.0), Some(ToolId::UrlParse));
        assert_eq!(detect("https://example.com/"), None);
        assert_eq!(detect("just some words"), None);
        assert_eq!(detect("12345"), None);
        assert_eq!(detect("password"), None);
    }

    #[test]
    fn uuid_shape() {
        let u = uuid_v4();
        assert_eq!(u.len(), 36);
        assert_eq!(&u[14..15], "4");
        let v7 = make_ids(IdKind::UuidV7, 20);
        assert!(v7.iter().all(|u| u.len() == 36 && &u[14..15] == "7"));
        assert!(v7.windows(2).all(|w| w[0] <= w[1]));
        let ul = ulid();
        assert_eq!(ul.len(), 26);
        assert!(ul.bytes().all(|b| CROCKFORD.contains(&b)));
        assert!(ul.as_bytes()[0] <= b'7');
    }
}
