//! Write Markdown on the left, see it rendered on the right.

use gpui_kit::component::input::EditorState;
use gpui_kit::component::scroll::ScrollableElement;
use gpui_kit::component::text::TextView;
use gpui_kit::{
    Context, Entity, InteractiveElement, IntoElement, ParentElement, Render, SharedString, Styled,
    Subscription, Window, div, prelude::FluentBuilder, px,
};

use super::*;
use crate::logic::plural;
use crate::ui;

/// The preview renders at most this much; laying out megabytes of rich text
/// takes minutes and gigabytes.
const PREVIEW_LIMIT: usize = 128 * 1024;

const SAMPLE: &str = "# Release notes\n\nSideKit **0.2** adds a few tools:\n\n- A *Markdown* preview (this one)\n- A cron expression reader\n- JSON ↔ CSV\n\n> Everything runs offline.\n\n```rust\nfn main() {\n    println!(\"hello\");\n}\n```\n\n| Tool | Category |\n| --- | --- |\n| Cron | Testers |\n| QR code | Generators |\n\n[Read more](https://example.com)\n";

pub struct MarkdownView {
    input: Entity<EditorState>,
    /// Bumped per edit so the preview re-parses.
    rev: usize,
    _subs: Vec<Subscription>,
}

impl MarkdownView {
    pub fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let input = code_editor(SAMPLE, "Write Markdown", "markdown", window, cx);
        input.update(cx, |s, cx| s.set_soft_wrap(true, window, cx));
        let subs = vec![watch(&input, window, cx, |this: &mut Self, _, cx| {
            this.rev += 1;
            cx.notify();
        })];
        Self { input, rev: 0, _subs: subs }
    }
}

impl Render for MarkdownView {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let pal = Pal::get(cx);
        let text = text_of(&self.input, cx);
        let words = text.split_whitespace().count();
        let [paste, clear] = paste_clear("md", &self.input, &pal, cx, |this: &mut Self, _, cx| {
            this.rev += 1;
            cx.notify();
        });
        let html_ish: SharedString = text.clone().into();
        let cut = text.len() > PREVIEW_LIMIT;
        let preview_text = if cut {
            let end = text.floor_char_boundary(PREVIEW_LIMIT);
            text[..text[..end].rfind('\n').unwrap_or(end)].to_string()
        } else {
            text.clone()
        };
        let zoom = ui::PaneZoom::new("md-preview", window, cx);

        div()
            .flex()
            .gap(px(12.))
            .flex_1()
            .min_h(px(460.))
            .child(
                ui::pane(is_focused(&self.input, window, cx), &pal)
                    .flex_1()
                    .child(ui::pane_head("Markdown", None, &pal).child(paste).child(clear))
                    .child(code_editor_el(&self.input, false, cx))
                    .child(ui::pane_foot(&pal).child(format!("{} · {}", plural(words, "word"), plural(text.lines().count(), "line")))),
            )
            .child(zoom.wrap(
                ui::pane(false, &pal)
                    .flex_1()
                    .child(ui::pane_head("Preview", None, &pal).child(ui::copy_btn("md-copy", html_ish, &pal, window, cx)).child(zoom.button(&pal)))
                    .child(
                        div()
                            .id("md-scroll")
                            .flex_1()
                            .min_h_0()
                            .overflow_y_scrollbar()
                            .px(px(20.))
                            .py(px(16.))
                            .child(TextView::markdown(crate::id!("md-view-{}", self.rev), preview_text).selectable(true))
                            .when(cut, |d| {
                                d.child(div().mt(px(16.)).text_size(px(12.)).text_color(pal.text3).child("The preview shows the first 128 KB."))
                            }),
                    ),
                &pal,
                window,
            ))
    }
}
