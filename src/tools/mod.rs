//! One view per tool. Views are created lazily the first time a tool opens,
//! which keeps start-up work to the chrome and the home grid.

mod codec;
mod color;
mod date;
mod hash;
mod json;
mod jwt;
mod lorem;
mod numbase;
mod password;
mod regex;
mod textcase;
mod uuid;

#[cfg(test)]
mod syntax_tests;

use gpui_kit::base::input::{InputBaseState, MultiLineMode};
use gpui_kit::component::input::{Editor, EditorState, InputEvent, InputState, Textarea, TextareaState, Input};
use gpui_kit::{
    AnyView, App, AppContext, Context, Entity, Focusable, IntoElement, ParentElement, Styled,
    Subscription, Window, div, prelude::FluentBuilder, px, relative,
};

use crate::registry::ToolId;
use crate::settings::Settings;
use crate::theme::{MONO_FONT, Pal};
use crate::ui;

/// Build the view for a tool.
pub fn create(id: ToolId, window: &mut Window, cx: &mut App) -> AnyView {
    match id {
        ToolId::JsonFmt => cx.new(|cx| json::JsonFmtView::new(window, cx)).into(),
        ToolId::JsonYaml => cx.new(|cx| json::JsonYamlView::new(window, cx)).into(),
        ToolId::NumBase => cx.new(|cx| numbase::NumBaseView::new(window, cx)).into(),
        ToolId::Date => cx.new(|cx| date::DateView::new(window, cx)).into(),
        ToolId::Base64 | ToolId::Url | ToolId::Html | ToolId::Escape => {
            cx.new(|cx| codec::CodecView::new(id, window, cx)).into()
        }
        ToolId::Jwt => cx.new(|cx| jwt::JwtView::new(window, cx)).into(),
        ToolId::Hash => cx.new(|cx| hash::HashView::new(window, cx)).into(),
        ToolId::Uuid => cx.new(|cx| uuid::UuidView::new(window, cx)).into(),
        ToolId::Password => cx.new(|cx| password::PasswordView::new(window, cx)).into(),
        ToolId::Lorem => cx.new(|cx| lorem::LoremView::new(window, cx)).into(),
        ToolId::Color => cx.new(|cx| color::ColorView::new(window, cx)).into(),
        ToolId::Regex => cx.new(|cx| regex::RegexView::new(window, cx)).into(),
        ToolId::TextCase => cx.new(|cx| textcase::TextCaseView::new(window, cx)).into(),
    }
}

/// Load `text` into a tool's main input (used by smart detection).
pub fn fill(id: ToolId, view: &AnyView, text: &str, window: &mut Window, cx: &mut App) {
    let text = text.trim();
    let v = view.clone();
    match id {
        ToolId::JsonFmt => {
            if let Ok(e) = v.downcast::<json::JsonFmtView>() {
                e.update(cx, |t, cx| t.set_input(text, window, cx));
            }
        }
        ToolId::Jwt => {
            if let Ok(e) = v.downcast::<jwt::JwtView>() {
                e.update(cx, |t, cx| t.set_input(text, window, cx));
            }
        }
        ToolId::Date => {
            if let Ok(e) = v.downcast::<date::DateView>() {
                e.update(cx, |t, cx| t.set_input(text, window, cx));
            }
        }
        ToolId::Color => {
            if let Ok(e) = v.downcast::<color::ColorView>() {
                e.update(cx, |t, cx| t.set_input(text, window, cx));
            }
        }
        ToolId::Base64 | ToolId::Url => {
            if let Ok(e) = v.downcast::<codec::CodecView>() {
                e.update(cx, |t, cx| t.set_input(text, true, window, cx));
            }
        }
        _ => {}
    }
}

/// A callback for controls that report an index (segmented controls, dropdowns),
/// routed back into the owning view.
pub fn on_index<V: 'static, F: Fn(&mut V, usize, &mut Window, &mut Context<V>) + 'static>(
    cx: &Context<V>,
    f: F,
) -> impl Fn(usize, &mut Window, &mut App) + 'static + use<V, F> {
    let weak = cx.weak_entity();
    move |i, window, cx| {
        let _ = weak.update(cx, |v, cx| f(v, i, window, cx));
    }
}

/// A multi-line editor state preloaded with `value`.
pub fn editor<V: 'static>(
    value: &str,
    placeholder: &str,
    window: &mut Window,
    cx: &mut Context<V>,
) -> Entity<TextareaState> {
    let value = value.to_string();
    let placeholder = placeholder.to_string();
    cx.new(|cx| {
        TextareaState::new(window, cx)
            .soft_wrap(false)
            .placeholder(placeholder)
            .default_value(value)
    })
}

/// A syntax-aware editor with the same editing behavior as the plain text panes.
pub fn code_editor<V: 'static>(
    value: &str,
    placeholder: &str,
    language: &'static str,
    window: &mut Window,
    cx: &mut Context<V>,
) -> Entity<EditorState> {
    cx.new(|cx| {
        EditorState::new(window, cx)
            .language(language)
            .line_number(false)
            .folding(false)
            .auto_close(false)
            .smart_indent(false)
            .soft_wrap(false)
            .placeholder(placeholder.to_string())
            .default_value(value.to_string())
    })
}

/// A single-line input state preloaded with `value`.
pub fn line<V: 'static>(
    value: &str,
    placeholder: &str,
    window: &mut Window,
    cx: &mut Context<V>,
) -> Entity<InputState> {
    let value = value.to_string();
    let placeholder = placeholder.to_string();
    cx.new(|cx| InputState::new(window, cx).placeholder(placeholder).default_value(value))
}

/// Re-render on focus changes and run `on_change` on edits.
pub fn watch<V: 'static, S: 'static + gpui_kit::EventEmitter<InputEvent>>(
    state: &Entity<S>,
    window: &mut Window,
    cx: &mut Context<V>,
    on_change: impl Fn(&mut V, &mut Window, &mut Context<V>) + 'static,
) -> Subscription {
    cx.subscribe_in(state, window, move |this, _, ev: &InputEvent, window, cx| match ev {
        InputEvent::Change => on_change(this, window, cx),
        InputEvent::Focus | InputEvent::Blur => cx.notify(),
        _ => {}
    })
}

pub fn is_focused<S: Focusable>(state: &Entity<S>, window: &Window, cx: &App) -> bool {
    state.read(cx).focus_handle(cx).contains_focused(window, cx)
}

pub fn text_of<M: MultiLineMode>(state: &Entity<InputBaseState<M>>, cx: &App) -> String {
    state.read(cx).value().to_string()
}

pub fn line_text(state: &Entity<InputState>, cx: &App) -> String {
    state.read(cx).value().to_string()
}

/// Replace an editor's text without emitting a change event.
pub fn set_text<M: MultiLineMode>(state: &Entity<InputBaseState<M>>, text: &str, window: &mut Window, cx: &mut App) {
    if state.read(cx).value().as_ref() != text {
        let text = text.to_string();
        state.update(cx, |s, cx| s.set_value(text, window, cx));
    }
}

pub fn set_line(state: &Entity<InputState>, text: &str, window: &mut Window, cx: &mut App) {
    let text = text.to_string();
    state.update(cx, |s, cx| s.set_value(text, window, cx));
}

/// Keep editors' soft wrap in line with the "Wrap long lines" setting.
/// `applied` remembers what was last pushed, since the state has no getter.
pub fn sync_wrap<M: MultiLineMode>(
    states: &[&Entity<InputBaseState<M>>],
    applied: &mut bool,
    window: &mut Window,
    cx: &mut App,
) {
    let wrap = Settings::get(cx).wrap;
    if *applied != wrap {
        *applied = wrap;
        for s in states {
            s.update(cx, |s, cx| s.set_soft_wrap(wrap, window, cx));
        }
    }
}

/// The editor surface inside a pane: mono text, design padding and line height.
pub fn editor_el(state: &Entity<TextareaState>, readonly: bool, cx: &App) -> gpui_kit::Div {
    let fs = Settings::get(cx).font_size as f32;
    div()
        .flex_1()
        .min_h_0()
        .font_family(MONO_FONT)
        .child(
            Textarea::new(state)
                .appearance(false)
                .readonly(readonly)
                .h_full()
                .text_size(px(fs))
                .line_height(relative(1.65)),
        )
}

/// Syntax-aware surface, retaining selection, copy, search and read-only behavior.
pub fn code_editor_el(state: &Entity<EditorState>, readonly: bool, cx: &App) -> gpui_kit::Div {
    let fs = Settings::get(cx).font_size as f32;
    div()
        .flex_1()
        .min_h_0()
        .font_family(MONO_FONT)
        .child(
            Editor::new(state)
                .appearance(false)
                .readonly(readonly)
                .h_full()
                .text_size(px(fs))
                .line_height(relative(1.65)),
        )
}

/// A single-line input inside a design `.field` frame.
pub fn field_el(state: &Entity<InputState>, mono: bool, height: f32, font: f32, window: &Window, cx: &App) -> gpui_kit::Div {
    let pal = Pal::get(cx);
    let focused = is_focused(state, window, cx);
    ui::field(focused, &pal)
        .h(px(height))
        .px(px(if height > 36. { 14. } else { 10. }))
        .text_size(px(font))
        .when(mono, |d| d.font_family(MONO_FONT))
        .child(
            div().flex_1().min_w_0().child(
                Input::new(state).appearance(false).text_size(px(font)).when(mono, |i| i.font_family(MONO_FONT)),
            ),
        )
}

/// Paste and Clear buttons for an input pane head.
pub fn paste_clear<V: 'static, M: MultiLineMode>(
    id: &'static str,
    state: &Entity<InputBaseState<M>>,
    pal: &Pal,
    cx: &mut Context<V>,
    after: impl Fn(&mut V, &mut Window, &mut Context<V>) + Clone + 'static,
) -> [gpui_kit::AnyElement; 2] {
    let (s1, s2) = (state.clone(), state.clone());
    let after2 = after.clone();
    [
        ui::icon_btn((id, 0usize), "paste", pal, cx.listener(move |this, _, window, cx| {
            if let Some(text) = cx.read_from_clipboard().and_then(|c| c.text()) {
                set_text(&s1, &text, window, cx);
                after(this, window, cx);
            }
        }))
        .into_any_element(),
        ui::icon_btn((id, 1usize), "x", pal, cx.listener(move |this, _, window, cx| {
            set_text(&s2, "", window, cx);
            after2(this, window, cx);
        }))
        .into_any_element(),
    ]
}
