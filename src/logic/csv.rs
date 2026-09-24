//! CSV ↔ JSON. Nested objects become dotted column names (`address.city`)
//! and back; other nested values are written as JSON inside the cell.

use serde_json::{Map, Value};

/// The delimiter a CSV most likely uses, judged by its first line.
pub fn sniff_delimiter(text: &str) -> char {
    let first = text.lines().next().unwrap_or("");
    [',', ';', '\t', '|'].into_iter().max_by_key(|d| first.matches(*d).count()).filter(|d| first.contains(*d)).unwrap_or(',')
}

/// Split CSV into records, honouring quotes, doubled quotes and line breaks
/// inside quoted fields.
pub fn parse_records(text: &str, delim: char) -> Result<Vec<Vec<String>>, String> {
    let mut rows = Vec::new();
    let mut row = Vec::new();
    let mut field = String::new();
    let mut quoted = false;
    let mut line = 1;
    let mut it = text.chars().peekable();
    let mut any = false;
    while let Some(c) = it.next() {
        any = true;
        if quoted {
            match c {
                '"' if it.peek() == Some(&'"') => {
                    field.push('"');
                    it.next();
                }
                '"' => quoted = false,
                '\n' => {
                    line += 1;
                    field.push(c);
                }
                _ => field.push(c),
            }
            continue;
        }
        match c {
            '"' if field.is_empty() => quoted = true,
            c if c == delim => row.push(std::mem::take(&mut field)),
            '\r' if it.peek() == Some(&'\n') => {}
            '\n' => {
                line += 1;
                row.push(std::mem::take(&mut field));
                rows.push(std::mem::take(&mut row));
                any = false;
            }
            _ => field.push(c),
        }
    }
    if quoted {
        return Err(format!("A quoted field starting before line {line} is never closed"));
    }
    if any {
        row.push(field);
        rows.push(row);
    }
    rows.retain(|r| !(r.len() == 1 && r[0].is_empty()));
    Ok(rows)
}

fn infer(cell: &str) -> Value {
    let t = cell.trim();
    match t {
        "" => Value::String(String::new()),
        "true" | "TRUE" | "True" => Value::Bool(true),
        "false" | "FALSE" | "False" => Value::Bool(false),
        "null" | "NULL" => Value::Null,
        _ => {
            // Leading zeros are identifiers (zip codes, phone numbers), not numbers.
            let leading_zero = t.len() > 1 && t.starts_with('0') && !t.starts_with("0.");
            if !leading_zero {
                if let Ok(i) = t.parse::<i64>() {
                    return i.into();
                }
                if let Some(n) = t.parse::<f64>().ok().filter(|f| f.is_finite()).and_then(serde_json::Number::from_f64) {
                    return Value::Number(n);
                }
            }
            Value::String(cell.to_string())
        }
    }
}

fn set_path(obj: &mut Map<String, Value>, key: &str, v: Value) {
    match key.split_once('.') {
        Some((head, rest)) if !head.is_empty() && !rest.is_empty() => {
            let child = obj.entry(head.to_string()).or_insert_with(|| Value::Object(Map::new()));
            if let Value::Object(m) = child {
                set_path(m, rest, v);
            } else {
                obj.insert(key.to_string(), v);
            }
        }
        _ => {
            obj.insert(key.to_string(), v);
        }
    }
}

/// CSV with a header row → an array of objects.
pub fn csv_to_json(text: &str, types: bool) -> Result<Value, String> {
    let delim = sniff_delimiter(text);
    let rows = parse_records(text, delim)?;
    let Some((header, body)) = rows.split_first() else { return Ok(Value::Array(Vec::new())) };
    let out = body
        .iter()
        .map(|r| {
            let mut obj = Map::new();
            for (i, key) in header.iter().enumerate() {
                let cell = r.get(i).map(String::as_str).unwrap_or("");
                let v = if types { infer(cell) } else { Value::String(cell.to_string()) };
                set_path(&mut obj, key.trim(), v);
            }
            Value::Object(obj)
        })
        .collect();
    Ok(Value::Array(out))
}

fn flatten(prefix: &str, v: &Value, out: &mut Vec<(String, Value)>) {
    match v {
        Value::Object(m) if !m.is_empty() => {
            for (k, x) in m {
                let key = if prefix.is_empty() { k.clone() } else { format!("{prefix}.{k}") };
                flatten(&key, x, out);
            }
        }
        _ => out.push((prefix.to_string(), v.clone())),
    }
}

fn cell(v: &Value) -> String {
    match v {
        Value::Null => String::new(),
        Value::String(s) => s.clone(),
        Value::Bool(b) => b.to_string(),
        Value::Number(n) => n.to_string(),
        other => serde_json::to_string(other).unwrap_or_default(),
    }
}

fn quote(s: &str, delim: char) -> String {
    if s.contains([delim, '"', '\n', '\r']) || s.starts_with(' ') || s.ends_with(' ') {
        format!("\"{}\"", s.replace('"', "\"\""))
    } else {
        s.to_string()
    }
}

/// An array of objects (or one object) → CSV with a header row. Columns keep
/// their first-seen order across all rows.
pub fn json_to_csv(v: &Value, delim: char) -> Result<String, String> {
    let rows: Vec<&Value> = match v {
        Value::Array(a) => a.iter().collect(),
        Value::Object(_) => vec![v],
        _ => return Err("CSV needs an array of objects (or a single object)".into()),
    };
    let mut columns: Vec<String> = Vec::new();
    let mut flat_rows = Vec::new();
    for (i, r) in rows.iter().enumerate() {
        if !r.is_object() {
            return Err(format!("Item {i} is not an object, so it has no columns"));
        }
        let mut flat = Vec::new();
        flatten("", r, &mut flat);
        for (k, _) in &flat {
            if !columns.contains(k) {
                columns.push(k.clone());
            }
        }
        flat_rows.push(flat);
    }
    let d = delim.to_string();
    let mut out = columns.iter().map(|c| quote(c, delim)).collect::<Vec<_>>().join(&d);
    for flat in flat_rows {
        out.push('\n');
        let line = columns
            .iter()
            .map(|c| flat.iter().find(|(k, _)| k == c).map(|(_, v)| quote(&cell(v), delim)).unwrap_or_default())
            .collect::<Vec<_>>()
            .join(&d);
        out.push_str(&line);
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn round_trip() {
        let v = json!([{"id": 1, "name": "Ada, L.", "address": {"city": "London"}, "tags": ["a"]}, {"id": 2, "name": "Bob \"B\"", "zip": "02134"}]);
        let csv = json_to_csv(&v, ',').unwrap();
        assert_eq!(csv, "id,name,address.city,tags,zip\n1,\"Ada, L.\",London,\"[\"\"a\"\"]\",\n2,\"Bob \"\"B\"\"\",,,02134");
        let back = csv_to_json(&csv, true).unwrap();
        assert_eq!(back[0]["address"]["city"], "London");
        assert_eq!(back[1]["zip"], "02134");
        assert_eq!(back[1]["id"], 2);
    }

    #[test]
    fn parsing() {
        assert_eq!(sniff_delimiter("a;b;c\n1;2;3"), ';');
        assert_eq!(parse_records("a,\"x\ny\"\r\n1,2\n", ',').unwrap(), vec![vec!["a", "x\ny"], vec!["1", "2"]]);
        assert!(parse_records("a,\"open", ',').is_err());
        assert_eq!(csv_to_json("a\tb\n1\ttrue", true).unwrap(), json!([{"a": 1, "b": true}]));
        assert!(json_to_csv(&json!([1, 2]), ',').is_err());
    }
}
