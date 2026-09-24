//! XML well-formedness checks, pretty-printing and minifying.

use quick_xml::Reader;
use quick_xml::Writer;
use quick_xml::events::Event;

fn line_col(src: &str, pos: usize) -> (usize, usize) {
    let before = &src[..src.floor_char_boundary(pos.min(src.len()))];
    let line = before.matches('\n').count() + 1;
    let col = before.rsplit('\n').next().map_or(0, |l| l.chars().count()) + 1;
    (line, col)
}

/// Re-write `src` with `indent` spaces per level, or on one line when `None`.
/// Fails with a position when the document is not well-formed.
pub fn format_xml(src: &str, indent: Option<usize>) -> Result<String, String> {
    // Text is kept exactly as written (trimming would also eat the spaces
    // around entities like `&amp;`); only whitespace between tags is dropped.
    let mut r = Reader::from_str(src);
    let mut w = match indent {
        Some(n) => Writer::new_with_indent(Vec::new(), b' ', n),
        None => Writer::new(Vec::new()),
    };
    let mut open: Vec<String> = Vec::new();
    let mut roots = 0;
    loop {
        let ev = match r.read_event() {
            Ok(Event::Eof) => break,
            Ok(ev) => ev,
            Err(e) => {
                let (l, c) = line_col(src, r.error_position() as usize);
                let msg = e.to_string();
                let mut m = msg.chars();
                let msg = m.next().map(|f| f.to_uppercase().collect::<String>() + m.as_str()).unwrap_or(msg);
                return Err(format!("{msg} (line {l}, column {c})"));
            }
        };
        match &ev {
            Event::Start(s) => {
                if open.is_empty() {
                    roots += 1;
                }
                open.push(s.name().as_ref().to_string());
            }
            Event::Empty(_) if open.is_empty() => roots += 1,
            Event::End(_) => {
                open.pop();
            }
            Event::Text(t) if t.as_ref().trim().is_empty() => continue,
            Event::Text(_) if open.is_empty() => {
                let (l, c) = line_col(src, r.buffer_position() as usize);
                return Err(format!("Text outside the root element (line {l}, column {c})"));
            }
            _ => {}
        }
        w.write_event(ev).map_err(|e| e.to_string())?;
    }
    if let Some(name) = open.last() {
        return Err(format!("<{name}> is never closed"));
    }
    if roots == 0 {
        return Err("There is no root element".into());
    }
    if roots > 1 {
        return Err(format!("An XML document has one root element; this has {roots}"));
    }
    String::from_utf8(w.into_inner()).map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn formats_and_validates() {
        let src = "<?xml version=\"1.0\"?><a x=\"1\"><b>hi &amp; bye</b><c/><!-- note --></a>";
        assert_eq!(
            format_xml(src, Some(2)).unwrap(),
            "<?xml version=\"1.0\"?>\n<a x=\"1\">\n  <b>hi &amp; bye</b>\n  <c/>\n  <!-- note -->\n</a>"
        );
        assert_eq!(format_xml("<a>\n  <b> x </b>\n</a>", None).unwrap(), "<a><b> x </b></a>");
        let err = format_xml("<a>\n<b></c></a>", Some(2)).unwrap_err();
        assert!(err.contains("line 2"), "{err}");
        assert!(format_xml("<a><b></b>", Some(2)).unwrap_err().contains("<a> is never closed"));
        assert!(format_xml("<a/><b/>", Some(2)).is_err());
    }
}
