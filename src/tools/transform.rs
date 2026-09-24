//! Input → output tools with one choice of mode: SQL, XML and IP ranges.

use gpui_kit::component::input::EditorState;
use gpui_kit::{
    Context, Entity, IntoElement, ParentElement, Render, SharedString, Styled, Subscription, Task,
    Window, div, prelude::FluentBuilder, px,
};

use super::*;
use super::big;
use crate::logic::{self, plural};
use crate::registry::ToolId;
use crate::ui::{self, Tone};

/// A finished run: the output and a short status line.
type Run = Result<(String, String), String>;

struct Spec {
    label: &'static str,
    modes: &'static [&'static str],
    hint: &'static str,
    sample: &'static str,
    languages: (&'static str, &'static str),
    run: fn(&str, usize) -> Run,
}

fn spec(id: ToolId) -> Spec {
    match id {
        ToolId::Sql => Spec {
            label: "Style",
            modes: logic::SQL_MODES,
            hint: "Indent clauses, or put the whole query on one line",
            sample: "select u.id, u.name, count(o.id) as orders from users u left join orders o on o.user_id = u.id where u.active = true and o.created_at > '2026-01-01' group by u.id, u.name having count(o.id) > 3 order by orders desc limit 20;",
            languages: ("sql", "sql"),
            run: |s, mode| {
                let out = logic::format_sql(s, mode, 2);
                let n = out.lines().count();
                Ok((out, format!("Formatted · {}", plural(n, "line"))))
            },
        },
        ToolId::Xml => Spec {
            label: "Indentation",
            modes: &["2 spaces", "4 spaces", "Minified"],
            hint: "Checks the document is well-formed while formatting it",
            sample: "<?xml version=\"1.0\" encoding=\"UTF-8\"?><catalog><book id=\"bk101\"><author>Gambardella, Matthew</author><title>XML Developer's Guide</title><price currency=\"USD\">44.95</price></book><book id=\"bk102\"><author>Ralls, Kim</author><title>Midnight Rain</title><price currency=\"USD\">5.95</price></book></catalog>",
            languages: ("html", "html"),
            run: |s, mode| {
                let indent = [Some(2), Some(4), None][mode.min(2)];
                let out = logic::xml::format_xml(s, indent)?;
                Ok((out, "Well-formed XML".into()))
            },
        },
        _ => Spec {
            label: "Mode",
            modes: &["Expand", "Summarize"],
            hint: "One address, CIDR block or range (a - b) per line",
            sample: "10.0.0.0/30\n10.0.0.4\n10.0.0.5\n10.0.0.6 - 10.0.0.7\n192.168.10.0/24",
            languages: ("plaintext", "plaintext"),
            run: |s, mode| {
                if mode == 0 {
                    let (out, total, cut) = logic::iprange::expand(s)?;
                    let shown = out.lines().count();
                    let status = if cut {
                        format!("{} addresses · showing the first {}", logic::group(&total.to_string(), 3, ","), logic::thousands(shown as u64))
                    } else {
                        plural(shown, "address")
                    };
                    Ok((out, status))
                } else {
                    let (out, n) = logic::iprange::summarize(s)?;
                    Ok((out, format!("{} {}", n, if n == 1 { "block" } else { "blocks" })))
                }
            },
        },
    }
}

pub struct TransformView {
    spec: Spec,
    input: Entity<EditorState>,
    output: Entity<EditorState>,
    mode: usize,
    out: SharedString,
    status: String,
    err: Option<String>,
    task: Option<Task<()>>,
    _subs: Vec<Subscription>,
}

impl TransformView {
    pub fn new(id: ToolId, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let spec = spec(id);
        let input = code_editor(spec.sample, "Type or paste here", spec.languages.0, window, cx);
        let output = code_editor("", "", spec.languages.1, window, cx);
        // One-line SQL and minified XML are the usual input: wrap it.
        input.update(cx, |s, cx| s.set_soft_wrap(true, window, cx));
        let subs = vec![watch(&input, window, cx, Self::recompute), watch(&output, window, cx, |_, _, _| {})];
        let mut this = Self { spec, input, output, mode: 0, out: SharedString::default(), status: String::new(), err: None, task: None, _subs: subs };
        this.recompute(window, cx);
        this
    }

    fn recompute(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let src = text_of(&self.input, cx);
        big::fit_language(&self.input, self.spec.languages.0, src.len(), cx);
        let (run, mode) = (self.spec.run, self.mode);
        let work = move || {
            if src.trim().is_empty() {
                return (String::new(), "Waiting for input".to_string(), None);
            }
            match run(&src, mode) {
                Ok((o, s)) => (o, s, None),
                Err(e) => (String::new(), "Error".to_string(), Some(e)),
            }
        };
        let size = text_len(&self.input, cx);
        if size > big::LARGE {
            self.status = "Working…".into();
            cx.notify();
        }
        big::run(size, self, |t| &mut t.task, window, cx, work, |this, (out, mut status, err), window, cx| {
            let shown = big::for_display(&out);
            if shown.is_some() {
                status = format!("{status} · {}", big::SHORTENED);
            }
            big::fit_language(&this.output, this.spec.languages.1, out.len(), cx);
            set_text(&this.output, shown.as_deref().unwrap_or(&out), window, cx);
            this.out = out.into();
            this.status = status;
            this.err = err;
            cx.notify();
        });
    }
}

impl Render for TransformView {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let pal = Pal::get(cx);
        let on_mode = on_index(cx, |this: &mut Self, i, w, cx| {
            this.mode = i;
            this.recompute(w, cx);
        });
        let [paste, clear] = paste_clear("xf", &self.input, &pal, cx, Self::recompute);
        let zoom = ui::PaneZoom::new("xf-output", window, cx);
        let tone = if self.err.is_some() { Some(Tone::Err) } else if self.out.is_empty() { None } else { Some(Tone::Ok) };

        div()
            .flex()
            .flex_col()
            .gap(px(6.))
            .flex_1()
            .child(ui::section_label("Configuration", &pal))
            .child(ui::setting(
                "xf-mode",
                ui::setting_icon("conv", &pal),
                self.spec.label,
                Some(self.spec.hint.into()),
                ui::seg("xf-seg", self.spec.modes, self.mode, &pal, on_mode),
                &pal,
            ))
            .child(
                div()
                    .flex()
                    .gap(px(12.))
                    .flex_1()
                    .min_h(px(340.))
                    .mt(px(8.))
                    .child(
                        ui::pane(is_focused(&self.input, window, cx), &pal)
                            .flex_1()
                            .child(ui::pane_head("Input", None, &pal).child(paste).child(clear))
                            .child(code_editor_el(&self.input, false, cx)),
                    )
                    .child(zoom.wrap(
                        ui::pane(is_focused(&self.output, window, cx), &pal)
                            .flex_1()
                            .child(
                                ui::pane_head("Output", None, &pal)
                                    .child(ui::copy_btn("xf-copy", self.out.clone(), &pal, window, cx))
                                    .child(zoom.button(&pal)),
                            )
                            .when_some(self.err.clone(), |d, e| d.child(ui::err_box(e, &pal).m(px(12.))))
                            .child(code_editor_el(&self.output, true, cx))
                            .child(ui::pane_foot(&pal).child(ui::dot(tone, &pal)).child(self.status.clone())),
                        &pal,
                        window,
                    )),
            )
    }
}
