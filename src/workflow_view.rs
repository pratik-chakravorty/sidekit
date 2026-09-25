//! The Workflows page: an input, a list of steps that each feed the next, and
//! the final output. Saved workflows save themselves on every change; a new
//! one is kept only once the user presses Save.

use std::path::PathBuf;

use gpui_kit::component::input::{EditorState, InputState};
use gpui_kit::component::scroll::ScrollableElement;
use gpui_kit::{
    AnyElement, Context, Entity, FontWeight, InteractiveElement, IntoElement, ParentElement,
    Render, SharedString, StatefulInteractiveElement, Styled, Subscription, Task, Window, div,
    prelude::FluentBuilder, px,
};

use crate::icons::icon;
use crate::logic::plural;
use crate::logic::workflow::{self, Arg, LABELS, OPS, Outcome, Step, Workflow};
use crate::theme::{MONO_FONT, Pal};
use crate::tools::{self, big, code_editor, code_editor_el, field_el, is_focused, line, line_text, paste_clear, set_line, set_text, text_len, text_of, watch};
use crate::ui::{self, BtnKind, MenuEntry, Tone};
use crate::workflows::{self, Saved};

/// What a first-time visitor sees: the example from the feature pitch.
fn starter() -> (Workflow, &'static str) {
    let s = |op: &str, arg: &str| Step { op: op.into(), arg: arg.into(), on: true };
    let flow = Workflow {
        name: "Decode log token".into(),
        steps: vec![s("param", "token"), s("urldec", ""), s("b64dec", ""), s("jsonfmt", "2 spaces")],
    };
    (flow, "token=eyJ1c2VyIjoicHJhdGlrIiwicm9sZSI6ImFkbWluIn0%3D")
}

pub struct WorkflowView {
    saved: Vec<Saved>,
    /// The file of the workflow on show; `None` until a new one is saved.
    file: Option<PathBuf>,
    name: Entity<InputState>,
    steps: Vec<Step>,
    /// One per step; only shown for steps that take free text.
    args: Vec<Entity<InputState>>,
    input: Entity<EditorState>,
    output: Entity<EditorState>,
    outcomes: Vec<Outcome>,
    out: SharedString,
    shortened: bool,
    notice: Option<(SharedString, Tone)>,
    task: Option<Task<()>>,
    /// Set while the view writes into its own inputs, so that is not an edit.
    loading: bool,
    arg_subs: Vec<Subscription>,
    _subs: Vec<Subscription>,
}

impl WorkflowView {
    pub fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let saved = workflows::load_all();
        let name = line("", "Workflow name", window, cx);
        let input = code_editor("", "Paste the text to run the workflow on", "plaintext", window, cx);
        let output = code_editor("", "", "plaintext", window, cx);
        for s in [&input, &output] {
            s.update(cx, |s, cx| s.set_soft_wrap(true, window, cx));
        }
        let subs = vec![
            watch(&input, window, cx, Self::recompute),
            watch(&output, window, cx, |_, _, _| {}),
            watch(&name, window, cx, Self::name_changed),
        ];
        let mut this = Self {
            saved,
            file: None,
            name,
            steps: Vec::new(),
            args: Vec::new(),
            input,
            output,
            outcomes: Vec::new(),
            out: SharedString::default(),
            shortened: false,
            notice: None,
            task: None,
            loading: false,
            arg_subs: Vec::new(),
            _subs: subs,
        };
        if this.saved.is_empty() {
            let (flow, sample) = starter();
            this.show(flow, None, window, cx);
            set_text(&this.input, sample, window, cx);
            this.recompute(window, cx);
        } else {
            this.open(0, window, cx);
        }
        this
    }

    /// Saved workflow names, in the order `open` and the palette use.
    pub fn names() -> Vec<String> {
        workflows::load_all().into_iter().map(|s| s.flow.name).collect()
    }

    /// Show saved workflow `i` (re-reading the folder first).
    pub fn open(&mut self, i: usize, window: &mut Window, cx: &mut Context<Self>) {
        self.saved = workflows::load_all();
        if let Some(s) = self.saved.get(i) {
            let (flow, file) = (s.flow.clone(), Some(s.file.clone()));
            self.show(flow, file, window, cx);
        }
    }

    /// Run the workflow on show against `text` (the palette's "run on clipboard").
    pub fn set_input(&mut self, text: &str, window: &mut Window, cx: &mut Context<Self>) {
        set_text(&self.input, text, window, cx);
        self.recompute(window, cx);
    }

    fn show(&mut self, flow: Workflow, file: Option<PathBuf>, window: &mut Window, cx: &mut Context<Self>) {
        self.loading = true;
        set_line(&self.name, &flow.name, window, cx);
        self.loading = false;
        self.steps = flow.steps;
        self.file = file;
        self.notice = None;
        self.rebuild_args(window, cx);
        self.recompute(window, cx);
    }

    fn new_flow(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let flow = Workflow { name: "Untitled workflow".into(), steps: Vec::new() };
        self.show(flow, None, window, cx);
        self.name.update(cx, |s, cx| s.focus(window, cx));
    }

    fn rebuild_args(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.args.clear();
        self.arg_subs.clear();
        for (i, step) in self.steps.iter().enumerate() {
            let placeholder = match workflow::op(&step.op).map(|(_, o)| &o.arg) {
                Some(Arg::Text(p, _)) => *p,
                _ => "",
            };
            let input = line(&step.arg, placeholder, window, cx);
            self.arg_subs.push(watch(&input, window, cx, move |this, w, cx| {
                let v = line_text(&this.args[i], cx);
                if let Some(s) = this.steps.get_mut(i) {
                    s.arg = v;
                }
                this.changed(w, cx);
            }));
            self.args.push(input);
        }
    }

    fn flow(&self, cx: &Context<Self>) -> Workflow {
        Workflow { name: line_text(&self.name, cx).trim().to_string(), steps: self.steps.clone() }
    }

    // ------------------------------------------------------------ edits

    fn name_changed(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let _ = window;
        if !self.loading {
            self.persist(cx);
            cx.notify();
        }
    }

    /// Steps changed: run again, and save if this workflow lives on disk.
    fn changed(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.persist(cx);
        self.recompute(window, cx);
    }

    fn structure_changed(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.rebuild_args(window, cx);
        self.changed(window, cx);
    }

    fn add(&mut self, op: usize, window: &mut Window, cx: &mut Context<Self>) {
        self.steps.push(Step::new(&OPS[op]));
        self.structure_changed(window, cx);
    }

    fn remove(&mut self, i: usize, window: &mut Window, cx: &mut Context<Self>) {
        self.steps.remove(i);
        self.structure_changed(window, cx);
    }

    fn shift(&mut self, i: usize, down: bool, window: &mut Window, cx: &mut Context<Self>) {
        let j = if down { i + 1 } else { i.wrapping_sub(1) };
        if j < self.steps.len() {
            self.steps.swap(i, j);
            self.structure_changed(window, cx);
        }
    }

    fn toggle(&mut self, i: usize, window: &mut Window, cx: &mut Context<Self>) {
        self.steps[i].on = !self.steps[i].on;
        self.changed(window, cx);
    }

    fn set_op(&mut self, i: usize, op: usize, window: &mut Window, cx: &mut Context<Self>) {
        if self.steps[i].op != OPS[op].key {
            self.steps[i] = Step { on: self.steps[i].on, ..Step::new(&OPS[op]) };
            self.structure_changed(window, cx);
        }
    }

    fn set_choice(&mut self, i: usize, value: &str, window: &mut Window, cx: &mut Context<Self>) {
        self.steps[i].arg = value.into();
        self.changed(window, cx);
    }

    // ------------------------------------------------------------ files

    /// Autosave a workflow that already has a file.
    fn persist(&mut self, cx: &mut Context<Self>) {
        if self.file.is_some() {
            self.write(false, cx);
        }
    }

    /// Save to disk; `first` for the explicit Save of a new workflow.
    fn write(&mut self, first: bool, cx: &mut Context<Self>) {
        let flow = self.flow(cx);
        if flow.name.is_empty() {
            self.notice = Some(("Give the workflow a name to save it".into(), Tone::Err));
            return;
        }
        if workflows::taken(&flow.name, self.file.as_ref()) {
            self.notice = Some((format!("Another workflow is already called \"{}\"", flow.name).into(), Tone::Err));
            return;
        }
        match workflows::save(&flow, self.file.as_ref()) {
            Ok(file) => {
                self.file = Some(file);
                self.saved = workflows::load_all();
                self.notice = first.then(|| ("Saved · changes now save automatically".into(), Tone::Ok));
            }
            Err(e) => self.notice = Some((format!("Could not save: {e}").into(), Tone::Err)),
        }
    }

    fn delete(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(file) = self.file.take() else { return };
        if let Err(e) = workflows::delete(&file) {
            self.file = Some(file);
            self.notice = Some((format!("Could not delete: {e}").into(), Tone::Err));
            cx.notify();
            return;
        }
        self.saved = workflows::load_all();
        if self.saved.is_empty() {
            self.new_flow(window, cx);
        } else {
            self.open(0, window, cx);
        }
    }

    // ------------------------------------------------------------ running

    fn recompute(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let input = text_of(&self.input, cx);
        let steps = self.steps.clone();
        let size = text_len(&self.input, cx);
        big::run(size, self, |t| &mut t.task, window, cx, move || workflow::run(&input, &steps), |this, (outcomes, out), window, cx| {
            let out = out.unwrap_or_default();
            let shown = big::for_display(&out);
            this.shortened = shown.is_some();
            set_text(&this.output, shown.as_deref().unwrap_or(&out), window, cx);
            this.out = out.into();
            this.outcomes = outcomes;
            cx.notify();
        });
    }

    // ------------------------------------------------------------ render

    fn render_step(&self, i: usize, pal: &Pal, window: &mut Window, cx: &mut Context<Self>) -> AnyElement {
        let p = *pal;
        let step = &self.steps[i];
        let found = workflow::op(&step.op);
        let last = i + 1 == self.steps.len();
        let op_index = found.map_or(0, |(k, _)| k);
        let on_op = tools::on_index(cx, move |this: &mut Self, k, w, cx| this.set_op(i, k, w, cx));

        let arg: Option<AnyElement> = match found.map(|(_, o)| &o.arg) {
            Some(Arg::Text(..)) => Some(field_el(&self.args[i], true, 32., 13., window, cx).into_any_element()),
            Some(Arg::Choice(choices)) => {
                let choices: &'static [&'static str] = choices;
                let sel = choices.iter().position(|c| *c == step.arg).unwrap_or(0);
                let on_pick = tools::on_index(cx, move |this: &mut Self, k, w, cx| this.set_choice(i, choices[k], w, cx));
                Some(ui::dropdown(("wf-choice", i), choices, sel, pal, window, cx, on_pick).into_any_element())
            }
            _ => None,
        };

        let (tone, text): (Option<Tone>, SharedString) = match self.outcomes.get(i) {
            Some(Outcome::Ok(s)) if s.is_empty() => (Some(Tone::Ok), "(empty)".into()),
            Some(Outcome::Ok(s)) => (Some(Tone::Ok), big::preview(s.lines().next().unwrap_or(""), 90).into()),
            Some(Outcome::Err(e)) => (Some(Tone::Err), e.clone().into()),
            Some(Outcome::Off) => (None, "Off · passes its input through".into()),
            Some(Outcome::Skipped) => (None, "Not run · an earlier step failed".into()),
            None => (None, "".into()),
        };
        let err = tone == Some(Tone::Err);
        let on = step.on;

        div()
            .id(("wf-step", i))
            .flex()
            .flex_col()
            .gap(px(8.))
            .p(px(10.))
            .bg(p.card)
            .border_1()
            .border_color(if err { p.danger } else { p.stroke })
            .rounded(px(8.))
            .when(!step.on, |d| d.opacity(0.6))
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(6.))
                    .child(
                        div()
                            .size(px(22.))
                            .flex_none()
                            .flex()
                            .items_center()
                            .justify_center()
                            .rounded(px(11.))
                            .bg(p.subtle2)
                            .text_size(px(11.))
                            .font_weight(FontWeight::SEMIBOLD)
                            .text_color(p.text2)
                            .child((i + 1).to_string()),
                    )
                    .child(div().flex_1().min_w_0().child(ui::dropdown(("wf-op", i), LABELS.as_slice(), op_index, pal, window, cx, on_op)))
                    .child(
                        ui::icon_btn(("wf-on", i), "eye", pal, cx.listener(move |this, _, w, cx| this.toggle(i, w, cx)))
                            .tooltip(move |w, cx| gpui_kit::component::tooltip::Tooltip::new(if on { "Turn step off" } else { "Turn step on" }).build(w, cx)),
                    )
                    .when(i > 0, |d| d.child(ui::icon_btn(("wf-up", i), "chev-up", pal, cx.listener(move |this, _, w, cx| this.shift(i, false, w, cx)))))
                    .when(!last, |d| d.child(ui::icon_btn(("wf-down", i), "chev-down", pal, cx.listener(move |this, _, w, cx| this.shift(i, true, w, cx)))))
                    .child(ui::icon_btn(("wf-del", i), "trash", pal, cx.listener(move |this, _, w, cx| this.remove(i, w, cx)))),
            )
            .when_some(arg, |d, a| d.child(a))
            .when(!text.is_empty(), |d| {
                d.child(
                    div()
                        .flex()
                        .items_center()
                        .gap(px(8.))
                        .child(ui::dot(tone, pal))
                        .child(
                            div()
                                .flex_1()
                                .min_w_0()
                                .text_size(px(12.))
                                .font_family(MONO_FONT)
                                .text_color(if err { p.danger } else { p.text3 })
                                .when(!err, |d| d.whitespace_nowrap().overflow_hidden().text_ellipsis())
                                .child(text),
                        ),
                )
            })
            .into_any_element()
    }
}

impl Render for WorkflowView {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let pal = Pal::get(cx);
        let p = pal;
        let saved_now = self.file.is_some();

        // Open menu: every saved workflow, then "New workflow".
        let mut open_entries: Vec<MenuEntry> = self.saved.iter().map(|s| MenuEntry::new(s.flow.name.clone(), Some("flow"))).collect();
        open_entries.push(MenuEntry::new("New workflow", Some("plus")));
        let n_saved = self.saved.len();
        let on_open = tools::on_index(cx, move |this: &mut Self, i, w, cx| {
            if i < n_saved {
                this.open(i, w, cx);
            } else {
                this.new_flow(w, cx);
            }
        });
        let add_entries: Vec<MenuEntry> = OPS.iter().map(|o| MenuEntry::new(o.label, None)).collect();
        let on_add = tools::on_index(cx, |this: &mut Self, i, w, cx| this.add(i, w, cx));

        let [paste, clear] = paste_clear("wf-in", &self.input, &pal, cx, Self::recompute);
        let zoom = ui::PaneZoom::new("wf-output", window, cx);
        let failed = self.outcomes.iter().position(|o| matches!(o, Outcome::Err(_)));
        let (tone, status): (Option<Tone>, String) = match failed {
            Some(k) => (Some(Tone::Err), format!("Step {} failed", k + 1)),
            None if self.steps.is_empty() => (None, "Add a step to start".into()),
            None if self.shortened => (Some(Tone::Ok), format!("{} · {}", plural(self.out.len(), "byte"), big::SHORTENED)),
            None => (Some(Tone::Ok), format!("{} · {}", plural(self.out.encode_utf16().count(), "character"), plural(self.out.len(), "byte"))),
        };

        let steps: Vec<AnyElement> = (0..self.steps.len()).map(|i| self.render_step(i, &pal, window, cx)).collect();

        div()
            .flex_1()
            .min_h_0()
            .flex()
            .flex_col()
            // Header
            .child(
                div()
                    .flex()
                    .items_end()
                    .gap(px(14.))
                    .pt(px(26.))
                    .px(px(36.))
                    .pb(px(14.))
                    .child(
                        div()
                            .flex()
                            .flex_col()
                            .gap(px(4.))
                            .flex_1()
                            .min_w_0()
                            .child(div().text_size(px(26.)).font_weight(FontWeight::SEMIBOLD).child("Workflows"))
                            .child(div().text_size(px(13.)).text_color(p.text2).child("Chain tools together: each step's output is the next step's input.")),
                    )
                    .child(ui::menu_btn("wf-open", Some("flow"), Some("Open".into()), BtnKind::Normal, open_entries, &pal, window, cx, on_open)),
            )
            // Name bar
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(8.))
                    .px(px(36.))
                    .pb(px(12.))
                    .child(div().flex_1().min_w_0().child(field_el(&self.name, false, 36., 14., window, cx)))
                    .when_some(self.notice.clone(), |d, (text, tone)| {
                        d.child(div().flex_none().text_size(px(12.)).text_color(if tone == Tone::Err { p.danger } else { p.ok }).child(text))
                    })
                    .when(!saved_now, |d| {
                        d.child(ui::btn("wf-save", Some("check"), "Save", BtnKind::Accent, &pal, cx.listener(|this, _, _, cx| {
                            this.write(true, cx);
                            cx.notify();
                        })))
                    })
                    .when(saved_now, |d| {
                        d.child(
                            ui::icon_btn("wf-delete", "trash", &pal, cx.listener(|this, _, w, cx| this.delete(w, cx)))
                                .tooltip(|w, cx| gpui_kit::component::tooltip::Tooltip::new("Delete this workflow").build(w, cx)),
                        )
                    }),
            )
            // Body: input and output on the left, the steps on the right.
            .child(
                div()
                    .flex()
                    .gap(px(16.))
                    .flex_1()
                    .min_h_0()
                    .px(px(36.))
                    .pb(px(24.))
                    .child(
                        div()
                            .flex()
                            .flex_col()
                            .gap(px(12.))
                            .flex_1()
                            .min_w_0()
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
                                            .child(ui::copy_btn("wf-copy", self.out.clone(), &pal, window, cx))
                                            .child(zoom.button(&pal)),
                                    )
                                    .child(code_editor_el(&self.output, true, cx))
                                    .child(ui::pane_foot(&pal).child(ui::dot(tone, &pal)).child(status)),
                                &pal,
                                window,
                            )),
                    )
                    .child(
                        div()
                            .w(px(380.))
                            .flex_none()
                            .flex()
                            .flex_col()
                            .gap(px(8.))
                            .min_h_0()
                            .child(
                                div()
                                    .flex()
                                    .items_center()
                                    .gap(px(8.))
                                    .child(ui::section_label(format!("Steps · {}", self.steps.len()), &pal).flex_1())
                                    .child(ui::menu_btn("wf-add", Some("plus"), Some("Add step".into()), BtnKind::Normal, add_entries, &pal, window, cx, on_add)),
                            )
                            .child(
                                div()
                                    .id("wf-steps")
                                    .flex_1()
                                    .min_h_0()
                                    .overflow_y_scrollbar()
                                    .child(
                                        div()
                                            .flex()
                                            .flex_col()
                                            .gap(px(8.))
                                            .pr(px(4.))
                                            .children(steps)
                                            .when(self.steps.is_empty(), |d| {
                                                d.child(
                                                    div()
                                                        .p(px(16.))
                                                        .rounded(px(8.))
                                                        .border_1()
                                                        .border_color(p.stroke)
                                                        .text_size(px(13.))
                                                        .text_color(p.text3)
                                                        .flex()
                                                        .items_center()
                                                        .gap(px(8.))
                                                        .child(icon("flow", 16., p.text3))
                                                        .child("No steps yet. Use Add step to pick the first tool."),
                                                )
                                            }),
                                    ),
                            ),
                    ),
            )
    }
}
