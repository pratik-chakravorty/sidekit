//! The Ctrl+K command palette: fuzzy search over every tool and a few commands.

use std::time::Duration;

use gpui_kit::component::input::{Input, InputEvent, InputState};
use gpui_kit::base::actions::{Cancel, Confirm, SelectDown, SelectUp};
use gpui_kit::{
    Animation, AnimationExt, App, AppContext, Context, EventEmitter, FontWeight,
    InteractiveElement, IntoElement, KeyBinding, ParentElement, Render, ScrollHandle,
    SharedString, StatefulInteractiveElement, Styled, Subscription, Window,
    deferred, div, hsla, prelude::FluentBuilder, px, relative,
};

use crate::app::kbd_el;
use crate::icons::icon;
use crate::registry::{TOOLS, ToolId, cat, tool};
use crate::settings::Settings;
use crate::theme::Pal;
use crate::ui;

const CONTEXT: &str = "Palette";

pub fn bind_keys(cx: &mut App) {
    cx.bind_keys([
        KeyBinding::new("escape", Cancel, Some(CONTEXT)),
        KeyBinding::new("enter", Confirm { secondary: false }, Some(CONTEXT)),
        KeyBinding::new("up", SelectUp, Some(CONTEXT)),
        KeyBinding::new("down", SelectDown, Some(CONTEXT)),
        KeyBinding::new("ctrl-p", SelectUp, Some(CONTEXT)),
        KeyBinding::new("ctrl-n", SelectDown, Some(CONTEXT)),
    ]);
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum PaletteAction {
    Open(ToolId),
    Home,
    Settings,
    ToggleTheme,
    ToggleFavorite(ToolId),
    ToggleWrap,
    Back,
}

pub enum PaletteEvent {
    Run(PaletteAction),
    Dismiss,
}

struct Entry {
    action: PaletteAction,
    title: SharedString,
    subtitle: SharedString,
    icon: &'static str,
    hint: SharedString,
    keywords: String,
    fav: bool,
}

pub struct Palette {
    input: Entity<InputState>,
    entries: Vec<Entry>,
    /// Indices into `entries`, best match first.
    results: Vec<usize>,
    selected: usize,
    scroll: ScrollHandle,
    _sub: Subscription,
}

use gpui_kit::Entity;

/// Subsequence fuzzy score: higher is better, `None` if not all chars appear.
/// Rewards matches at word starts and consecutive runs, penalizes gaps.
pub fn fuzzy(query: &str, text: &str) -> Option<i32> {
    if query.is_empty() {
        return Some(0);
    }
    let t: Vec<char> = text.to_lowercase().chars().collect();
    let mut score = 0;
    let mut ti = 0;
    let mut prev_match: Option<usize> = None;
    let mut first: Option<usize> = None;
    for qc in query.chars() {
        let mut found = None;
        while ti < t.len() {
            if t[ti] == qc {
                found = Some(ti);
                ti += 1;
                break;
            }
            ti += 1;
        }
        let i = found?;
        first.get_or_insert(i);
        let word_start = i == 0 || !t[i - 1].is_alphanumeric();
        score += 1;
        if word_start {
            score += 8;
        }
        if prev_match == Some(i.wrapping_sub(1)) {
            score += 5;
        } else if let Some(p) = prev_match {
            score -= ((i - p) as i32).min(6);
        }
        prev_match = Some(i);
    }
    score -= first.unwrap_or(0).min(10) as i32;
    Some(score)
}

impl Palette {
    pub fn new(current: Option<ToolId>, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let input = cx.new(|cx| InputState::new(window, cx).placeholder("Search tools and commands…"));
        let _sub = cx.subscribe_in(&input, window, |this, _, ev: &InputEvent, _, cx| {
            if let InputEvent::Change = ev {
                this.filter(cx);
            }
        });
        input.update(cx, |s, cx| s.focus(window, cx));

        let settings = Settings::get(cx);
        let mut entries: Vec<Entry> = TOOLS
            .iter()
            .map(|t| Entry {
                action: PaletteAction::Open(t.id),
                title: t.name.into(),
                subtitle: t.desc.into(),
                icon: t.icon,
                hint: cat(t.cat).label.into(),
                keywords: format!("{} {} {} {}", t.title, t.keywords, cat(t.cat).label, t.key),
                fav: settings.is_fav(t.key),
            })
            .collect();
        let dark = settings.dark;
        let wrap = settings.wrap;
        let mut cmd = |action, title: &str, subtitle: &str, icon, keywords: &str| {
            entries.push(Entry {
                action,
                title: title.to_string().into(),
                subtitle: subtitle.to_string().into(),
                icon,
                hint: "Command".into(),
                keywords: keywords.into(),
                fav: false,
            })
        };
        cmd(PaletteAction::Home, "All tools", "Browse every tool", "grid", "home browse");
        cmd(PaletteAction::Settings, "Settings", "Theme, editor and behavior preferences", "gear", "preferences options config");
        cmd(
            PaletteAction::ToggleTheme,
            if dark { "Switch to light theme" } else { "Switch to dark theme" },
            "Change the app theme",
            if dark { "sun" } else { "moon" },
            "theme dark light mode appearance",
        );
        cmd(
            PaletteAction::ToggleWrap,
            if wrap { "Disable line wrapping" } else { "Enable line wrapping" },
            "Soft-wrap long lines in editors",
            "wrap",
            "wrap lines editor",
        );
        if let Some(id) = current {
            let t = tool(id);
            let on = settings.is_fav(t.key);
            let title = if on { format!("Remove {} from favorites", t.name) } else { format!("Add {} to favorites", t.name) };
            cmd(PaletteAction::ToggleFavorite(id), &title, "Pin the current tool to the navigation", "star", "favorite pin star");
        }
        cmd(PaletteAction::Back, "Go back", "Return to the previous page", "back", "back previous history");

        let mut this = Self { input, entries, results: Vec::new(), selected: 0, scroll: ScrollHandle::new(), _sub };
        this.filter(cx);
        this
    }

    pub fn focus(&self, window: &mut Window, cx: &mut App) {
        self.input.update(cx, |s, cx| s.focus(window, cx));
    }

    fn filter(&mut self, cx: &mut Context<Self>) {
        let q = self.input.read(cx).value().trim().to_lowercase();
        if q.is_empty() {
            // Favorites first, then the rest of the tools, then commands.
            let mut idx: Vec<usize> = (0..self.entries.len()).collect();
            idx.sort_by_key(|&i| {
                let e = &self.entries[i];
                let is_tool = matches!(e.action, PaletteAction::Open(_));
                (!e.fav, !is_tool, i)
            });
            self.results = idx;
        } else {
            let mut scored: Vec<(i32, usize)> = self
                .entries
                .iter()
                .enumerate()
                .filter_map(|(i, e)| {
                    let title = fuzzy(&q, &e.title).map(|s| s * 2 + 10);
                    let kw = fuzzy(&q, &e.keywords);
                    let desc = if q.len() >= 3 && e.subtitle.to_lowercase().contains(&q) { Some(4) } else { None };
                    let best = [title, kw, desc].into_iter().flatten().max()?;
                    Some((best + if e.fav { 3 } else { 0 }, i))
                })
                .collect();
            scored.sort_by(|a, b| b.0.cmp(&a.0).then(a.1.cmp(&b.1)));
            self.results = scored.into_iter().map(|(_, i)| i).collect();
        }
        self.selected = 0;
        self.scroll.scroll_to_item(0);
        cx.notify();
    }

    fn move_sel(&mut self, delta: isize, cx: &mut Context<Self>) {
        if self.results.is_empty() {
            return;
        }
        let n = self.results.len() as isize;
        self.selected = ((self.selected as isize + delta).rem_euclid(n)) as usize;
        self.scroll.scroll_to_item(self.selected);
        cx.notify();
    }

    fn confirm(&mut self, cx: &mut Context<Self>) {
        if let Some(&i) = self.results.get(self.selected) {
            cx.emit(PaletteEvent::Run(self.entries[i].action));
        }
    }
}

impl EventEmitter<PaletteEvent> for Palette {}

impl Render for Palette {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let p = Pal::get(cx);
        let scrim = if p.dark { hsla(0., 0., 0., 0.45) } else { hsla(0., 0., 0., 0.22) };
        let ease = ui::ease_fluent();
        let has_query = !self.input.read(cx).value().trim().is_empty();

        let rows = self.results.iter().enumerate().map(|(row, &i)| {
            let e = &self.entries[i];
            let sel = row == self.selected;
            div()
                .id(("pal-row", row))
                .relative()
                .flex()
                .items_center()
                .gap(px(12.))
                .h(px(48.))
                .pl(px(12.))
                .pr(px(14.))
                .rounded(px(6.))
                .cursor_pointer()
                .when(sel, |d| {
                    d.bg(p.subtle2).child(
                        div().absolute().left(px(2.)).top(px(14.)).bottom(px(14.)).w(px(3.)).rounded(px(3.)).bg(p.accent),
                    )
                })
                .when(!sel, |d| d.hover(move |s| s.bg(p.subtle)))
                .on_mouse_move(cx.listener(move |this, _, _, cx| {
                    if this.selected != row {
                        this.selected = row;
                        cx.notify();
                    }
                }))
                .on_click(cx.listener(move |this, _, _, cx| {
                    this.selected = row;
                    this.confirm(cx);
                }))
                .child(
                    div()
                        .size(px(30.))
                        .flex_none()
                        .rounded(px(7.))
                        .flex()
                        .items_center()
                        .justify_center()
                        .bg(if sel { p.accent } else { p.accent_soft })
                        .child(icon(e.icon, 16., if sel { p.accent_text } else { p.accent })),
                )
                .child(
                    div()
                        .flex()
                        .flex_col()
                        .flex_1()
                        .min_w_0()
                        .child(
                            div()
                                .flex()
                                .items_center()
                                .gap(px(6.))
                                .text_size(px(13.5))
                                .font_weight(if sel { FontWeight::SEMIBOLD } else { FontWeight::MEDIUM })
                                .child(e.title.clone())
                                .when(e.fav, |d| d.child(icon("star.fill", 11., p.star))),
                        )
                        .child(
                            div()
                                .text_size(px(12.))
                                .text_color(p.text3)
                                .whitespace_nowrap()
                                .overflow_hidden()
                                .text_ellipsis()
                                .child(e.subtitle.clone()),
                        ),
                )
                .child(
                    div()
                        .flex_none()
                        .text_size(px(11.5))
                        .text_color(p.text3)
                        .child(e.hint.clone())
                        .when(sel, |d| d.child(div().ml(px(8.)).child(icon("enter", 13., p.text3))))
                        .flex()
                        .items_center(),
                )
        });

        let empty = self.results.is_empty();
        let panel = div()
            .id("palette")
            .key_context(CONTEXT)
            .occlude()
            .w(px(640.))
            .flex()
            .flex_col()
            .bg(p.card)
            .border_1()
            .border_color(p.stroke_strong)
            .rounded(px(10.))
            .shadow({
                let mut s = p.shadow();
                s[0].blur_radius = px(48.);
                s[0].offset.y = px(18.);
                s
            })
            .overflow_hidden()
            .on_action(cx.listener(|this, _: &SelectUp, _, cx| this.move_sel(-1, cx)))
            .on_action(cx.listener(|this, _: &SelectDown, _, cx| this.move_sel(1, cx)))
            .on_action(cx.listener(|this, _: &Confirm, _, cx| this.confirm(cx)))
            .on_action(cx.listener(|_, _: &Cancel, _, cx| cx.emit(PaletteEvent::Dismiss)))
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(10.))
                    .h(px(54.))
                    .px(px(16.))
                    .border_b_1()
                    .border_color(p.stroke)
                    .shadow(vec![Pal::underline(p.accent, 2.)])
                    .child(icon("search", 17., p.accent))
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .text_size(px(15.))
                            .child(Input::new(&self.input).appearance(false).text_size(px(15.))),
                    )
                    .child(kbd_el("Esc", &p)),
            )
            .child(
                div()
                    .px(px(14.))
                    .pt(px(10.))
                    .pb(px(4.))
                    .text_size(px(11.5))
                    .font_weight(FontWeight::SEMIBOLD)
                    .text_color(p.text3)
                    .child(if has_query { "Best matches" } else { "Tools & commands" }),
            )
            .child(
                div()
                    .id("pal-list")
                    .track_scroll(&self.scroll)
                    .overflow_y_scroll()
                    .max_h(px(392.))
                    .px(px(6.))
                    .pb(px(6.))
                    .flex()
                    .flex_col()
                    .gap(px(1.))
                    .children(rows)
                    .when(empty, |d| {
                        d.child(
                            div()
                                .py(px(28.))
                                .flex()
                                .justify_center()
                                .text_size(px(13.))
                                .text_color(p.text3)
                                .child("No tools or commands match."),
                        )
                    }),
            )
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(16.))
                    .h(px(38.))
                    .px(px(14.))
                    .border_t_1()
                    .border_color(p.stroke)
                    .bg(p.layer)
                    .text_size(px(12.))
                    .text_color(p.text3)
                    .child(div().flex().items_center().gap(px(6.)).child(kbd_el("↑", &p)).child(kbd_el("↓", &p)).child("Navigate"))
                    .child(div().flex().items_center().gap(px(6.)).child(kbd_el("Enter", &p)).child("Open"))
                    .child(div().flex().items_center().gap(px(6.)).child(kbd_el("Esc", &p)).child("Close"))
                    .child(div().flex_1())
                    .child(crate::logic::plural(self.results.len(), "result")),
            )
            .on_mouse_down(gpui_kit::MouseButton::Left, |_, _, cx| cx.stop_propagation())
            .with_animation("palette-in", Animation::new(Duration::from_millis(180)), move |el, t| {
                let e = ease(t);
                el.opacity(e).mt(px(92. - 8. * (1. - e)))
            });

        deferred(
            div()
                .id("palette-scrim")
                .absolute()
                .inset_0()
                .flex()
                .flex_col()
                .items_center()
                .bg(scrim)
                .line_height(relative(1.3))
                .on_mouse_down(gpui_kit::MouseButton::Left, cx.listener(|_, _, _, cx| cx.emit(PaletteEvent::Dismiss)))
                .child(panel)
                .with_animation("scrim-in", Animation::new(Duration::from_millis(140)), |el, t| el.opacity(t)),
        )
        .with_priority(2)
    }
}

#[cfg(test)]
mod tests {
    use super::fuzzy;

    #[test]
    fn fuzzy_prefers_word_starts() {
        assert!(fuzzy("jwt", "JWT Decoder").unwrap() > fuzzy("jwt", "just waiting too").unwrap_or(-100));
        assert!(fuzzy("b64", "Base64 Text").is_some());
        assert!(fuzzy("xyz", "JSON").is_none());
        assert!(fuzzy("rx", "Regular Expression").is_some());
    }
}
