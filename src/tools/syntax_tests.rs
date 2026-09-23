//! Exercise the bundled grammars and themes used by the code panes.
use gpui_kit::component::highlighter::{HighlightTheme, SyntaxHighlighter};
use gpui_kit::component::input::Rope;

fn assert_tokens(language: &str, source: &str, tokens: &[(&str, &str)]) {
    let mut highlighter = SyntaxHighlighter::new(language);
    assert!(highlighter.update(None, &Rope::from(source), None));
    for theme in [HighlightTheme::default_light(), HighlightTheme::default_dark()] {
        let styles = highlighter.styles(&(0..source.len()), &*theme);
        let mut end = 0;
        for (range, _) in &styles {
            assert_eq!(range.start, end, "highlight runs must cover the text without gaps");
            assert!(range.start < range.end && range.end <= source.len());
            assert!(source.is_char_boundary(range.start) && source.is_char_boundary(range.end));
            end = range.end;
        }
        assert_eq!(end, source.len());
        for (token, kind) in tokens {
            let offset = source.find(token).expect("sample must contain token");
            let (_, actual) = styles.iter().find(|(range, _)| range.contains(&offset)).unwrap();
            let expected = theme.style(kind).expect("theme must style this token kind");
            assert!(expected.color.is_some(), "{kind} needs a foreground color");
            assert_eq!(actual.color, expected.color, "{language}: {token} should be {kind}");
        }
    }
}

#[test]
fn json_properties_values_and_unicode_have_syntax_colors() {
    assert_tokens(
        "json",
        r#"{"name":"café 👋","count":-12.5e2,"enabled":true,"empty":null,"escaped":"a\nb"}"#,
        &[("name", "property"), ("café", "string"), ("-12.5e2", "number"),
          ("true", "boolean"), ("null", "constant"), ("\\n", "string.escape")],
    );
}

#[test]
fn yaml_keys_scalars_and_comments_have_syntax_colors() {
    assert_tokens(
        "yaml",
        "service: api-gateway\nreplicas: 3\nenabled: true\nempty: null\n# café 👋\nroutes:\n  - /users\n",
        &[("service", "property"), ("api-gateway", "string"), ("3", "number"),
          ("true", "boolean"), ("null", "constant"), ("# café", "comment")],
    );
}

#[test]
fn html_tags_attributes_and_comments_have_syntax_colors() {
    assert_tokens(
        "html",
        "<a href=\"/docs\">café 👋</a><!-- note -->",
        &[("a href", "tag"), ("href", "attribute"), ("/docs", "string"), ("<!--", "comment")],
    );
}

#[test]
fn incomplete_json_can_be_highlighted_and_then_replaced() {
    let mut highlighter = SyntaxHighlighter::new("json");
    let theme = HighlightTheme::default_dark();
    for source in [r#"{"name":"é 👋", "enabled": tru"#, r#"{"count":42}"#, ""] {
        assert!(highlighter.update(None, &Rope::from(source), None));
        let styles = highlighter.styles(&(0..source.len()), &*theme);
        for (range, _) in styles {
            assert!(range.end <= source.len());
            assert!(source.is_char_boundary(range.start) && source.is_char_boundary(range.end));
        }
        assert_eq!(highlighter.text().to_string(), source);
    }
}
