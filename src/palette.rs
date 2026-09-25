//! The Ctrl+K command palette: fuzzy search over every tool and a few commands.
//! Queries that name a mode ("base64 decode", "unix to date") also offer the tool
//! already switched to it.

use std::ops::Range;
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
use crate::library_view::{LibAction, PaletteCommand, PaletteItem};
use crate::registry::{Mode, TOOLS, VARIANTS, ToolId, Variant, cat, tool};
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
    OpenMode(ToolId, Mode),
    Home,
    Library,
    /// A library item, by id.
    LibraryItem(u64),
    /// An AI library command: new, import, install, copy …
    Lib(LibAction),
    Workflows,
    /// A saved workflow, by its place in the name order; `true` runs it on the clipboard.
    Workflow(usize, bool),
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
    /// Set for a tool-in-a-mode entry, which only shows when the query asks for the mode.
    variant: Option<&'static Variant>,
    /// Ranked first: actions on the library item on show.
    boost: bool,
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

/// A query's words: lowercase runs of letters and digits.
fn words(query: &str) -> Vec<&str> {
    query.split(|c: char| !c.is_alphanumeric()).filter(|w| !w.is_empty()).collect()
}

/// Where the words of `cue` appear in a row in `words`. The last one may still be
/// being typed ("base64 dec"), or carry a suffix ("decoder").
fn find_cue(words: &[&str], cue: &str) -> Option<Range<usize>> {
    let cue: Vec<&str> = cue.split(' ').collect();
    let last = cue.len() - 1;
    (0..=words.len().checked_sub(cue.len())?)
        .find(|&i| {
            cue.iter().enumerate().all(|(j, c)| {
                let w = words[i + j];
                w == *c || (j == last && w.len() >= 3 && (c.starts_with(w) || w.starts_with(c)))
            })
        })
        .map(|i| i..i + cue.len())
}

/// Score for a tool-in-a-mode entry: the query must name the mode, and every
/// other word must fit the tool well.
fn variant_score(words: &[&str], v: &Variant, keywords: &str) -> Option<i32> {
    let cue = v.cues.iter().find_map(|c| find_cue(words, c))?;
    let mut score = 0;
    for (_, w) in words.iter().enumerate().filter(|(i, _)| !cue.contains(i)) {
        score += fuzzy(w, keywords).filter(|&s| s >= w.len() as i32)?;
    }
    // Asking for a mode is the most specific thing a query can do.
    Some(1000 + score)
}

impl Palette {
    pub fn new(
        current: Option<ToolId>,
        library: Vec<PaletteItem>,
        lib_cmds: Vec<PaletteCommand>,
        workflows: Vec<String>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let input = cx.new(|cx| InputState::new(window, cx).placeholder("Search tools, commands and your AI library…"));
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
                variant: None,
                boost: false,
            })
            .collect();
        entries.extend(VARIANTS.iter().map(|v| {
            let t = tool(v.tool);
            Entry {
                action: PaletteAction::OpenMode(v.tool, v.mode),
                title: format!("{} · {}", t.name, v.label).into(),
                subtitle: t.desc.into(),
                icon: t.icon,
                hint: cat(t.cat).label.into(),
                keywords: format!("{} {} {}", t.title, t.keywords, t.key),
                fav: settings.is_fav(t.key),
                variant: Some(v),
                boost: false,
            }
        }));
        entries.extend(library.into_iter().map(|i| Entry {
            action: PaletteAction::LibraryItem(i.id),
            // Names like "code-review" should match a query typed with spaces.
            keywords: format!("{} {} {} {} library", i.title.replace(['-', '_'], " "), i.tags.join(" "), i.kind.label(), i.kind.plural()),
            title: i.title.into(),
            subtitle: i.summary.into(),
            icon: i.kind.icon(),
            hint: format!("Library · {}", i.kind.label()).into(),
            fav: false,
            variant: None,
            boost: false,
        }));
        entries.extend(lib_cmds.into_iter().map(|c| Entry {
            action: PaletteAction::Lib(c.action),
            title: c.title.into(),
            subtitle: c.subtitle.into(),
            icon: c.icon,
            hint: if c.contextual { "This item".into() } else { "Library".into() },
            keywords: c.keywords,
            fav: false,
            variant: None,
            boost: c.contextual,
        }));
        for (i, name) in workflows.into_iter().enumerate() {
            for (run, title, subtitle) in [
                (false, name.clone(), "Open this workflow"),
                (true, format!("Run {name} on clipboard"), "Run this workflow on the clipboard text"),
            ] {
                entries.push(Entry {
                    action: PaletteAction::Workflow(i, run),
                    keywords: format!("{} workflow chain", name.replace(['-', '_'], " ")),
                    title: title.into(),
                    subtitle: subtitle.into(),
                    icon: "flow",
                    hint: "Workflow".into(),
                    fav: false,
                    variant: None,
                    boost: false,
                });
            }
        }
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
                variant: None,
                boost: false,
            })
        };
        cmd(PaletteAction::Home, "All tools", "Browse every tool", "grid", "home browse");
        cmd(PaletteAction::Library, "AI Library", "Skills, prompts, agents and project rules", "library", "ai skills prompts agents rules claude codex cursor");
        cmd(PaletteAction::Workflows, "Workflows", "Chain tools: each step's output feeds the next", "flow", "workflow chain pipeline recipe steps");
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
            // Library items and saved workflows only show once something is typed.
            let mut idx: Vec<usize> = (0..self.entries.len())
                .filter(|&i| {
                    self.entries[i].variant.is_none()
                        && !matches!(self.entries[i].action, PaletteAction::LibraryItem(_) | PaletteAction::Workflow(..))
                })
                .collect();
            idx.sort_by_key(|&i| {
                let e = &self.entries[i];
                let is_tool = matches!(e.action, PaletteAction::Open(_));
                (!e.boost, !e.fav, !is_tool, i)
            });
            self.results = idx;
        } else {
            let words = words(&q);
            let mut scored: Vec<(i32, usize)> = self
                .entries
                .iter()
                .enumerate()
                .filter_map(|(i, e)| {
                    if let Some(v) = e.variant {
                        return variant_score(&words, v, &e.keywords).map(|s| (s + if e.fav { 3 } else { 0 }, i));
                    }
                    let title = fuzzy(&q, &e.title).map(|s| s * 2 + 10);
                    let kw = fuzzy(&q, &e.keywords);
                    let desc = if q.len() >= 3 && e.subtitle.to_lowercase().contains(&q) { Some(4) } else { None };
                    let best = [title, kw, desc].into_iter().flatten().max()?;
                    Some((best + if e.fav { 3 } else { 0 } + if e.boost { 6 } else { 0 }, i))
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
    use super::{find_cue, fuzzy, variant_score, words};
    use crate::registry::{Mode, ToolId, VARIANTS, tool};

    /// The tool-in-a-mode entries a query offers, best first.
    fn modes(query: &str) -> Vec<(ToolId, Mode)> {
        let q = query.to_lowercase();
        let w = words(&q);
        let mut hits: Vec<_> = VARIANTS
            .iter()
            .filter_map(|v| {
                let t = tool(v.tool);
                let kw = format!("{} {} {}", t.title, t.keywords, t.key);
                variant_score(&w, v, &kw).map(|s| (s, (v.tool, v.mode)))
            })
            .collect();
        hits.sort_by(|a, b| b.0.cmp(&a.0));
        hits.into_iter().map(|(_, m)| m).collect()
    }

    #[test]
    fn a_query_naming_a_mode_offers_the_tool_in_that_mode() {
        assert_eq!(modes("base64 decode"), [(ToolId::Base64, Mode::Decode)]);
        assert_eq!(modes("Decode base64"), [(ToolId::Base64, Mode::Decode)]);
        assert_eq!(modes("b64 enc"), [(ToolId::Base64, Mode::Encode)]);
        assert_eq!(modes("url decoder"), [(ToolId::Url, Mode::Decode)]);
        assert_eq!(modes("unix to date"), [(ToolId::Date, Mode::ToDate)]);
        assert_eq!(modes("date to timestamp"), [(ToolId::Date, Mode::ToUnix)]);
        assert_eq!(modes("minify json"), [(ToolId::JsonFmt, Mode::Minify)]);
        assert_eq!(modes("unescape"), [(ToolId::Html, Mode::Decode), (ToolId::Escape, Mode::Decode)]);
        assert_eq!(modes("decode").len(), 5);
    }

    #[test]
    fn no_mode_without_asking_for_one() {
        assert!(modes("base64").is_empty());
        assert!(modes("base64 d").is_empty());
        assert!(modes("jwt decode").is_empty());
        assert!(modes("date").is_empty());
        assert_eq!(find_cue(&["unix", "to", "da"], "to date"), None);
        assert_eq!(find_cue(&["unix", "to", "dat"], "to date"), Some(1..3));
    }

    #[test]
    fn fuzzy_prefers_word_starts() {
        assert!(fuzzy("jwt", "JWT Decoder").unwrap() > fuzzy("jwt", "just waiting too").unwrap_or(-100));
        assert!(fuzzy("b64", "Base64 Text").is_some());
        assert!(fuzzy("xyz", "JSON").is_none());
        assert!(fuzzy("rx", "Regular Expression").is_some());
    }
}
