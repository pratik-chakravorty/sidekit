//! Line diffs of text, and path-by-path diffs of JSON or YAML documents.

use serde_json::Value;
use similar::{ChangeTag, TextDiff};

#[derive(Clone, Copy, PartialEq, Debug)]
pub enum Side {
    Same,
    Removed,
    Added,
}

pub struct DiffLine {
    pub side: Side,
    pub old: Option<usize>,
    pub new: Option<usize>,
    pub text: String,
}

pub struct TextDiffOut {
    pub lines: Vec<DiffLine>,
    pub added: usize,
    pub removed: usize,
}

fn squash(s: &str) -> String {
    s.lines().map(|l| l.split_whitespace().collect::<Vec<_>>().join(" ")).collect::<Vec<_>>().join("\n")
}

/// A unified line diff. With `ignore_ws`, runs of whitespace compare equal
/// (the lines shown are the normalized ones).
pub fn text_diff(a: &str, b: &str, ignore_ws: bool) -> TextDiffOut {
    let (a, b) = if ignore_ws { (squash(a), squash(b)) } else { (a.replace("\r\n", "\n"), b.replace("\r\n", "\n")) };
    // Myers' diff is O(N·D): two large, very different texts could run for
    // minutes. Past the deadline `similar` settles for a coarser (still
    // correct) diff.
    let diff = TextDiff::configure().timeout(std::time::Duration::from_secs(2)).diff_lines(&a, &b);
    let mut out = TextDiffOut { lines: Vec::new(), added: 0, removed: 0 };
    for c in diff.iter_all_changes() {
        let side = match c.tag() {
            ChangeTag::Equal => Side::Same,
            ChangeTag::Delete => {
                out.removed += 1;
                Side::Removed
            }
            ChangeTag::Insert => {
                out.added += 1;
                Side::Added
            }
        };
        out.lines.push(DiffLine {
            side,
            old: c.old_index().map(|i| i + 1),
            new: c.new_index().map(|i| i + 1),
            text: c.value().trim_end_matches(['\n', '\r']).to_string(),
        });
    }
    out
}

#[derive(Clone, Copy, PartialEq, Debug)]
pub enum Change {
    Added,
    Removed,
    Changed,
}

pub struct PathChange {
    pub path: String,
    pub change: Change,
    pub old: Option<String>,
    pub new: Option<String>,
}

/// JSON first, then YAML, so either can be pasted on each side.
pub fn parse_doc(s: &str) -> Result<Value, String> {
    if s.trim().is_empty() {
        return Ok(Value::Null);
    }
    serde_json::from_str(s).or_else(|je| super::parse_yaml(s).map_err(|_| super::json_err(&je)))
}

fn short(v: &Value) -> String {
    let s = serde_json::to_string(v).unwrap_or_default();
    if s.chars().count() > 120 { s.chars().take(119).collect::<String>() + "…" } else { s }
}

fn key_path(base: &str, k: &str) -> String {
    let plain = !k.is_empty() && k.chars().all(|c| c.is_alphanumeric() || c == '_') && !k.starts_with(|c: char| c.is_ascii_digit());
    if plain { format!("{base}.{k}") } else { format!("{base}[{}]", serde_json::to_string(k).unwrap_or_default()) }
}

fn walk(path: &str, a: &Value, b: &Value, out: &mut Vec<PathChange>) {
    match (a, b) {
        (Value::Object(x), Value::Object(y)) => {
            for (k, va) in x {
                match y.get(k) {
                    Some(vb) => walk(&key_path(path, k), va, vb, out),
                    None => out.push(PathChange { path: key_path(path, k), change: Change::Removed, old: Some(short(va)), new: None }),
                }
            }
            for (k, vb) in y {
                if !x.contains_key(k) {
                    out.push(PathChange { path: key_path(path, k), change: Change::Added, old: None, new: Some(short(vb)) });
                }
            }
        }
        (Value::Array(x), Value::Array(y)) => {
            for i in 0..x.len().max(y.len()) {
                let p = format!("{path}[{i}]");
                match (x.get(i), y.get(i)) {
                    (Some(va), Some(vb)) => walk(&p, va, vb, out),
                    (Some(va), None) => out.push(PathChange { path: p, change: Change::Removed, old: Some(short(va)), new: None }),
                    (None, Some(vb)) => out.push(PathChange { path: p, change: Change::Added, old: None, new: Some(short(vb)) }),
                    (None, None) => {}
                }
            }
        }
        _ if a != b => out.push(PathChange { path: path.to_string(), change: Change::Changed, old: Some(short(a)), new: Some(short(b)) }),
        _ => {}
    }
}

/// Every path whose value differs. Object key order is ignored; arrays are
/// compared position by position.
pub fn structured_diff(a: &Value, b: &Value) -> Vec<PathChange> {
    let mut out = Vec::new();
    walk("$", a, b, &mut out);
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn lines() {
        let d = text_diff("a\nb\nc\n", "a\nB\nc\nd\n", false);
        assert_eq!((d.added, d.removed), (2, 1));
        let sides: Vec<Side> = d.lines.iter().map(|l| l.side).collect();
        assert_eq!(sides, [Side::Same, Side::Removed, Side::Added, Side::Same, Side::Added]);
        assert_eq!((d.lines[2].old, d.lines[2].new), (None, Some(2)));
        let ws = text_diff("a  b\nc", "a b\nc  ", true);
        assert_eq!(ws.added + ws.removed, 0);
    }

    #[test]
    fn structured() {
        let a = json!({"name": "api", "port": 80, "tags": ["a", "b"], "db": {"host": "x"}, "old": 1});
        let b = parse_doc("name: api\nport: 8080\ntags: [a]\ndb:\n  host: x\n  user: root\n\"weird key\": true").unwrap();
        let d = structured_diff(&a, &b);
        let got: Vec<(String, Change)> = d.iter().map(|c| (c.path.clone(), c.change)).collect();
        assert_eq!(got, [
            ("$.port".to_string(), Change::Changed),
            ("$.tags[1]".to_string(), Change::Removed),
            ("$.db.user".to_string(), Change::Added),
            ("$.old".to_string(), Change::Removed),
            ("$[\"weird key\"]".to_string(), Change::Added),
        ]);
        assert_eq!(d[0].old.as_deref(), Some("80"));
        assert!(structured_diff(&a, &a).is_empty());
        assert!(parse_doc("{").is_err());
    }
}
