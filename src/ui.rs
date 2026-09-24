//! Small building blocks that reproduce the design's CSS components.

use std::{rc::Rc, time::Duration};

use gpui_kit::{
    Animation, AnimationExt, AnyElement, App, ClickEvent, ClipboardItem, Div, ElementId,
    FontWeight, Hsla, InteractiveElement, IntoElement, ParentElement, Rgba, SharedString,
    SpringAnimation, SpringConfig, Stateful, StatefulInteractiveElement, Styled, Transformation,
    Window, anchored, deferred, div, point, prelude::FluentBuilder, px, relative, size,
};

use crate::icons::icon;
use crate::theme::{MONO_FONT, Pal};

/// Build an element id from a format string: `id!("card-{}", key)`.
#[macro_export]
macro_rules! id {
    ($($t:tt)*) => { gpui_kit::ElementId::Name(format!($($t)*).into()) };
}

/// A child id namespaced under `parent`.
pub fn child(parent: &ElementId, name: impl std::fmt::Display) -> ElementId {
    ElementId::NamedChild(std::sync::Arc::new(parent.clone()), name.to_string().into())
}

/// Blend two colors in RGB space (hue-safe for grey/white endpoints).
pub fn mix(a: Hsla, b: Hsla, t: f32) -> Hsla {
    let (a, b) = (Rgba::from(a), Rgba::from(b));
    Rgba {
        r: a.r + (b.r - a.r) * t,
        g: a.g + (b.g - a.g) * t,
        b: a.b + (b.b - a.b) * t,
        a: a.a + (b.a - a.a) * t,
    }
    .into()
}

/// CSS `cubic-bezier(x1, y1, x2, y2)` as an easing function.
pub fn bezier(x1: f32, y1: f32, x2: f32, y2: f32) -> impl Fn(f32) -> f32 {
    move |x: f32| {
        if x <= 0. {
            return 0.;
        }
        if x >= 1. {
            return 1.;
        }
        let bx = |t: f32| 3. * (1. - t) * (1. - t) * t * x1 + 3. * (1. - t) * t * t * x2 + t * t * t;
        let by = |t: f32| 3. * (1. - t) * (1. - t) * t * y1 + 3. * (1. - t) * t * t * y2 + t * t * t;
        let (mut lo, mut hi) = (0f32, 1f32);
        for _ in 0..20 {
            let mid = (lo + hi) / 2.;
            if bx(mid) < x { lo = mid } else { hi = mid }
        }
        by((lo + hi) / 2.)
    }
}

/// `cubic-bezier(.2,.8,.2,1)`, the design's default motion curve.
pub fn ease_fluent() -> impl Fn(f32) -> f32 {
    bezier(0.2, 0.8, 0.2, 1.0)
}

/// Springs tuned to the design's transitions.
pub fn spring_soft() -> SpringConfig {
    SpringConfig::new(380., 34., 1.)
}
pub fn spring_bouncy() -> SpringConfig {
    SpringConfig::new(420., 24., 1.)
}

/// `.enter` / `.enter-2` / `.enter-3`: fade and rise 10px when a view appears.
pub fn enter<E: Styled + IntoElement + 'static>(id: impl Into<ElementId>, delay_ms: u64, el: E) -> AnyElement {
    let total = 320 + delay_ms;
    let ease = ease_fluent();
    el.relative().with_animation(
        id,
        Animation::new(Duration::from_millis(total)),
        move |el, t| {
            let ms = t * total as f32;
            let p = ((ms - delay_ms as f32) / 320.).clamp(0., 1.);
            let e = ease(p);
            el.opacity(e).top(px(10. * (1. - e)))
        },
    )
    .into_any_element()
}

pub fn section_label(text: impl Into<SharedString>, pal: &Pal) -> Div {
    div()
        .text_size(px(13.))
        .font_weight(FontWeight::SEMIBOLD)
        .text_color(pal.text)
        .mt(px(6.))
        .mb(px(2.))
        .child(text.into())
}

/// `.setting`: a card row with icon, title/description and a trailing control.
pub fn setting(
    id: impl Into<ElementId>,
    leading: impl IntoElement,
    title: impl Into<SharedString>,
    desc: Option<SharedString>,
    control: impl IntoElement,
    pal: &Pal,
) -> Stateful<Div> {
    let p = *pal;
    div()
        .id(id)
        .flex()
        .items_center()
        .gap(px(16.))
        .min_h(px(64.))
        .py(px(12.))
        .pl(px(18.))
        .pr(px(16.))
        .bg(p.card)
        .border_1()
        .border_color(p.stroke)
        .rounded(px(7.))
        .hover(move |s| s.bg(p.card_hover).border_color(p.stroke_strong))
        .child(leading)
        .child(
            div()
                .flex()
                .flex_col()
                .gap(px(2.))
                .flex_1()
                .min_w_0()
                .child(div().text_size(px(14.)).child(title.into()))
                .when_some(desc, |d, desc| {
                    d.child(div().text_size(px(12.)).text_color(p.text3).child(desc))
                }),
        )
        .child(control)
}

pub fn setting_icon(name: &str, pal: &Pal) -> gpui_kit::Svg {
    icon(name, 20., pal.text2)
}

/// `.toggle` switch with a springy knob.
pub fn toggle(
    id: impl Into<ElementId>,
    on: bool,
    pal: &Pal,
    on_click: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
) -> Stateful<Div> {
    let p = *pal;
    let id: ElementId = id.into();
    let group: SharedString = format!("tg-{id}").into();
    let knob = div()
        .absolute()
        .top(px(4.))
        .size(px(10.))
        .rounded(px(6.))
        .bg(if on { p.accent_text } else { p.text2 })
        .group_hover(group.clone(), |s| s.size(px(12.)).top(px(3.)))
        .with_spring(
            child(&id, "knob"),
            SpringAnimation::new(spring_bouncy()).to(on),
            |el, phase| el.left(phase.interpolate(px(4.), px(24.))),
        );
    div()
        .id(id)
        .group(group)
        .relative()
        .flex_none()
        .w(px(40.))
        .h(px(20.))
        .rounded(px(10.))
        .border_1()
        .cursor_pointer()
        .when(on, |d| d.bg(p.accent).border_color(p.accent).hover(move |s| s.bg(p.accent_hover)))
        .when(!on, |d| d.border_color(p.text2).hover(move |s| s.bg(p.subtle)))
        .on_click(on_click)
        .child(knob)
}

/// `.toggle-wrap`: On/Off label followed by a toggle.
pub fn toggle_labeled(
    id: impl Into<ElementId>,
    on: bool,
    pal: &Pal,
    on_click: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
) -> Div {
    div()
        .flex()
        .items_center()
        .gap(px(12.))
        .text_size(px(13.))
        .text_color(pal.text2)
        .child(div().w(px(22.)).child(if on { "On" } else { "Off" }))
        .child(toggle(id, on, pal, on_click))
}

/// `.seg`: segmented control.
pub fn seg(
    id: impl Into<ElementId>,
    labels: &[&'static str],
    selected: usize,
    pal: &Pal,
    on_pick: impl Fn(usize, &mut Window, &mut App) + 'static,
) -> Div {
    let p = *pal;
    let id: ElementId = id.into();
    let on_pick = Rc::new(on_pick);
    div()
        .flex()
        .flex_none()
        .gap(px(2.))
        .p(px(2.))
        .rounded(px(7.))
        .bg(p.subtle)
        .border_1()
        .border_color(p.stroke)
        .children(labels.iter().enumerate().map(|(i, label)| {
            let sel = i == selected;
            let on_pick = on_pick.clone();
            div()
                .id(child(&id, i))
                .h(px(28.))
                .px(px(14.))
                .flex()
                .items_center()
                .rounded(px(5.))
                .text_size(px(13.))
                .cursor_pointer()
                .when(sel, |d| {
                    d.bg(p.card)
                        .text_color(p.text)
                        .font_weight(FontWeight::SEMIBOLD)
                        .shadow(vec![
                            gpui_kit::BoxShadow {
                                color: gpui_kit::hsla(0., 0., 0., 0.12),
                                offset: point(px(0.), px(1.)),
                                blur_radius: px(2.),
                                spread_radius: px(0.),
                                inset: false,
                            },
                            Pal::ring(p.stroke, 1.),
                        ])
                })
                .when(!sel, |d| d.text_color(p.text2).hover(move |s| s.text_color(p.text)))
                .on_click(move |_, w, cx| on_pick(i, w, cx))
                .child(*label)
        }))
}

pub enum BtnKind {
    Normal,
    Accent,
}

/// `.btn` / `.btn.accent`.
pub fn btn(
    id: impl Into<ElementId>,
    icon_name: Option<&str>,
    label: impl Into<SharedString>,
    kind: BtnKind,
    pal: &Pal,
    on_click: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
) -> Stateful<Div> {
    let p = *pal;
    let accent = matches!(kind, BtnKind::Accent);
    let fg = if accent { p.accent_text } else { p.text };
    div()
        .id(id)
        .flex()
        .flex_none()
        .items_center()
        .gap(px(8.))
        .h(px(32.))
        .px(px(14.))
        .rounded(px(6.))
        .text_size(px(13.))
        .whitespace_nowrap()
        .cursor_pointer()
        .text_color(fg)
        .when(accent, |d| {
            d.bg(p.accent)
                .font_weight(FontWeight::SEMIBOLD)
                .hover(move |s| s.bg(p.accent_hover).shadow(vec![gpui_kit::BoxShadow {
                    color: p.accent_soft,
                    offset: point(px(0.), px(2.)),
                    blur_radius: px(10.),
                    spread_radius: px(0.),
                    inset: false,
                }]))
        })
        .when(!accent, |d| {
            d.bg(p.card)
                .border_1()
                .border_color(p.stroke_strong)
                .hover(move |s| s.bg(p.card_hover))
        })
        .active(|s| s.opacity(0.85))
        .when_some(icon_name, |d, n| d.child(icon(n, 16., fg)))
        .child(label.into())
        .on_click(on_click)
}

/// `.icon-btn`: a borderless square button.
pub fn icon_btn(
    id: impl Into<ElementId>,
    icon_name: &str,
    pal: &Pal,
    on_click: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
) -> Stateful<Div> {
    let p = *pal;
    div()
        .id(id)
        .flex()
        .flex_none()
        .items_center()
        .justify_center()
        .size(px(32.))
        .rounded(px(6.))
        .cursor_pointer()
        .hover(move |s| s.bg(p.subtle2))
        .active(move |s| s.bg(p.stroke_strong))
        .child(icon(icon_name, 16., p.text2))
        .on_click(on_click)
}

/// The copy icon that flips to a popping check for 1.4s after copying.
fn copy_glyph(id: ElementId, copied: bool, pal: &Pal) -> AnyElement {
    if copied {
        icon("check", 16., pal.ok)
            .with_animation(
                child(&id, "pop"),
                Animation::new(Duration::from_millis(350)).with_easing(bezier(0.3, 1.7, 0.5, 1.)),
                |svg, t| {
                    let s = 0.3 + 0.7 * t;
                    svg.with_transformation(Transformation::scale(size(s, s)))
                },
            )
            .into_any_element()
    } else {
        icon("copy", 16., pal.text2).into_any_element()
    }
}

fn use_copied(id: &ElementId, window: &mut Window, cx: &mut App) -> gpui_kit::Entity<bool> {
    window.use_keyed_state(child(id, "copied"), cx, |_, _| false)
}

fn flash_copied(state: &gpui_kit::Entity<bool>, text: &str, cx: &mut App) {
    cx.write_to_clipboard(ClipboardItem::new_string(text.to_string()));
    state.update(cx, |v, cx| {
        *v = true;
        cx.notify();
    });
    let state = state.downgrade();
    cx.spawn(async move |cx| {
        cx.background_executor().timer(Duration::from_millis(1400)).await;
        let _ = state.update(cx, |v, cx| {
            *v = false;
            cx.notify();
        });
    })
    .detach();
}

/// Icon button that copies `text` to the clipboard.
pub fn copy_btn(
    id: impl Into<ElementId>,
    text: SharedString,
    pal: &Pal,
    window: &mut Window,
    cx: &mut App,
) -> Stateful<Div> {
    let p = *pal;
    let id: ElementId = id.into();
    let state = use_copied(&id, window, cx);
    let copied = *state.read(cx);
    div()
        .id(id.clone())
        .flex()
        .flex_none()
        .items_center()
        .justify_center()
        .size(px(32.))
        .rounded(px(6.))
        .cursor_pointer()
        .hover(move |s| s.bg(p.subtle2))
        .active(move |s| s.bg(p.stroke_strong))
        .child(copy_glyph(id, copied, pal))
        .on_click(move |_, _, cx| flash_copied(&state, &text, cx))
}

/// `.btn` with a copy glyph and label ("Copy all").
pub fn copy_btn_labeled(
    id: impl Into<ElementId>,
    label: &'static str,
    text: SharedString,
    pal: &Pal,
    window: &mut Window,
    cx: &mut App,
) -> Stateful<Div> {
    let p = *pal;
    let id: ElementId = id.into();
    let state = use_copied(&id, window, cx);
    let copied = *state.read(cx);
    div()
        .id(id.clone())
        .flex()
        .flex_none()
        .items_center()
        .gap(px(8.))
        .h(px(32.))
        .px(px(14.))
        .rounded(px(6.))
        .text_size(px(13.))
        .cursor_pointer()
        .bg(p.card)
        .border_1()
        .border_color(if copied { p.ok } else { p.stroke_strong })
        .text_color(if copied { p.ok } else { p.text })
        .hover(move |s| s.bg(p.card_hover))
        .child(copy_glyph(id, copied, pal))
        .child(label)
        .on_click(move |_, _, cx| flash_copied(&state, &text, cx))
}

#[derive(Clone, Copy, PartialEq)]
pub enum Tone {
    Ok,
    Err,
    Info,
    Neutral,
}

/// `.badge`.
pub fn badge(text: impl Into<SharedString>, tone: Tone, pal: &Pal) -> Div {
    badge_base(tone, pal).child(text.into())
}

/// An empty `.badge` shell for custom content.
pub fn badge_base(tone: Tone, pal: &Pal) -> Div {
    let (bg, fg) = match tone {
        Tone::Ok => (pal.ok_soft, pal.ok),
        Tone::Err => (pal.danger_soft, pal.danger),
        Tone::Info => (pal.accent_soft, pal.accent),
        Tone::Neutral => (pal.subtle2, pal.text2),
    };
    div()
        .flex()
        .flex_none()
        .items_center()
        .gap(px(6.))
        .h(px(22.))
        .px(px(8.))
        .rounded(px(11.))
        .text_size(px(11.5))
        .font_weight(FontWeight::SEMIBOLD)
        .whitespace_nowrap()
        .bg(bg)
        .text_color(fg)
}

/// `.dot` status indicator; an error dot pulses.
pub fn dot(tone: Option<Tone>, pal: &Pal) -> AnyElement {
    let base = div().size(px(8.)).rounded_full().flex_none();
    match tone {
        Some(Tone::Ok) => base.bg(pal.ok).shadow(vec![Pal::ring(pal.ok_soft, 3.)]).into_any_element(),
        Some(Tone::Err) => {
            let (c, soft) = (pal.danger, pal.danger_soft);
            base.bg(c)
                .with_animation(
                    "dot-pulse",
                    Animation::new(Duration::from_millis(1600)).repeat(),
                    move |el, t| {
                        let k = 1. - (t * 2. - 1.).abs();
                        let mut s = soft;
                        s.a *= 1. - k;
                        el.shadow(vec![Pal::ring(s, 3. + 2. * k)])
                    },
                )
                .into_any_element()
        }
        _ => base.bg(pal.stroke_strong).into_any_element(),
    }
}

/// `.err-box`.
pub fn err_box(text: impl Into<SharedString>, pal: &Pal) -> Div {
    div()
        .px(px(12.))
        .py(px(10.))
        .rounded(px(6.))
        .bg(pal.danger_soft)
        .text_color(pal.danger)
        .text_size(px(12.5))
        .font_family(MONO_FONT)
        .child(text.into())
}

/// `.pane`: an editor frame whose border lights up while focused.
pub fn pane(focused: bool, pal: &Pal) -> Div {
    div()
        .flex()
        .flex_col()
        .min_h_0()
        .min_w_0()
        .bg(pal.editor)
        .border_1()
        .rounded(px(8.))
        .overflow_hidden()
        .when(focused, |d| d.border_color(pal.accent).shadow(vec![Pal::ring(pal.accent_soft, 3.)]))
        .when(!focused, |d| d.border_color(pal.stroke))
}

struct PaneZoomState {
    expanded: bool,
    focus: gpui_kit::FocusHandle,
    previous_focus: Option<gpui_kit::FocusHandle>,
}

/// Moves a single pane into a window-sized overlay without duplicating its editor.
pub struct PaneZoom {
    id: ElementId,
    state: gpui_kit::Entity<PaneZoomState>,
    expanded: bool,
    focus: gpui_kit::FocusHandle,
}

impl PaneZoom {
    pub fn new(id: impl Into<ElementId>, window: &mut Window, cx: &mut App) -> Self {
        let id = id.into();
        let state = window.use_keyed_state(child(&id, "zoom"), cx, |_, cx| PaneZoomState {
            expanded: false,
            focus: cx.focus_handle(),
            previous_focus: None,
        });
        let expanded = state.read(cx).expanded;
        let focus = state.read(cx).focus.clone();
        Self { id, state, expanded, focus }
    }

    pub fn button(&self, pal: &Pal) -> Stateful<Div> {
        let state = self.state.clone();
        let label = if self.expanded { "Restore pane (Esc)" } else { "Expand pane" };
        icon_btn(child(&self.id, "zoom-button"), if self.expanded { "shrink" } else { "expand" }, pal, move |_, window, cx| {
            state.update(cx, |s, cx| {
                if s.expanded {
                    s.expanded = false;
                    if let Some(focus) = s.previous_focus.take() {
                        window.focus(&focus, cx);
                    }
                } else {
                    s.previous_focus = window.focused(cx);
                    s.expanded = true;
                    window.focus(&s.focus, cx);
                }
                cx.notify();
            });
            window.refresh();
        })
        .tooltip(move |w, cx| gpui_kit::component::tooltip::Tooltip::new(label).build(w, cx))
    }

    pub fn wrap(&self, pane: Div, pal: &Pal, window: &Window) -> AnyElement {
        if !self.expanded {
            return pane.into_any_element();
        }
        let viewport = window.viewport_size();
        let state = self.state.clone();
        let overlay = div()
            .id(child(&self.id, "expanded"))
            .occlude()
            .track_focus(&self.focus)
            .w(viewport.width)
            .h((viewport.height - px(48.)).max(px(0.)))
            .p(px(16.))
            .bg(pal.layer)
            .flex()
            .flex_col()
            .on_mouse_down(gpui_kit::MouseButton::Left, |_, _, cx| cx.stop_propagation())
            .on_scroll_wheel(|_, _, cx| cx.stop_propagation())
            .capture_key_down(move |ev, window, cx| {
                if ev.keystroke.key == "escape" {
                    state.update(cx, |s, cx| {
                        s.expanded = false;
                        if let Some(focus) = s.previous_focus.take() {
                            window.focus(&focus, cx);
                        }
                        cx.notify();
                    });
                    window.refresh();
                    cx.stop_propagation();
                }
            })
            .child(pane.m_0().size_full().min_h_0());
        div()
            .flex_1()
            .min_w_0()
            .child(deferred(anchored().position(point(px(0.), px(48.))).child(overlay)).with_priority(1))
            .into_any_element()
    }
}

/// `.pane-head`.
pub fn pane_head(title: impl Into<SharedString>, title_color: Option<Hsla>, pal: &Pal) -> Div {
    div()
        .flex()
        .flex_none()
        .items_center()
        .gap(px(2.))
        .h(px(40.))
        .pl(px(14.))
        .pr(px(4.))
        .border_b_1()
        .border_color(pal.stroke)
        .bg(pal.card)
        .child(
            div()
                .flex_1()
                .text_size(px(13.))
                .font_weight(FontWeight::SEMIBOLD)
                .text_color(title_color.unwrap_or(pal.text))
                .child(title.into()),
        )
}

/// `.pane-foot`.
pub fn pane_foot(pal: &Pal) -> Div {
    div()
        .flex()
        .flex_none()
        .items_center()
        .gap(px(8.))
        .h(px(30.))
        .px(px(12.))
        .border_t_1()
        .border_color(pal.stroke)
        .bg(pal.card)
        .text_size(px(12.))
        .text_color(pal.text3)
}

/// `.kv-card`.
pub fn kv_card(pal: &Pal) -> Div {
    div()
        .flex()
        .flex_col()
        .bg(pal.card)
        .border_1()
        .border_color(pal.stroke)
        .rounded(px(8.))
        .overflow_hidden()
}

/// `.kv` row: label column, value column and a trailing slot.
pub fn kv_row(id: impl Into<ElementId>, last: bool, pal: &Pal) -> Stateful<Div> {
    let p = *pal;
    div()
        .id(id)
        .flex()
        .items_center()
        .gap(px(14.))
        .min_h(px(46.))
        .py(px(6.))
        .pl(px(18.))
        .pr(px(8.))
        .when(!last, |d| d.border_b_1().border_color(p.stroke))
        .hover(move |s| s.bg(p.subtle))
}

pub fn kv_label(text: impl Into<SharedString>, width: f32, pal: &Pal) -> Div {
    div()
        .w(px(width))
        .flex_none()
        .text_size(px(13.))
        .text_color(pal.text2)
        .child(text.into())
}

/// `.kv-val`: monospace value, ellipsized unless `wrap`.
pub fn kv_val(text: impl Into<SharedString>, wrap: bool) -> Div {
    div()
        .flex_1()
        .min_w_0()
        .font_family(MONO_FONT)
        .text_size(px(13.))
        .when(!wrap, |d| d.whitespace_nowrap().overflow_hidden().text_ellipsis())
        .when(wrap, |d| d.line_height(relative(1.5)))
        .child(text.into())
}

/// A label / value / copy row, the most common output shape.
pub fn kv_copy_row(
    id: impl Into<ElementId>,
    label: impl Into<SharedString>,
    label_w: f32,
    value: SharedString,
    wrap: bool,
    last: bool,
    pal: &Pal,
    window: &mut Window,
    cx: &mut App,
) -> Stateful<Div> {
    let id: ElementId = id.into();
    kv_row(id.clone(), last, pal)
        .child(kv_label(label, label_w, pal))
        .child(kv_val(value.clone(), wrap))
        .child(copy_btn(child(&id, "copy"), value, pal, window, cx))
}

/// `.field` frame for a single-line input: Fluent underline, accent on focus.
pub fn field(focused: bool, pal: &Pal) -> Div {
    let p = *pal;
    div()
        .flex()
        .items_center()
        .h(px(32.))
        .px(px(10.))
        .rounded(px(6.))
        .border_1()
        .border_color(p.stroke_strong)
        .bg(p.editor)
        .text_size(px(13.))
        .shadow(vec![Pal::underline(if focused { p.accent } else { p.stroke_strong }, if focused { 2. } else { 1. })])
        .when(!focused, |d| d.hover(move |s| s.border_color(p.text3)))
}

/// A dropdown in the style of the design's `select.field`.
pub fn dropdown(
    id: impl Into<ElementId>,
    options: &'static [&'static str],
    selected: usize,
    pal: &Pal,
    window: &mut Window,
    cx: &mut App,
    on_change: impl Fn(usize, &mut Window, &mut App) + 'static,
) -> Div {
    let p = *pal;
    let id: ElementId = id.into();
    let open = window.use_keyed_state(child(&id, "open"), cx, |_, _| false);
    let is_open = *open.read(cx);
    let on_change = Rc::new(on_change);
    let toggle_state = open.clone();
    div()
        .relative()
        .flex_none()
        .child(
            field(false, pal)
                .id(id.clone())
                .min_w(px(150.))
                .pr(px(10.))
                .gap(px(8.))
                .cursor_pointer()
                .child(div().flex_1().child(options[selected]))
                .child(icon("chev-down", 12., p.text3))
                .on_click(move |_, _, cx| {
                    toggle_state.update(cx, |v, cx| {
                        *v = !*v;
                        cx.notify();
                    })
                }),
        )
        .when(is_open, |d| {
            let close = open.clone();
            d.child(div().absolute().top(px(36.)).left_0().child(
                deferred(
                    anchored().snap_to_window_with_margin(px(8.)).child(
                        div()
                            .id(child(&id, "menu"))
                            .occlude()
                            .min_w(px(150.))
                            .p(px(4.))
                            .flex()
                            .flex_col()
                            .gap(px(2.))
                            .bg(p.card)
                            .border_1()
                            .border_color(p.stroke_strong)
                            .rounded(px(8.))
                            .shadow(p.shadow())
                            .text_size(px(13.))
                            .text_color(p.text)
                            .on_mouse_down_out({
                                let close = close.clone();
                                move |_, _, cx| {
                                    close.update(cx, |v, cx| {
                                        *v = false;
                                        cx.notify();
                                    })
                                }
                            })
                            .children(options.iter().enumerate().map(|(i, label)| {
                                let sel = i == selected;
                                let close = close.clone();
                                let on_change = on_change.clone();
                                div()
                                    .id(child(&id, i))
                                    .relative()
                                    .flex()
                                    .items_center()
                                    .h(px(32.))
                                    .pl(px(14.))
                                    .pr(px(12.))
                                    .rounded(px(5.))
                                    .cursor_pointer()
                                    .hover(move |s| s.bg(p.subtle2))
                                    .when(sel, |d| {
                                        d.bg(p.subtle).child(
                                            div()
                                                .absolute()
                                                .left(px(2.))
                                                .top(px(9.))
                                                .bottom(px(9.))
                                                .w(px(3.))
                                                .rounded(px(3.))
                                                .bg(p.accent),
                                        )
                                    })
                                    .child(*label)
                                    .on_click(move |_, w, cx| {
                                        close.update(cx, |v, cx| {
                                            *v = false;
                                            cx.notify();
                                        });
                                        on_change(i, w, cx);
                                    })
                            })),
                    ),
                )
                .with_priority(1),
            ))
        })
}

/// A plain divider line (`height: 1px; margin: 6px 10px`).
pub fn divider(pal: &Pal) -> Div {
    div().h(px(1.)).mx(px(10.)).my(px(6.)).bg(pal.stroke).flex_none()
}

/// A large figure over a small caption, for rows of text statistics.
pub fn stat_tile(value: impl Into<SharedString>, label: &'static str, pal: &Pal) -> Stateful<Div> {
    let p = *pal;
    div()
        .id(label)
        .flex()
        .flex_col()
        .gap(px(2.))
        .px(px(16.))
        .py(px(12.))
        .min_w_0()
        .rounded(px(8.))
        .bg(p.card)
        .border_1()
        .border_color(p.stroke)
        .hover(move |s| s.border_color(p.accent))
        .child(div().text_size(px(22.)).font_weight(FontWeight::SEMIBOLD).child(value.into()))
        .child(div().text_size(px(12.)).text_color(p.text3).child(label))
}

pub struct MenuEntry {
    pub label: SharedString,
    pub icon: Option<&'static str>,
    pub danger: bool,
}

impl MenuEntry {
    pub fn new(label: impl Into<SharedString>, icon: Option<&'static str>) -> Self {
        Self { label: label.into(), icon, danger: false }
    }

    pub fn danger(mut self) -> Self {
        self.danger = true;
        self
    }
}

/// A button that opens a list of actions, anchored under its right edge.
/// Without a label it is an icon button (default: "more").
#[allow(clippy::too_many_arguments)]
pub fn menu_btn(
    id: impl Into<ElementId>,
    icon_name: Option<&'static str>,
    label: Option<SharedString>,
    kind: BtnKind,
    entries: Vec<MenuEntry>,
    pal: &Pal,
    window: &mut Window,
    cx: &mut App,
    on_pick: impl Fn(usize, &mut Window, &mut App) + 'static,
) -> Div {
    let p = *pal;
    let id: ElementId = id.into();
    let open = window.use_keyed_state(child(&id, "open"), cx, |_, _| false);
    let is_open = *open.read(cx);
    let on_pick = Rc::new(on_pick);
    let toggle_state = open.clone();
    let toggle = move |_: &ClickEvent, _: &mut Window, cx: &mut App| {
        toggle_state.update(cx, |v, cx| {
            *v = !*v;
            cx.notify();
        })
    };
    let chev = if matches!(kind, BtnKind::Accent) { p.accent_text } else { p.text3 };
    let trigger = match label {
        Some(l) => btn(child(&id, "btn"), icon_name, l, kind, pal, toggle).child(icon("chev-down", 12., chev)),
        None => icon_btn(child(&id, "btn"), icon_name.unwrap_or("more"), pal, toggle),
    };
    div().relative().flex_none().child(trigger).when(is_open, |d| {
        let close = open.clone();
        d.child(div().absolute().top(px(36.)).right_0().child(
            deferred(
                anchored().snap_to_window_with_margin(px(8.)).child(
                    div()
                        .id(child(&id, "menu"))
                        .occlude()
                        .min_w(px(220.))
                        .p(px(4.))
                        .flex()
                        .flex_col()
                        .gap(px(2.))
                        .bg(p.card)
                        .border_1()
                        .border_color(p.stroke_strong)
                        .rounded(px(8.))
                        .shadow(p.shadow())
                        .text_size(px(13.))
                        .text_color(p.text)
                        .on_mouse_down_out({
                            let close = close.clone();
                            move |_, _, cx| {
                                close.update(cx, |v, cx| {
                                    *v = false;
                                    cx.notify();
                                })
                            }
                        })
                        .children(entries.into_iter().enumerate().map(|(i, e)| {
                            let close = close.clone();
                            let on_pick = on_pick.clone();
                            let fg = if e.danger { p.danger } else { p.text };
                            div()
                                .id(child(&id, i))
                                .flex()
                                .items_center()
                                .gap(px(10.))
                                .h(px(32.))
                                .px(px(10.))
                                .rounded(px(5.))
                                .cursor_pointer()
                                .whitespace_nowrap()
                                .text_color(fg)
                                .hover(move |s| s.bg(p.subtle2))
                                .when_some(e.icon, |d, n| d.child(icon(n, 15., fg)))
                                .child(e.label)
                                .on_click(move |_, w, cx| {
                                    close.update(cx, |v, cx| {
                                        *v = false;
                                        cx.notify();
                                    });
                                    on_pick(i, w, cx);
                                })
                        })),
                ),
            )
            .with_priority(1),
        ))
    })
}
