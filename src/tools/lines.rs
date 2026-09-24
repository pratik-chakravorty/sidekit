//! Sort, deduplicate and tidy a list of lines.

use gpui_kit::component::input::EditorState;
use gpui_kit::{
    Context, Entity, IntoElement, ParentElement, Render, SharedString, Styled, Subscription, Task,
    Window, div, prelude::FluentBuilder, px,
};

use super::*;
use super::big;
use crate::logic::{self, LineOpts, LineSort, plural, random_u32};
use crate::ui::{self, Tone};

const SORTS: &[&str] = &["Keep order", "A → Z", "Z → A", "Natural", "By length", "Reverse", "Shuffle"];
const SORT_KINDS: [LineSort; 7] =
    [LineSort::Keep, LineSort::Asc, LineSort::Desc, LineSort::Natural, LineSort::Length, LineSort::Reverse, LineSort::Shuffle];

pub struct LinesView {
    input: Entity<EditorState>,
    output: Entity<EditorState>,
    sort: usize,
    dedupe: bool,
    ignore_case: bool,
    trim: bool,
    drop_empty: bool,
    seed: u32,
    out: SharedString,
    status: String,
    task: Option<Task<()>>,
    _subs: Vec<Subscription>,
}

impl LinesView {
    pub fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let sample = "banana\nApple\nfile10.txt\ncherry\n\nfile2.txt\napple\n  banana  ";
        let input = code_editor(sample, "One item per line", "plaintext", window, cx);
        let output = code_editor("", "", "plaintext", window, cx);
        let subs = vec![
            watch(&input, window, cx, Self::recompute),
            watch(&output, window, cx, |_, _, _| {}),
        ];
        let mut this = Self {
            input,
            output,
            sort: 1,
            dedupe: true,
            ignore_case: false,
            trim: true,
            drop_empty: true,
            seed: random_u32(),
            out: SharedString::default(),
            status: String::new(),
            task: None,
            _subs: subs,
        };
        this.recompute(window, cx);
        this
    }

    fn recompute(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let input = text_of(&self.input, cx);
        let opts = LineOpts {
            sort: SORT_KINDS[self.sort],
            dedupe: self.dedupe,
            ignore_case: self.ignore_case,
            trim: self.trim,
            drop_empty: self.drop_empty,
            seed: self.seed,
        };
        let size = input.len();
        big::run(size, self, |t| &mut t.task, window, cx, move || logic::process_lines(&input, &opts), |this, r, window, cx| {
            let mut status = plural(r.lines, "line");
            if r.dupes > 0 {
                status.push_str(&format!(" · {} removed", plural(r.dupes, "duplicate")));
            }
            if r.empties > 0 {
                status.push_str(&format!(" · {} removed", plural(r.empties, "empty line")));
            }
            let shown = big::for_display(&r.text);
            if shown.is_some() {
                status.push_str(&format!(" · {}", big::SHORTENED));
            }
            set_text(&this.output, shown.as_deref().unwrap_or(&r.text), window, cx);
            this.status = status;
            this.out = r.text.into();
            cx.notify();
        });
    }

    fn toggle(&mut self, f: fn(&mut Self) -> &mut bool, window: &mut Window, cx: &mut Context<Self>) {
        let v = f(self);
        *v = !*v;
        self.recompute(window, cx);
    }

    /// "Use output as input", e.g. to apply a second sort on top of a cleanup.
    fn swap(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let out = self.out.to_string();
        set_text(&self.input, &out, window, cx);
        self.recompute(window, cx);
    }
}

impl Render for LinesView {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let pal = Pal::get(cx);
        let on_sort = on_index(cx, |this: &mut Self, i, w, cx| {
            this.sort = i;
            this.recompute(w, cx);
        });
        let toggle_row = |id: &'static str, icon: &str, title: &'static str, desc: &'static str, on: bool, f: fn(&mut Self) -> &mut bool, cx: &mut Context<Self>| {
            ui::setting(
                id,
                ui::setting_icon(icon, &pal),
                title,
                Some(desc.into()),
                ui::toggle_labeled(crate::id!("{id}-tg"), on, &pal, cx.listener(move |this, _, w, cx| this.toggle(f, w, cx))),
                &pal,
            )
        };
        let shuffling = SORT_KINDS[self.sort] == LineSort::Shuffle;
        let [paste, clear] = paste_clear("lines", &self.input, &pal, cx, Self::recompute);
        let zoom = ui::PaneZoom::new("lines-output", window, cx);
        let tone = if self.out.is_empty() { None } else { Some(Tone::Ok) };

        div()
            .flex()
            .flex_col()
            .gap(px(6.))
            .flex_1()
            .child(ui::section_label("Configuration", &pal))
            .child(ui::setting(
                "lines-sort",
                ui::setting_icon("sort", &pal),
                "Order",
                Some("How to arrange the lines that remain".into()),
                ui::dropdown("lines-sort-dd", SORTS, self.sort, &pal, window, cx, on_sort),
                &pal,
            ))
            .child(toggle_row("lines-dedupe", "filter", "Remove duplicates", "Keep the first occurrence of each line", self.dedupe, |t| &mut t.dedupe, cx))
            .child(toggle_row("lines-case", "upper", "Ignore case", "Treat \"Apple\" and \"apple\" as the same when sorting and deduplicating", self.ignore_case, |t| &mut t.ignore_case, cx))
            .child(toggle_row("lines-trim", "length", "Trim whitespace", "Strip spaces at the start and end of each line", self.trim, |t| &mut t.trim, cx))
            .child(toggle_row("lines-empty", "lines", "Remove empty lines", "Drop blank and whitespace-only lines", self.drop_empty, |t| &mut t.drop_empty, cx))
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
                                    .when(shuffling, |d| {
                                        d.child(ui::icon_btn("lines-reshuffle", "shuffle", &pal, cx.listener(|this, _, w, cx| {
                                            this.seed = random_u32();
                                            this.recompute(w, cx);
                                        })))
                                    })
                                    .child(ui::icon_btn("lines-swap", "swap", &pal, cx.listener(|this, _, w, cx| this.swap(w, cx))))
                                    .child(ui::copy_btn("lines-copy", self.out.clone(), &pal, window, cx))
                                    .child(zoom.button(&pal)),
                            )
                            .child(code_editor_el(&self.output, true, cx))
                            .child(ui::pane_foot(&pal).child(ui::dot(tone, &pal)).child(self.status.clone())),
                        &pal, window,
                    )),
            )
    }
}
