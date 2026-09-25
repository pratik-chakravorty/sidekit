//! Workflows: tools chained so each step's output is the next step's input.
//! Every step is text in, text or an error out, so steps compose freely.

use std::sync::LazyLock;

use serde::{Deserialize, Serialize};
use serde_json::Value;

use super::{Indent, json_err, to_json};

/// What a step's single setting looks like.
pub enum Arg {
    None,
    /// Free text, with a placeholder and the value a new step starts with.
    Text(&'static str, &'static str),
    /// One of a fixed list; the first is the default.
    Choice(&'static [&'static str]),
}

pub struct Op {
    /// Stored in workflow files, so never rename one.
    pub key: &'static str,
    pub label: &'static str,
    pub arg: Arg,
    pub run: fn(&str, &str) -> Result<String, String>,
}

impl Op {
    pub fn default_arg(&self) -> &'static str {
        match self.arg {
            Arg::None => "",
            Arg::Text(_, d) => d,
            Arg::Choice(c) => c[0],
        }
    }
}

const INDENTS: &[&str] = &["2 spaces", "4 spaces", "Tab"];
const HASHES: &[&str] = &["SHA-256", "SHA-1", "SHA-512", "MD5", "CRC-32"];

fn bad(what: &str) -> impl Fn(()) -> String + '_ {
    move |()| format!("Not valid {what}")
}

fn parse(s: &str) -> Result<Value, String> {
    serde_json::from_str(s.trim()).map_err(|e| format!("Not valid JSON: {}", json_err(&e)))
}

fn pretty(v: &Value) -> String {
    to_json(v, &Indent::Spaces(2))
}

/// A JSON string comes out bare, so the next step gets the text itself.
fn bare(v: &Value) -> String {
    match v {
        Value::String(s) => s.clone(),
        other => pretty(other),
    }
}

pub const OPS: &[Op] = &[
    Op { key: "b64enc", label: "Base64 encode", arg: Arg::None, run: |s, _| Ok(super::b64_enc(s)) },
    Op { key: "b64dec", label: "Base64 decode", arg: Arg::None, run: |s, _| super::b64_dec(s).map_err(bad("Base64 text")) },
    Op {
        key: "b64urldec",
        label: "Base64URL decode",
        arg: Arg::None,
        run: |s, _| {
            let clean: String = s.chars().filter(|c| !c.is_whitespace()).collect();
            let bytes = super::b64url_decode(&clean).map_err(bad("Base64URL"))?;
            String::from_utf8(bytes).map_err(|_| "Decoded bytes are not UTF-8 text".into())
        },
    },
    Op { key: "urlenc", label: "URL encode", arg: Arg::None, run: |s, _| Ok(super::url_enc(s)) },
    Op { key: "urldec", label: "URL decode", arg: Arg::None, run: |s, _| super::url_dec(s.trim()).map_err(bad("percent-encoding")) },
    Op { key: "htmlenc", label: "HTML encode", arg: Arg::None, run: |s, _| Ok(super::html_enc(s)) },
    Op { key: "htmldec", label: "HTML decode", arg: Arg::None, run: |s, _| Ok(super::html_dec(s)) },
    Op { key: "hexenc", label: "Hex encode", arg: Arg::None, run: |s, _| Ok(super::hex_enc(s)) },
    Op { key: "hexdec", label: "Hex decode", arg: Arg::None, run: |s, _| super::hex_dec(s).map_err(bad("hex text")) },
    Op { key: "escape", label: "Escape string", arg: Arg::None, run: |s, _| Ok(super::esc_enc(s)) },
    Op { key: "unescape", label: "Unescape string", arg: Arg::None, run: |s, _| super::esc_dec(s).map_err(bad("escape sequences")) },
    Op { key: "param", label: "Get query parameter", arg: Arg::Text("Parameter name", "token"), run: query_param },
    Op { key: "jwt", label: "JWT payload", arg: Arg::None, run: jwt_payload },
    Op {
        key: "jsonfmt",
        label: "Format JSON",
        arg: Arg::Choice(INDENTS),
        run: |s, a| {
            let indent = match a {
                "4 spaces" => Indent::Spaces(4),
                "Tab" => Indent::Tab,
                _ => Indent::Spaces(2),
            };
            Ok(to_json(&parse(s)?, &indent))
        },
    },
    Op { key: "jsonmin", label: "Minify JSON", arg: Arg::None, run: |s, _| Ok(to_json(&parse(s)?, &Indent::Minified)) },
    Op { key: "jsonsort", label: "Sort JSON keys", arg: Arg::None, run: |s, _| Ok(pretty(&super::sort_keys(parse(s)?))) },
    Op { key: "jsonpath", label: "JSONPath query", arg: Arg::Text("$.path.to.value", "$"), run: jsonpath },
    Op { key: "json2yaml", label: "JSON → YAML", arg: Arg::None, run: |s, _| Ok(super::to_yaml(&parse(s)?, 0, 2)) },
    Op { key: "yaml2json", label: "YAML → JSON", arg: Arg::None, run: |s, _| Ok(pretty(&super::parse_yaml(s)?)) },
    Op { key: "json2toml", label: "JSON → TOML", arg: Arg::None, run: |s, _| super::to_toml(&parse(s)?) },
    Op { key: "toml2json", label: "TOML → JSON", arg: Arg::None, run: |s, _| Ok(pretty(&super::parse_toml(s)?)) },
    Op { key: "hash", label: "Hash", arg: Arg::Choice(HASHES), run: hash },
    Op { key: "regex", label: "Regex extract", arg: Arg::Text("Pattern, e.g. id=(\\d+)", ""), run: regex_extract },
    Op { key: "trim", label: "Trim whitespace", arg: Arg::None, run: |s, _| Ok(s.trim().to_string()) },
    Op { key: "upper", label: "UPPERCASE", arg: Arg::None, run: |s, _| Ok(s.to_uppercase()) },
    Op { key: "lower", label: "lowercase", arg: Arg::None, run: |s, _| Ok(s.to_lowercase()) },
    Op {
        key: "sortlines",
        label: "Sort lines",
        arg: Arg::None,
        run: |s, _| {
            let mut v: Vec<&str> = s.lines().collect();
            v.sort_by(|a, b| super::natural_cmp(a, b));
            Ok(v.join("\n"))
        },
    },
    Op {
        key: "dedupe",
        label: "Remove duplicate lines",
        arg: Arg::None,
        run: |s, _| {
            let mut seen = std::collections::HashSet::new();
            Ok(s.lines().filter(|l| seen.insert(*l)).collect::<Vec<_>>().join("\n"))
        },
    },
];

/// Op labels in `OPS` order, for the step picker.
pub static LABELS: LazyLock<Vec<&'static str>> = LazyLock::new(|| OPS.iter().map(|o| o.label).collect());

pub fn op(key: &str) -> Option<(usize, &'static Op)> {
    OPS.iter().enumerate().find(|(_, o)| o.key == key)
}

/// `token` from `token=abc`, `a=1&token=abc` or a full URL. Values are form-decoded.
fn query_param(s: &str, name: &str) -> Result<String, String> {
    let name = name.trim();
    if name.is_empty() {
        return Err("Enter a parameter name".into());
    }
    let s = s.trim();
    let query = s.split_once('?').map_or(s, |(_, q)| q);
    let query = query.split('#').next().unwrap_or(query);
    let dec = |x: &str| {
        let x = x.replace('+', " ");
        super::url_dec(&x).unwrap_or(x)
    };
    query
        .split(['&', ';', '\n'])
        .filter_map(|pair| pair.trim().split_once('='))
        .find(|(k, _)| dec(k) == name)
        .map(|(_, v)| dec(v))
        .ok_or_else(|| format!("No parameter named \"{name}\""))
}

fn jwt_payload(s: &str, _: &str) -> Result<String, String> {
    let token = s.trim().trim_start_matches("Bearer ").trim();
    let payload = token.split('.').nth(1).ok_or("Not a JWT: expected three parts separated by dots")?;
    let bytes = super::b64url_decode(payload).map_err(|()| "The payload is not valid Base64URL".to_string())?;
    let v: Value = serde_json::from_slice(&bytes).map_err(|e| format!("The payload is not JSON: {}", json_err(&e)))?;
    Ok(pretty(&v))
}

fn jsonpath(s: &str, path: &str) -> Result<String, String> {
    let (Value::Array(found), _) = super::jsonpath(s, path)? else { unreachable!() };
    match found.as_slice() {
        [] => Err(format!("Nothing matches {}", path.trim())),
        [one] => Ok(bare(one)),
        _ => Ok(pretty(&Value::Array(found))),
    }
}

fn hash(s: &str, algo: &str) -> Result<String, String> {
    super::hashes(s, None)
        .into_iter()
        .find(|(name, _)| *name == algo)
        .map(|(_, h)| h)
        .ok_or_else(|| format!("Unknown hash {algo}"))
}

/// Every match on its own line: the first capture group when there is one.
fn regex_extract(s: &str, pattern: &str) -> Result<String, String> {
    if pattern.is_empty() {
        return Err("Enter a pattern".into());
    }
    let re = fancy_regex::Regex::new(pattern).map_err(|e| format!("Invalid pattern: {e}"))?;
    let mut out = Vec::new();
    for caps in re.captures_iter(s) {
        let caps = caps.map_err(|e| e.to_string())?;
        if let Some(m) = caps.get(1).or_else(|| caps.get(0)) {
            out.push(m.as_str());
        }
    }
    if out.is_empty() {
        return Err("No matches".into());
    }
    Ok(out.join("\n"))
}

// ---------------------------------------------------------------- workflows

#[derive(Clone, Serialize, Deserialize, PartialEq, Debug)]
pub struct Step {
    pub op: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub arg: String,
    /// Off steps pass their input straight through.
    #[serde(default = "yes", skip_serializing_if = "is_true")]
    pub on: bool,
}

fn yes() -> bool {
    true
}

fn is_true(b: &bool) -> bool {
    *b
}

impl Step {
    pub fn new(op: &Op) -> Self {
        Self { op: op.key.into(), arg: op.default_arg().into(), on: true }
    }
}

#[derive(Clone, Serialize, Deserialize, PartialEq, Debug, Default)]
pub struct Workflow {
    pub name: String,
    pub steps: Vec<Step>,
}

#[derive(Clone, PartialEq, Debug)]
pub enum Outcome {
    Ok(String),
    Err(String),
    /// Switched off by the user.
    Off,
    /// Not run because an earlier step failed.
    Skipped,
}

/// Run every step in order. Returns each step's outcome and the final output,
/// which is `None` when a step failed.
pub fn run(input: &str, steps: &[Step]) -> (Vec<Outcome>, Option<String>) {
    let mut cur = input.to_string();
    let mut failed = false;
    let outcomes = steps
        .iter()
        .map(|s| {
            if failed {
                return Outcome::Skipped;
            }
            if !s.on {
                return Outcome::Off;
            }
            let Some((_, op)) = op(&s.op) else {
                failed = true;
                return Outcome::Err(format!("Unknown step \"{}\"", s.op));
            };
            match (op.run)(&cur, &s.arg) {
                Ok(out) => {
                    cur = out;
                    Outcome::Ok(cur.clone())
                }
                Err(e) => {
                    failed = true;
                    Outcome::Err(e)
                }
            }
        })
        .collect();
    (outcomes, (!failed).then_some(cur))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn step(op: &str, arg: &str) -> Step {
        Step { op: op.into(), arg: arg.into(), on: true }
    }

    #[test]
    fn keys_are_unique() {
        let mut keys: Vec<_> = OPS.iter().map(|o| o.key).collect();
        keys.sort();
        keys.dedup();
        assert_eq!(keys.len(), OPS.len());
    }

    #[test]
    fn log_token_chain() {
        let steps = [step("param", "token"), step("urldec", ""), step("b64dec", ""), step("jsonfmt", "2 spaces")];
        let (outs, out) = run("token=eyJ1c2VyIjoicHJhdGlrIiwicm9sZSI6ImFkbWluIn0%3D", &steps);
        assert!(outs.iter().all(|o| matches!(o, Outcome::Ok(_))), "{outs:?}");
        assert_eq!(out.unwrap(), "{\n  \"user\": \"pratik\",\n  \"role\": \"admin\"\n}");
    }

    #[test]
    fn failure_stops_the_chain() {
        let steps = [step("b64dec", ""), step("upper", "")];
        let (outs, out) = run("not base64!!", &steps);
        assert!(matches!(outs[0], Outcome::Err(_)));
        assert_eq!(outs[1], Outcome::Skipped);
        assert_eq!(out, None);
    }

    #[test]
    fn off_steps_pass_through() {
        let steps = [Step { on: false, ..step("upper", "") }, step("b64enc", "")];
        let (outs, out) = run("hi", &steps);
        assert_eq!(outs[0], Outcome::Off);
        assert_eq!(out.unwrap(), "aGk=");
    }

    #[test]
    fn jsonpath_single_string_is_bare() {
        let (_, out) = run(r#"{"data":{"password":"aHVudGVyMg=="}}"#, &[step("jsonpath", "$.data.password"), step("b64dec", "")]);
        assert_eq!(out.unwrap(), "hunter2");
    }

    #[test]
    fn query_params() {
        assert_eq!(query_param("https://x.io/cb?code=a%20b&state=1#frag", "code").unwrap(), "a b");
        assert_eq!(query_param("a=1&b=2", "b").unwrap(), "2");
        assert!(query_param("a=1", "z").is_err());
    }

    #[test]
    fn jwt_and_hash_and_regex() {
        let t = "Bearer eyJhbGciOiJIUzI1NiJ9.eyJzdWIiOiIxMjMifQ.sig";
        assert_eq!(jwt_payload(t, "").unwrap(), "{\n  \"sub\": \"123\"\n}");
        assert_eq!(hash("abc", "MD5").unwrap(), "900150983cd24fb0d6963f7d28e17f72");
        assert_eq!(regex_extract("id=1, id=22", r"id=(\d+)").unwrap(), "1\n22");
    }

    #[test]
    fn file_format() {
        let w = Workflow { name: "x".into(), steps: vec![step("upper", ""), Step { on: false, ..step("param", "t") }] };
        let json = serde_json::to_string(&w).unwrap();
        assert_eq!(json, r#"{"name":"x","steps":[{"op":"upper"},{"op":"param","arg":"t","on":false}]}"#);
        assert_eq!(serde_json::from_str::<Workflow>(&json).unwrap(), w);
    }
}
