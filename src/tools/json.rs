use gpui_kit::component::input::{Input, InputEvent, InputState, EditorState};
use gpui_kit::{
    App, AppContext, Context, Entity, InteractiveElement, IntoElement, ParentElement, Render,
    SharedString, Styled, Subscription, Task, Window, div, prelude::FluentBuilder, px,
};

use super::*;
use super::big::{self, has_long_line};
use crate::logic::{self, Indent, plural};
use crate::registry::ToolId;
use crate::ui::{self, Tone};

/// Output state shared by the JSON tools.
#[derive(Default)]
struct Out {
    /// The full result; Copy takes this even when the editor shows a shortened view.
    text: SharedString,
    err: Option<String>,
    status: String,
    tone: Option<Tone>,
}

/// A finished computation, ready to show.
struct Done {
    out: Out,
    /// What the output pane shows when the full result is too large to edit comfortably.
    shown: Option<String>,
    /// A re-laid copy of an input whose lines are too long to edit comfortably.
    input: Option<String>,
}

fn finish(src: &str, compute: impl FnOnce(&str) -> Out) -> Done {
    if src.trim().is_empty() {
        let out = Out { status: "Waiting for input".into(), ..Default::default() };
        return Done { out, shown: None, input: None };
    }
    let mut out = compute(src);
    let shown = big::for_display(&out.text);
    if shown.is_some() {
        out.status.push_str(" · ");
        out.status.push_str(big::SHORTENED);
    }
    // Minified input: pretty-print it so the input pane stays responsive.
    let input = if has_long_line(src) {
        serde_json::from_str::<serde_json::Value>(src)
            .ok()
            .map(|v| logic::to_json(&v, &Indent::Spaces(2)))
            .filter(|s| !has_long_line(s))
    } else {
        None
    };
    Done { out, shown, input }
}

/// The input and output editors of a JSON tool and the computation between them.
struct Io {
    input: Entity<EditorState>,
    output: Entity<EditorState>,
    out: Out,
    finder: Finder,
    task: Option<Task<()>>,
    /// Input and output languages, for when a document is small enough to highlight.
    langs: (&'static str, &'static str),
}

impl Io {
    fn new<V: 'static>(
        sample: &str,
        (in_lang, out_lang): (&'static str, &'static str),
        window: &mut Window,
        cx: &mut Context<V>,
    ) -> (Self, Subscription) {
        let input = code_editor(sample, "Paste or type JSON", in_lang, window, cx);
        let output = code_editor("", "", out_lang, window, cx);
        let (finder, find_sub) = Finder::new(&output, window, cx);
        (Self { input, output, out: Out::default(), finder, task: None, langs: (in_lang, out_lang) }, find_sub)
    }

    /// Recompute the output from the input: right away for small inputs, in the
    /// background after a pause in typing for large ones.
    fn run<V: 'static>(
        &mut self,
        window: &mut Window,
        cx: &mut Context<V>,
        io: fn(&mut V) -> &mut Io,
        compute: impl FnOnce(&str) -> Out + Send + 'static,
    ) {
        let text = self.input.read(cx).text().clone();
        if text.len() <= big::LARGE {
            self.task = None;
            let done = finish(&text.to_string(), compute);
            self.apply(done, window, cx);
            return;
        }
        self.out.status = "Working…".into();
        self.out.tone = None;
        cx.notify();
        // Replacing the task drops (cancels) the one still waiting.
        self.task = Some(cx.spawn_in(window, async move |this, cx| {
            cx.background_executor().timer(big::DEBOUNCE).await;
            let done = cx
                .background_executor()
                .spawn(async move { finish(&text.to_string(), compute) })
                .await;
            let _ = this.update_in(cx, |v, window, cx| io(v).apply(done, window, cx));
        }));
    }

    fn apply<V: 'static>(&mut self, done: Done, window: &mut Window, cx: &mut Context<V>) {
        if let Some(input) = done.input {
            set_text(&self.input, &input, window, cx);
        }
        self.out = done.out;
        let shown = done.shown.as_deref().unwrap_or(&self.out.text);
        // Syntax trees for multi-megabyte documents cost seconds and hundreds of MB.
        let in_len = self.input.read(cx).text().len();
        big::fit_language(&self.input, self.langs.0, in_len, cx);
        big::fit_language(&self.output, self.langs.1, shown.len(), cx);
        set_text(&self.output, shown, window, cx);
        self.finder.refresh(cx);
        cx.notify();
    }
}

/// Find-in-output: a small search box that drives the read-only result editor's
/// own search engine, so matches are highlighted and scrolled into view.
struct Finder {
    input: Entity<InputState>,
    output: Entity<EditorState>,
}

impl Finder {
    fn new<V: 'static>(output: &Entity<EditorState>, window: &mut Window, cx: &mut Context<V>) -> (Self, Subscription) {
        let input = cx.new(|cx| InputState::new(window, cx).placeholder("Find in output").clean_on_escape());
        let target = output.clone();
        let sub = cx.subscribe_in(&input, window, move |_, input, ev: &InputEvent, _, cx| {
            match ev {
                InputEvent::Change => {
                    let query = input.read(cx).value().to_string();
                    find(&target, &query, cx);
                }
                InputEvent::PressEnter { shift, .. } => step(&target, !*shift, cx),
                _ => {}
            }
            cx.notify();
        });
        (Self { input, output: output.clone() }, sub)
    }

    /// Re-run the current query after the output text changed.
    fn refresh(&self, cx: &mut App) {
        let query = self.input.read(cx).value().to_string();
        if !query.is_empty() {
            find(&self.output, &query, cx);
        }
    }
}

fn find(output: &Entity<EditorState>, query: &str, cx: &mut App) {
    let query = query.to_string();
    output.update(cx, |s, cx| {
        if query.is_empty() {
            s.close_search(cx);
            return;
        }
        s.set_search_query(query, true, cx);
        // Land on (and scroll to) the first match.
        if s.next_search_match(cx).is_some() && s.search_session().matcher.len() > 1 {
            s.previous_search_match(cx);
        }
    });
}

fn step(output: &Entity<EditorState>, forward: bool, cx: &mut App) {
    output.update(cx, |s, cx| {
        if forward {
            s.next_search_match(cx);
        } else {
            s.previous_search_match(cx);
        }
    });
}

/// The search box in the output pane header: field, "2 of 5" and prev/next.
fn finder_el<V: 'static>(id: &'static str, finder: &Finder, window: &mut Window, cx: &mut Context<V>) -> gpui_kit::AnyElement {
    let p = Pal::get(cx);
    let focused = is_focused(&finder.input, window, cx);
    let has_query = !finder.input.read(cx).value().is_empty();
    let session = finder.output.read(cx).search_session();
    let (count, current) = (session.matcher.len(), session.matcher.current());
    let label = match current {
        Some(i) => format!("{} of {}", i + 1, count),
        None => "No matches".into(),
    };
    let nav = |dir: &'static str, forward: bool| {
        let target = finder.output.clone();
        ui::icon_btn(crate::id!("{id}-find-{dir}"), dir, &p, move |_, _, cx| step(&target, forward, cx))
            .size(px(22.))
            .rounded(px(4.))
    };
    div()
        .flex()
        .items_center()
        .gap(px(4.))
        .h(px(28.))
        .w(px(if has_query { 300. } else { 220. }))
        .pl(px(8.))
        .pr(px(3.))
        .mr(px(4.))
        .rounded(px(6.))
        .bg(p.editor)
        .border_1()
        .border_color(if focused { p.accent } else { p.stroke_strong })
        .child(crate::icons::icon("search", 13., if focused { p.accent } else { p.text3 }))
        .child(
            div()
                .flex_1()
                .min_w_0()
                .text_size(px(12.5))
                .child(Input::new(&finder.input).appearance(false).text_size(px(12.5)).h(px(24.))),
        )
        .when(has_query, |d| {
            d.child(
                div()
                    .flex_none()
                    .text_size(px(11.5))
                    .text_color(if count == 0 { p.danger } else { p.text3 })
                    .child(label),
            )
            .child(nav("chev-up", false))
            .child(nav("chev-down", true))
        })
        .into_any_element()
}

fn panes<V: 'static>(
    id: &'static str,
    (in_title, out_title): (&'static str, &'static str),
    io: &Io,
    window: &mut Window,
    cx: &mut Context<V>,
    recompute: impl Fn(&mut V, &mut Window, &mut Context<V>) + Clone + 'static,
) -> gpui_kit::AnyElement {
    let Io { input, output, finder, out, .. } = io;
    let pal = Pal::get(cx);
    let [paste, clear] = paste_clear(id, input, &pal, cx, recompute);
    let search = finder_el(id, finder, window, cx);
    let finder_input = finder.input.clone();
    let zoom = ui::PaneZoom::new(crate::id!("{id}-output"), window, cx);
    div()
        .flex()
        .gap(px(12.))
        .flex_1()
        .min_h(px(340.))
        .mt(px(8.))
        .child(
            ui::pane(is_focused(input, window, cx), &pal)
                .flex_1()
                .child(ui::pane_head(in_title, None, &pal).child(paste).child(clear))
                .child(code_editor_el(input, false, cx)),
        )
        .child(zoom.wrap(
            ui::pane(is_focused(output, window, cx), &pal)
                .flex_1()
                // Ctrl+F inside the output searches the output rather than the tool list.
                .on_action(move |_: &crate::app::FocusSearch, window, cx| {
                    finder_input.update(cx, |s, cx| s.focus(window, cx));
                })
                .child(ui::pane_head(out_title, None, &pal).child(search).child(ui::copy_btn(
                    crate::id!("{id}-copy"),
                    out.text.clone(),
                    &pal,
                    window,
                    cx,
                )).child(zoom.button(&pal)))
                .when_some(out.err.clone(), |d, e| d.child(ui::err_box(e, &pal).m(px(12.))))
                .child(code_editor_el(output, true, cx))
                .child(ui::pane_foot(&pal).child(ui::dot(out.tone, &pal)).child(out.status.clone())),
            &pal, window,
        ))
        .into_any_element()
}

// ------------------------------------------------------------ JSON formatter

pub const INDENTS: &[&str] = &["2 spaces", "4 spaces", "1 tab", "Minified"];

pub struct JsonFmtView {
    io: Io,
    indent: usize,
    sort: bool,
    wrap: bool,
    _subs: Vec<Subscription>,
}

impl JsonFmtView {
    pub fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let (io, find_sub) = Io::new(
            r#"{"name":"SideKit","version":2,"offline":true,"tags":["developer","utilities","open-source"],"platforms":{"windows":true,"macos":true,"linux":true}}"#,
            ("json", "json"),
            window,
            cx,
        );
        let subs = vec![
            watch(&io.input, window, cx, Self::recompute),
            watch(&io.output, window, cx, |_, _, _| {}),
            find_sub,
        ];
        let mut this = Self { io, indent: 0, sort: false, wrap: false, _subs: subs };
        this.recompute(window, cx);
        this
    }

    pub fn set_input(&mut self, text: &str, window: &mut Window, cx: &mut Context<Self>) {
        set_text(&self.io.input, text, window, cx);
        self.recompute(window, cx);
    }

    /// Pick an entry of `INDENTS`.
    pub fn set_indent(&mut self, indent: usize, window: &mut Window, cx: &mut Context<Self>) {
        self.indent = indent;
        self.recompute(window, cx);
    }

    fn recompute(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let indent = match self.indent {
            0 => Indent::Spaces(2),
            1 => Indent::Spaces(4),
            2 => Indent::Tab,
            _ => Indent::Minified,
        };
        let sort = self.sort;
        self.io.run(window, cx, |this| &mut this.io, move |src| match logic::format_json(src, indent, sort) {
            Ok(r) => Out { text: r.out.into(), err: None, status: r.status, tone: Some(Tone::Ok) },
            Err(e) => Out { err: Some(e), status: "Invalid JSON".into(), tone: Some(Tone::Err), ..Default::default() },
        });
    }
}

impl Render for JsonFmtView {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let pal = Pal::get(cx);
        sync_wrap(&[&self.io.input, &self.io.output], &mut self.wrap, window, cx);
        let on_indent = on_index(cx, |this: &mut Self, i, w, cx| this.set_indent(i, w, cx));
        let sort = self.sort;
        div()
            .flex()
            .flex_col()
            .gap(px(6.))
            .flex_1()
            .child(ui::section_label("Configuration", &pal))
            .child(ui::setting(
                "indent",
                ui::setting_icon("indent", &pal),
                "Indentation",
                None,
                ui::dropdown("indent-dd", INDENTS, self.indent, &pal, window, cx, on_indent),
                &pal,
            ))
            .child(ui::setting(
                "sort",
                ui::setting_icon("sort", &pal),
                "Sort JSON properties alphabetically",
                None,
                ui::toggle_labeled("sort-tg", sort, &pal, cx.listener(|this, _, w, cx| {
                    this.sort = !this.sort;
                    this.recompute(w, cx);
                })),
                &pal,
            ))
            .child(panes("jsonfmt", ("Input", "Output"), &self.io, window, cx, Self::recompute))
    }
}

// ------------------------------------------------------------ JSON ↔ YAML / TOML

const INDENT_STEPS: &[&str] = &["2 spaces", "4 spaces"];

#[derive(Clone, Copy, PartialEq)]
enum Other {
    Yaml,
    Toml,
    Csv,
}

impl Other {
    fn name(self) -> &'static str {
        match self {
            Other::Yaml => "YAML",
            Other::Toml => "TOML",
            Other::Csv => "CSV",
        }
    }

    fn language(self) -> &'static str {
        match self {
            Other::Yaml => "yaml",
            Other::Toml => "toml",
            Other::Csv => "plaintext",
        }
    }

    fn directions(self) -> &'static [&'static str] {
        match self {
            Other::Yaml => &["JSON → YAML", "YAML → JSON"],
            Other::Toml => &["JSON → TOML", "TOML → JSON"],
            Other::Csv => &["JSON → CSV", "CSV → JSON"],
        }
    }
}

fn convert(src: &str, other: Other, reverse: bool, step: usize) -> Out {
    let fail = |e: String, status: String| Out { err: Some(e), status, tone: Some(Tone::Err), ..Default::default() };
    let (from, to) = if reverse { (other.name(), "JSON") } else { ("JSON", other.name()) };
    let parsed = match (reverse, other) {
        (false, _) => serde_json::from_str::<serde_json::Value>(src).map_err(|e| logic::json_err(&e)),
        (true, Other::Yaml) => logic::parse_yaml(src),
        (true, Other::Toml) => logic::parse_toml(src),
        (true, Other::Csv) => logic::csv::csv_to_json(src, true),
    };
    let v = match parsed {
        Ok(v) => v,
        Err(e) => return fail(e, format!("Invalid {from}")),
    };
    let text = match (reverse, other) {
        (true, _) => Ok(logic::to_json(&v, &Indent::Spaces(step))),
        (false, Other::Yaml) => Ok(logic::to_yaml(&v, 0, step)),
        (false, Other::Toml) => logic::to_toml(&v),
        (false, Other::Csv) => logic::csv::json_to_csv(&v, ','),
    };
    match text {
        Ok(t) => {
            let status = format!("Converted to {to} · {}", plural(t.split('\n').count(), "line"));
            Out { text: t.into(), err: None, status, tone: Some(Tone::Ok) }
        }
        Err(e) => fail(e, format!("Cannot convert to {to}")),
    }
}

/// JSON to YAML or TOML, and back.
pub struct DataConvView {
    other: Other,
    reverse: bool,
    io: Io,
    indent: usize,
    wrap: bool,
    _subs: Vec<Subscription>,
}

impl DataConvView {
    pub fn new(id: ToolId, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let other = match id {
            ToolId::JsonToml => Other::Toml,
            ToolId::JsonCsv => Other::Csv,
            _ => Other::Yaml,
        };
        let sample = match other {
            Other::Yaml => "{\n  \"service\": \"api-gateway\",\n  \"replicas\": 3,\n  \"ports\": [80, 443],\n  \"env\": { \"LOG_LEVEL\": \"info\", \"CACHE\": true },\n  \"routes\": [\n    { \"path\": \"/users\", \"timeout\": 30 },\n    { \"path\": \"/orders\", \"timeout\": 45 }\n  ]\n}",
            Other::Csv => "[\n  { \"id\": 1, \"name\": \"Ada Lovelace\", \"email\": \"ada@example.com\", \"address\": { \"city\": \"London\" } },\n  { \"id\": 2, \"name\": \"Alan Turing\", \"email\": \"alan@example.com\", \"address\": { \"city\": \"Manchester\" } }\n]",
            Other::Toml => "{\n  \"package\": { \"name\": \"sidekit\", \"version\": \"0.1.0\", \"edition\": \"2024\" },\n  \"dependencies\": {\n    \"serde\": { \"version\": \"1\", \"features\": [\"derive\"] },\n    \"base64\": \"0.22\"\n  }\n}",
        };
        let (io, find_sub) = Io::new(sample, ("json", other.language()), window, cx);
        let subs = vec![
            watch(&io.input, window, cx, Self::recompute),
            watch(&io.output, window, cx, |_, _, _| {}),
            find_sub,
        ];
        let mut this = Self { other, reverse: false, io, indent: 0, wrap: false, _subs: subs };
        this.recompute(window, cx);
        this
    }

    fn recompute(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let (other, reverse) = (self.other, self.reverse);
        let step = if self.indent == 0 { 2 } else { 4 };
        self.io.run(window, cx, |this| &mut this.io, move |src| convert(src, other, reverse, step));
    }

    fn set_reverse(&mut self, reverse: bool, window: &mut Window, cx: &mut Context<Self>) {
        if reverse == self.reverse {
            return;
        }
        // Carry the converted document across, so flipping direction round-trips it.
        if self.io.out.err.is_none() && !self.io.out.text.is_empty() {
            let text = self.io.out.text.to_string();
            set_text(&self.io.input, &text, window, cx);
        }
        self.reverse = reverse;
        let (a, b) = if reverse { (self.other.language(), "json") } else { ("json", self.other.language()) };
        self.io.langs = (a, b);
        self.io.input.update(cx, |s, cx| s.set_highlighter(a, cx));
        self.io.output.update(cx, |s, cx| s.set_highlighter(b, cx));
        self.recompute(window, cx);
    }
}

impl Render for DataConvView {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let pal = Pal::get(cx);
        sync_wrap(&[&self.io.input, &self.io.output], &mut self.wrap, window, cx);
        let on_dir = on_index(cx, |this: &mut Self, i, w, cx| this.set_reverse(i == 1, w, cx));
        let on_indent = on_index(cx, |this: &mut Self, i, w, cx| {
            this.indent = i;
            this.recompute(w, cx);
        });
        let target = if self.reverse { "JSON" } else { self.other.name() };
        // The TOML and CSV writers have one fixed layout.
        let has_indent = self.reverse || self.other == Other::Yaml;
        let (id, titles) = match (self.other, self.reverse) {
            (Other::Yaml, false) => ("jsonyaml", ("JSON", "YAML")),
            (Other::Yaml, true) => ("jsonyaml", ("YAML", "JSON")),
            (Other::Toml, false) => ("jsontoml", ("JSON", "TOML")),
            (Other::Toml, true) => ("jsontoml", ("TOML", "JSON")),
            (Other::Csv, false) => ("jsoncsv", ("JSON", "CSV")),
            (Other::Csv, true) => ("jsoncsv", ("CSV", "JSON")),
        };
        div()
            .flex()
            .flex_col()
            .gap(px(6.))
            .flex_1()
            .child(ui::section_label("Configuration", &pal))
            .child(ui::setting(
                "conv-dir",
                ui::setting_icon("conv", &pal),
                "Conversion",
                None,
                ui::seg("conv-dir-seg", self.other.directions(), self.reverse as usize, &pal, on_dir),
                &pal,
            ))
            .when(has_indent, |d| {
                d.child(ui::setting(
                    "indent",
                    ui::setting_icon("indent", &pal),
                    "Indentation",
                    Some(format!("Spaces used for each nesting level in {target}").into()),
                    ui::dropdown("yindent-dd", INDENT_STEPS, self.indent, &pal, window, cx, on_indent),
                    &pal,
                ))
            })
            .child(panes(id, titles, &self.io, window, cx, Self::recompute))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn identity(src: &str) -> Out {
        Out { text: src.to_string().into(), ..Default::default() }
    }

    #[test]
    fn minified_input_is_relaid_and_long_output_lines_are_shortened() {
        let big = format!("[{}]", vec!["\"é\""; big::LONG_LINE].join(","));
        let done = finish(&big, identity);
        let input = done.input.expect("minified input is re-laid");
        assert!(!has_long_line(&input));
        let shown = done.shown.expect("the one-line output is shortened");
        assert!(shown.len() <= big::LONG_LINE + '…'.len_utf8() && shown.ends_with('…'));
        assert_eq!(done.out.text.len(), big.len());
    }

    #[test]
    fn short_lines_are_left_alone() {
        let done = finish("{\"a\": 1}", identity);
        assert!(done.input.is_none() && done.shown.is_none());
        assert!(finish("  ", identity).out.status.starts_with("Waiting"));
    }
}
