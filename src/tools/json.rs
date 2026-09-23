use gpui_kit::component::input::{Input, InputEvent, InputState, EditorState};
use gpui_kit::{
    App, AppContext, Context, Entity, InteractiveElement, IntoElement, ParentElement, Render,
    SharedString, Styled, Subscription, Window, div, prelude::FluentBuilder, px,
};

use super::*;
use crate::logic::{self, Indent, plural};
use crate::ui::{self, Tone};

/// Output state shared by the two JSON tools.
#[derive(Default)]
struct Out {
    text: SharedString,
    err: Option<String>,
    status: String,
    tone: Option<Tone>,
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
    input: &Entity<EditorState>,
    output: &Entity<EditorState>,
    finder: &Finder,
    out: &Out,
    window: &mut Window,
    cx: &mut Context<V>,
    recompute: impl Fn(&mut V, &mut Window, &mut Context<V>) + Clone + 'static,
) -> gpui_kit::AnyElement {
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

const INDENTS: &[&str] = &["2 spaces", "4 spaces", "1 tab", "Minified"];

pub struct JsonFmtView {
    input: Entity<EditorState>,
    output: Entity<EditorState>,
    indent: usize,
    sort: bool,
    out: Out,
    finder: Finder,
    wrap: bool,
    _subs: Vec<Subscription>,
}

impl JsonFmtView {
    pub fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let input = code_editor(
            r#"{"name":"SideKit","version":2,"offline":true,"tags":["developer","utilities","open-source"],"platforms":{"windows":true,"macos":true,"linux":true}}"#,
            "Paste or type JSON",
            "json",
            window,
            cx,
        );
        let output = code_editor("", "", "json", window, cx);
        let (finder, find_sub) = Finder::new(&output, window, cx);
        let subs = vec![
            watch(&input, window, cx, Self::recompute),
            watch(&output, window, cx, |_, _, _| {}),
            find_sub,
        ];
        let mut this = Self { input, output, indent: 0, sort: false, out: Out::default(), finder, wrap: false, _subs: subs };
        this.recompute(window, cx);
        this
    }

    pub fn set_input(&mut self, text: &str, window: &mut Window, cx: &mut Context<Self>) {
        set_text(&self.input, text, window, cx);
        self.recompute(window, cx);
    }

    fn recompute(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let src = text_of(&self.input, cx);
        self.out = if src.trim().is_empty() {
            Out { status: "Waiting for input".into(), ..Default::default() }
        } else {
            let indent = match self.indent {
                0 => Indent::Spaces(2),
                1 => Indent::Spaces(4),
                2 => Indent::Tab,
                _ => Indent::Minified,
            };
            match logic::format_json(&src, indent, self.sort) {
                Ok(r) => Out { text: r.out.into(), err: None, status: r.status, tone: Some(Tone::Ok) },
                Err(e) => Out { err: Some(e), status: "Invalid JSON".into(), tone: Some(Tone::Err), ..Default::default() },
            }
        };
        set_text(&self.output, &self.out.text, window, cx);
        self.finder.refresh(cx);
        cx.notify();
    }
}

impl Render for JsonFmtView {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let pal = Pal::get(cx);
        sync_wrap(&[&self.input, &self.output], &mut self.wrap, window, cx);
        let on_indent = on_index(cx, |this: &mut Self, i, w, cx| {
            this.indent = i;
            this.recompute(w, cx);
        });
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
            .child(panes("jsonfmt", ("Input", "Output"), &self.input, &self.output, &self.finder, &self.out, window, cx, Self::recompute))
    }
}

// ------------------------------------------------------------ JSON → YAML

const YAML_INDENTS: &[&str] = &["2 spaces", "4 spaces"];

pub struct JsonYamlView {
    input: Entity<EditorState>,
    output: Entity<EditorState>,
    indent: usize,
    out: Out,
    finder: Finder,
    wrap: bool,
    _subs: Vec<Subscription>,
}

impl JsonYamlView {
    pub fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let input = code_editor(
            "{\n  \"service\": \"api-gateway\",\n  \"replicas\": 3,\n  \"ports\": [80, 443],\n  \"env\": { \"LOG_LEVEL\": \"info\", \"CACHE\": true },\n  \"routes\": [\n    { \"path\": \"/users\", \"timeout\": 30 },\n    { \"path\": \"/orders\", \"timeout\": 45 }\n  ]\n}",
            "Paste or type JSON",
            "json",
            window,
            cx,
        );
        let output = code_editor("", "", "yaml", window, cx);
        let (finder, find_sub) = Finder::new(&output, window, cx);
        let subs = vec![
            watch(&input, window, cx, Self::recompute),
            watch(&output, window, cx, |_, _, _| {}),
            find_sub,
        ];
        let mut this = Self { input, output, indent: 0, out: Out::default(), finder, wrap: false, _subs: subs };
        this.recompute(window, cx);
        this
    }

    fn recompute(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let src = text_of(&self.input, cx);
        self.out = if src.trim().is_empty() {
            Out { status: "Waiting for input".into(), ..Default::default() }
        } else {
            match serde_json::from_str::<serde_json::Value>(&src) {
                Ok(v) => {
                    let y = logic::to_yaml(&v, 0, if self.indent == 0 { 2 } else { 4 });
                    let status = format!("Converted · {}", plural(y.split('\n').count(), "line"));
                    Out { text: y.into(), err: None, status, tone: Some(Tone::Ok) }
                }
                Err(e) => Out {
                    err: Some(logic::json_err(&e)),
                    status: "Invalid JSON".into(),
                    tone: Some(Tone::Err),
                    ..Default::default()
                },
            }
        };
        set_text(&self.output, &self.out.text, window, cx);
        self.finder.refresh(cx);
        cx.notify();
    }
}

impl Render for JsonYamlView {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let pal = Pal::get(cx);
        sync_wrap(&[&self.input, &self.output], &mut self.wrap, window, cx);
        let on_indent = on_index(cx, |this: &mut Self, i, w, cx| {
            this.indent = i;
            this.recompute(w, cx);
        });
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
                Some("Spaces used for each nesting level in YAML".into()),
                ui::dropdown("yindent-dd", YAML_INDENTS, self.indent, &pal, window, cx, on_indent),
                &pal,
            ))
            .child(panes("jsonyaml", ("JSON", "YAML"), &self.input, &self.output, &self.finder, &self.out, window, cx, Self::recompute))
    }
}
