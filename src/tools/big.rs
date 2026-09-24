//! Keeping the window responsive with large inputs.
//!
//! Measured on a release build: the expensive part of a multi-megabyte paste
//! is rarely the conversion itself. It is the editor laying out very long
//! lines, tree-sitter building syntax trees for huge documents, and labels
//! shaping megabytes of text. So large results are computed off the UI
//! thread, panes show a bounded slice (Copy still takes everything), and very
//! large documents are shown as plain text.

use std::time::Duration;

use gpui_kit::component::input::EditorState;
use gpui_kit::{App, Context, Entity, Task, Window};

/// Lines longer than this are shortened for display: the editor shapes each
/// visible line in full on every frame.
pub const LONG_LINE: usize = 64 * 1024;
/// Inputs larger than this are processed in the background once typing pauses.
pub const LARGE: usize = 256 * 1024;
/// The most text an output pane shows.
pub const SHOW_LIMIT: usize = 512 * 1024;
/// Above this, editors drop syntax highlighting.
pub const PLAIN_ABOVE: usize = 2 * 1024 * 1024;
pub const DEBOUNCE: Duration = Duration::from_millis(150);

/// Shown in status lines when a pane holds only part of a result.
pub const SHORTENED: &str = "Showing part of the output · Copy gives all of it";

pub fn has_long_line(s: &str) -> bool {
    s.split('\n').any(|l| l.len() > LONG_LINE)
}

pub fn clip_long_lines(s: &str) -> String {
    s.split('\n')
        .map(|l| {
            if l.len() <= LONG_LINE {
                return l.to_string();
            }
            format!("{}…", &l[..l.floor_char_boundary(LONG_LINE)])
        })
        .collect::<Vec<_>>()
        .join("\n")
}

/// What a pane should show for `s`, or `None` when it can show `s` as it is:
/// over-long lines shortened, and at most [`SHOW_LIMIT`] bytes, cut at a line end.
pub fn for_display(s: &str) -> Option<String> {
    let clipped = has_long_line(s).then(|| clip_long_lines(s));
    let base = clipped.as_deref().unwrap_or(s);
    if base.len() <= SHOW_LIMIT {
        return clipped;
    }
    let end = base.floor_char_boundary(SHOW_LIMIT);
    let cut = base[..end].rfind('\n').unwrap_or(end);
    Some(format!("{}\n…", &base[..cut]))
}

/// The first `chars` characters, for one-line labels.
pub fn preview(s: &str, chars: usize) -> String {
    match s.char_indices().nth(chars) {
        Some((i, _)) => format!("{}…", &s[..i]),
        None => s.to_string(),
    }
}

/// Use `language` unless the text is too large to highlight.
pub fn fit_language(state: &Entity<EditorState>, language: &'static str, len: usize, cx: &mut App) {
    let want = if len > PLAIN_ABOVE { "plaintext" } else { language };
    if state.read(cx).language_name().as_ref() != want {
        state.update(cx, |s, cx| s.set_highlighter(want, cx));
    }
}

/// Run `work` right away for small inputs, or in the background after a
/// pause in typing for large ones, then hand the result to `done`. A newer
/// run replaces (cancels) one still waiting in `slot`.
pub fn run<V: 'static, T: Send + 'static>(
    size: usize,
    this: &mut V,
    slot: fn(&mut V) -> &mut Option<Task<()>>,
    window: &mut Window,
    cx: &mut Context<V>,
    work: impl FnOnce() -> T + Send + 'static,
    done: impl FnOnce(&mut V, T, &mut Window, &mut Context<V>) + 'static,
) {
    if size <= LARGE {
        *slot(this) = None;
        let r = work();
        done(this, r, window, cx);
        return;
    }
    *slot(this) = Some(cx.spawn_in(window, async move |view, cx| {
        cx.background_executor().timer(DEBOUNCE).await;
        let r = cx.background_executor().spawn(async move { work() }).await;
        let _ = view.update_in(cx, |v, w, cx| done(v, r, w, cx));
    }));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn display_bounds() {
        assert_eq!(for_display("short\ntext"), None);
        let long = "é".repeat(LONG_LINE);
        let shown = for_display(&long).unwrap();
        assert!(shown.len() <= LONG_LINE + '…'.len_utf8() && shown.ends_with('…'));
        let many = "0123456789\n".repeat(SHOW_LIMIT / 5);
        let shown = for_display(&many).unwrap();
        assert!(shown.len() <= SHOW_LIMIT + 4 && shown.ends_with("\n…"));
        assert!(shown.trim_end_matches(['\n', '…']).lines().all(|l| l == "0123456789"));
        assert_eq!(preview("abcdef", 3), "abc…");
        assert_eq!(preview("ab", 3), "ab");
    }
}
