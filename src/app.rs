//! The SideKit shell: title bar, navigation, home grid, settings and tool pages.

use std::collections::HashMap;
use std::f32::consts::PI;
use std::time::Duration;

use gpui_kit::component::scroll::ScrollableElement;
use gpui_kit::{
    Animation, AnimationExt, AnyElement, AnyView, App, AppContext, Context, Div, Entity,
    FocusHandle, Focusable, FontWeight, InteractiveElement, IntoElement, KeyBinding,
    ParentElement, Render, SharedString, SpringAnimation, Stateful, StatefulInteractiveElement,
    Decorations, MouseButton, MouseDownEvent, Styled, Subscription, Transformation, Window,
    Task, WindowControlArea, actions, div,
    prelude::FluentBuilder, px, radians, relative, size,
};

use crate::hotkey::{self, Hotkey};
use crate::icons::icon;
use crate::id;
use crate::library_view::{LibraryEvent, LibraryView};
use crate::palette::{Palette, PaletteAction, PaletteEvent};
use crate::registry::{CATS, Cat, TOOLS, Tool, ToolId, cat, tool};
use crate::settings::Settings;
use crate::theme::{self, Pal};
use crate::tools;
use crate::tray::{self, Tray, TrayMsg};
use crate::ui::{self, BtnKind, mix, spring_soft};

actions!(sidekit, [OpenPalette, FocusSearch, GoBack, ToggleTheme, OpenSettings]);

pub fn bind_keys(cx: &mut App) {
    cx.bind_keys([
        KeyBinding::new("ctrl-k", OpenPalette, None),
        KeyBinding::new("cmd-k", OpenPalette, None),
        KeyBinding::new("ctrl-p", OpenPalette, None),
        KeyBinding::new("ctrl-f", FocusSearch, Some("Shell")),
        KeyBinding::new("alt-left", GoBack, None),
        KeyBinding::new("ctrl-shift-t", ToggleTheme, None),
        KeyBinding::new("ctrl-,", OpenSettings, None),
    ]);
}

pub const APP_NAME: &str = "SideKit";

#[derive(Clone, Copy, PartialEq, Eq)]
enum Cap {
    Min,
    Max,
    Close,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum View {
    Home,
    Library,
    Settings,
    Tool(ToolId),
}

struct Suggestion {
    tool: ToolId,
    what: &'static str,
    text: String,
}

/// A place to come back to: a page, and for the library, the item on show.
type Stop = (View, Option<u64>);

pub struct SideKit {
    view: View,
    history: Vec<Stop>,
    home_cat: Option<Cat>,
    /// Explicit open/closed choices for nav groups, keyed by group.
    open_groups: HashMap<&'static str, bool>,
    group_gen: HashMap<&'static str, usize>,
    tool_views: HashMap<ToolId, AnyView>,
    library: Entity<LibraryView>,
    hovered_card: Option<ToolId>,
    palette: Option<Entity<Palette>>,
    focus: FocusHandle,
    /// Smart detection: what the clipboard looks like, if a tool fits it.
    suggestion: Option<Suggestion>,
    /// Clipboard text the user already acted on or dismissed.
    seen_clipboard: Option<String>,
    /// Bumped on every navigation so entry animations replay.
    epoch: usize,
    hotkey: Hotkey,
    tray: Tray,
    _hotkey_task: Task<()>,
    _tray_task: Task<()>,
    _subs: Vec<Subscription>,
}

impl SideKit {
    pub fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let focus = cx.focus_handle();
        window.focus(&focus, cx);
        let activation = cx.observe_window_activation(window, |this, window, cx| {
            if window.is_window_active() {
                this.check_clipboard(cx);
            }
        });
        // "System" theme: follow the OS when it switches between light and dark.
        let appearance = cx.observe_window_appearance(window, |this, window, cx| {
            if Settings::get(cx).follow_system {
                let dark = theme::system_dark(cx);
                if dark != Settings::get(cx).dark {
                    this.apply_theme(dark, window, cx);
                }
            }
        });
        let library = cx.new(|cx| LibraryView::new(window, cx));
        // Moving between library items is navigation too, so Back can retrace it.
        let lib_nav = cx.subscribe(&library, |this, _, ev: &LibraryEvent, cx| match ev {
            LibraryEvent::Moved { from } => {
                if this.view == View::Library {
                    this.push_history((View::Library, Some(*from)));
                    cx.notify();
                }
            }
        });
        let (mut hotkey, presses) = Hotkey::new();
        hotkey.set(Settings::get(cx).hotkey);
        let hotkey_task = cx.spawn_in(window, async move |this, cx| {
            while presses.recv().await.is_ok() {
                if this.update_in(cx, |this, window, cx| this.summon(window, cx)).is_err() {
                    break;
                }
            }
        });
        let mut tray = Tray::default();
        tray.set(tray::SUPPORTED && Settings::get(cx).tray);
        let tray_events = tray::events();
        let tray_task = cx.spawn_in(window, async move |this, cx| {
            while let Ok(msg) = tray_events.recv().await {
                let done = this.update_in(cx, |_, window, cx| match msg {
                    TrayMsg::Open => tray::show(window, cx),
                    TrayMsg::Quit => cx.quit(),
                });
                if done.is_err() {
                    break;
                }
            }
        });
        // With the tray on, closing only hides the window so the shortcut keeps working.
        window.on_window_should_close(cx, |window, cx| {
            if tray::SUPPORTED && Settings::get(cx).tray {
                tray::hide(window, cx);
                false
            } else {
                true
            }
        });
        let mut this = Self {
            view: View::Home,
            history: Vec::new(),
            home_cat: None,
            open_groups: HashMap::new(),
            group_gen: HashMap::new(),
            tool_views: HashMap::new(),
            library,
            hovered_card: None,
            palette: None,
            suggestion: None,
            seen_clipboard: None,
            focus,
            epoch: 0,
            hotkey,
            tray,
            _hotkey_task: hotkey_task,
            _tray_task: tray_task,
            _subs: vec![activation, lib_nav, appearance],
        };
        this.check_clipboard(cx);
        this
    }

    /// Look at the clipboard and offer the matching tool (Settings → Smart detection).
    fn check_clipboard(&mut self, cx: &mut Context<Self>) {
        let before = self.suggestion.as_ref().map(|s| s.tool);
        self.suggestion = None;
        if Settings::get(cx).smart {
            if let Some(text) = cx.read_from_clipboard().and_then(|c| c.text()) {
                if self.seen_clipboard.as_deref() != Some(text.as_str()) {
                    if let Some((tool, what)) = crate::logic::detect(&text) {
                        self.suggestion = Some(Suggestion { tool, what, text });
                    }
                }
            }
        }
        if before != self.suggestion.as_ref().map(|s| s.tool) {
            cx.notify();
        }
    }

    fn accept_suggestion(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(s) = self.suggestion.take() else { return };
        self.seen_clipboard = Some(s.text.clone());
        self.go(View::Tool(s.tool), window, cx);
        if let Some(view) = self.tool_views.get(&s.tool).cloned() {
            tools::fill(s.tool, &view, &s.text, window, cx);
        }
        cx.notify();
    }

    /// Global hotkey: come forward with the clipboard's tool open, or the
    /// palette when nothing fits. Pressed again while in front, step aside.
    fn summon(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if window.is_window_active() {
            if tray::SUPPORTED && Settings::get(cx).tray {
                tray::hide(window, cx);
            } else {
                window.minimize_window();
            }
            return;
        }
        tray::show(window, cx);
        self.check_clipboard(cx);
        if self.suggestion.is_some() {
            self.accept_suggestion(window, cx);
        } else {
            self.open_palette(window, cx);
        }
    }

    fn dismiss_suggestion(&mut self, cx: &mut Context<Self>) {
        if let Some(s) = self.suggestion.take() {
            self.seen_clipboard = Some(s.text);
        }
        cx.notify();
    }

    // ------------------------------------------------------------ navigation

    pub fn go(&mut self, v: View, window: &mut Window, cx: &mut Context<Self>) {
        if v == self.view {
            return;
        }
        if let View::Tool(id) = v {
            self.ensure_tool(id, window, cx);
        }
        let stop = (self.view, if self.view == View::Library { self.library.read(cx).selected() } else { None });
        self.push_history(stop);
        self.view = v;
        self.epoch += 1;
        self.hovered_card = None;
        cx.notify();
    }

    fn push_history(&mut self, stop: Stop) {
        if self.history.last() != Some(&stop) {
            self.history.push(stop);
        }
        if self.history.len() > 50 {
            self.history.remove(0);
        }
    }

    fn back(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        while let Some((view, item)) = self.history.pop() {
            // Items deleted since are skipped, and so are stops that would change nothing.
            if let Some(id) = item {
                if !self.library.update(cx, |v, cx| v.restore(id, window, cx)) {
                    continue;
                }
            } else if view == self.view {
                continue;
            }
            if view != self.view {
                self.view = view;
                self.epoch += 1;
            }
            cx.notify();
            return;
        }
    }

    fn ensure_tool(&mut self, id: ToolId, window: &mut Window, cx: &mut Context<Self>) {
        if !self.tool_views.contains_key(&id) {
            let view = tools::create(id, window, cx);
            self.tool_views.insert(id, view);
        }
    }

    /// The title-bar switch: an explicit choice, so it stops following the system.
    fn toggle_theme(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let dark = !Settings::get(cx).dark;
        self.set_theme_mode(if dark { 1 } else { 0 }, window, cx);
    }

    /// 0 = light, 1 = dark, 2 = follow the system.
    fn set_theme_mode(&mut self, mode: usize, window: &mut Window, cx: &mut Context<Self>) {
        let follow = mode == 2;
        let dark = if follow { theme::system_dark(cx) } else { mode == 1 };
        Settings::update(cx, |s| s.follow_system = follow);
        self.apply_theme(dark, window, cx);
    }

    fn apply_theme(&mut self, dark: bool, window: &mut Window, cx: &mut Context<Self>) {
        Settings::update(cx, |s| s.dark = dark);
        theme::apply(dark, Some(window), cx);
        cx.notify();
    }

    fn toggle_fav(&mut self, key: &'static str, cx: &mut Context<Self>) {
        Settings::update(cx, |s| {
            if let Some(i) = s.favorites.iter().position(|f| f == key) {
                s.favorites.remove(i);
            } else {
                s.favorites.push(key.to_string());
            }
        });
        cx.notify();
    }

    fn favs(&self, cx: &App) -> Vec<&'static Tool> {
        let s = Settings::get(cx);
        TOOLS.iter().filter(|t| s.is_fav(t.key)).collect()
    }

    // ------------------------------------------------------------ palette

    fn open_palette(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(p) = &self.palette {
            p.update(cx, |p, cx| p.focus(window, cx));
            return;
        }
        let current = match self.view {
            View::Tool(id) => Some(id),
            _ => None,
        };
        let lib = self.library.read(cx);
        let (library, lib_cmds) = (lib.palette_items(), lib.palette_commands(self.view == View::Library));
        let palette = cx.new(|cx| Palette::new(current, library, lib_cmds, window, cx));
        let sub = cx.subscribe_in(&palette, window, |this, _, ev: &PaletteEvent, window, cx| {
            this.palette = None;
            window.focus(&this.focus, cx);
            if let PaletteEvent::Run(action) = ev {
                this.run(*action, window, cx);
            }
            cx.notify();
        });
        self._subs.push(sub);
        self.palette = Some(palette);
        cx.notify();
    }

    fn run(&mut self, action: PaletteAction, window: &mut Window, cx: &mut Context<Self>) {
        match action {
            PaletteAction::Open(id) => self.go(View::Tool(id), window, cx),
            PaletteAction::OpenMode(id, mode) => {
                self.go(View::Tool(id), window, cx);
                if let Some(view) = self.tool_views.get(&id) {
                    tools::set_mode(view, mode, window, cx);
                }
            }
            PaletteAction::Home => self.go(View::Home, window, cx),
            PaletteAction::Library => self.go(View::Library, window, cx),
            PaletteAction::LibraryItem(id) => {
                self.go(View::Library, window, cx);
                self.library.update(cx, |v, cx| v.open_item(id, window, cx));
            }
            PaletteAction::Lib(action) => {
                self.go(View::Library, window, cx);
                self.library.update(cx, |v, cx| v.run(action, window, cx));
            }
            PaletteAction::Settings => self.go(View::Settings, window, cx),
            PaletteAction::ToggleTheme => self.toggle_theme(window, cx),
            PaletteAction::ToggleFavorite(id) => self.toggle_fav(tool(id).key, cx),
            PaletteAction::Back => self.back(window, cx),
            PaletteAction::ToggleWrap => {
                Settings::update(cx, |s| s.wrap = !s.wrap);
                cx.notify();
            }
        }
    }

    // ------------------------------------------------------------ title bar

    fn render_title_bar(&mut self, window: &mut Window, cx: &mut Context<Self>) -> AnyElement {
        let pal = Pal::get(cx);
        let p = pal;
        let no_back = self.history.is_empty();
        let dark = pal.dark;

        let tb_btn = |id: &'static str| {
            div()
                .id(id)
                .flex()
                .flex_none()
                .items_center()
                .justify_center()
                .w(px(40.))
                .h(px(32.))
                .rounded(px(6.))
        };

        let search = div()
            .id("open-palette")
            .occlude()
            .flex()
            .items_center()
            .gap(px(8.))
            .w(px(420.))
            .max_w_full()
            .h(px(34.))
            .pl(px(12.))
            .pr(px(6.))
            .rounded(px(6.))
            .bg(p.card)
            .border_1()
            .border_color(p.stroke)
            .shadow(vec![Pal::underline(p.stroke_strong, 1.)])
            .cursor_pointer()
            .hover(move |s| s.border_color(p.stroke_strong).bg(p.card_hover))
            .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
            .on_click(cx.listener(|this, _, window, cx| this.open_palette(window, cx)))
            .child(icon("search", 15., p.text3))
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .text_size(px(13.))
                    .text_color(p.text3)
                    .overflow_hidden()
                    .whitespace_nowrap()
                    .text_ellipsis()
                    .child("Search tools and commands…"),
            )
            .child(kbd(if cfg!(target_os = "macos") { "⌘ K" } else { "Ctrl K" }, &p));

        // Window chrome differs per platform:
        // - Windows: our caption buttons map to native hit-test areas (keeps Snap Layouts).
        // - Linux (client-side decorations): buttons call the window APIs, the bar drags.
        // - macOS: the system draws traffic lights on the left; we leave room for them.
        let is_windows = cfg!(target_os = "windows");
        let is_mac = cfg!(target_os = "macos");
        let client_decorated = matches!(window.window_decorations(), Decorations::Client { .. });
        let show_caps = is_windows || (!is_mac && client_decorated);
        let controls = window.window_controls();
        let maximized = window.is_maximized();

        let cap = |id: &'static str, kind: Cap, name: &'static str, sz: f32| {
            let close = kind == Cap::Close;
            let area = match kind {
                Cap::Min => WindowControlArea::Min,
                Cap::Max => WindowControlArea::Max,
                Cap::Close => WindowControlArea::Close,
            };
            div()
                .id(id)
                .occlude()
                .flex()
                .items_center()
                .justify_center()
                .w(px(46.))
                .h(px(32.))
                .when(close, |d| d.rounded_tr(px(8.)))
                .hover(move |s| {
                    if close { s.bg(gpui_kit::rgb(0xc42b1c)) } else { s.bg(p.subtle2) }
                })
                .active(move |s| {
                    if close { s.bg(gpui_kit::rgb(0xb22a1b)) } else { s.bg(p.stroke_strong) }
                })
                .group(id)
                .when(is_windows, |d| d.window_control_area(area))
                .when(!is_windows, |d| {
                    d.cursor_pointer()
                        .on_mouse_down(MouseButton::Left, |_, window, cx| {
                            window.prevent_default();
                            cx.stop_propagation();
                        })
                        .on_click(move |_, window, cx| {
                            cx.stop_propagation();
                            match kind {
                                Cap::Min => window.minimize_window(),
                                Cap::Max => window.zoom_window(),
                                Cap::Close => window.remove_window(),
                            }
                        })
                })
                .child(
                    icon(name, sz, p.text2).group_hover(id, move |s| {
                        s.text_color(if close { gpui_kit::white() } else { p.text })
                    }),
                )
        };

        div()
            .id("title-bar")
            .flex()
            .flex_none()
            .items_center()
            .gap(px(8.))
            .h(px(48.))
            .pl(px(if is_mac { 78. } else { 8. }))
            .when(!show_caps, |d| d.pr(px(8.)))
            .window_control_area(WindowControlArea::Drag)
            // Outside Windows the app moves the window itself; interactive children
            // occlude this hitbox, so only empty title-bar space starts a drag.
            .when(!is_windows, |d| {
                d.on_mouse_down(MouseButton::Left, move |ev: &MouseDownEvent, window, _| {
                    if ev.click_count >= 2 {
                        if is_mac { window.titlebar_double_click() } else { window.zoom_window() }
                    } else {
                        window.start_window_move();
                    }
                })
                .when(client_decorated && controls.window_menu, |d| {
                    d.on_mouse_down(MouseButton::Right, |ev: &MouseDownEvent, window, _| {
                        window.show_window_menu(ev.position)
                    })
                })
            })
            .child(
                tb_btn("back")
                    .occlude()
                    .when(no_back, |d| d.opacity(0.35))
                    .when(!no_back, |d| {
                        d.cursor_pointer()
                            .hover(move |s| s.bg(p.subtle))
                            .on_click(cx.listener(|this, _, window, cx| this.back(window, cx)))
                    })
                    .child(icon("back", 16., p.text2)),
            )
            .child(
                div()
                    .flex()
                    .flex_none()
                    .items_center()
                    .gap(px(10.))
                    .w(px(216.))
                    .child(
                        div()
                            .size(px(22.))
                            .rounded(px(6.))
                            .bg(p.accent)
                            .flex()
                            .items_center()
                            .justify_center()
                            .child(icon("logo@2.4", 14., p.accent_text)),
                    )
                    .child(div().text_size(px(12.5)).font_weight(FontWeight::SEMIBOLD).child(APP_NAME))
                    .child(ui::badge("Offline", ui::Tone::Neutral, &p).h(px(18.)).text_size(px(10.5))),
            )
            .child(div().flex_1().min_w_0().flex().justify_center().child(search))
            .child(
                tb_btn("theme")
                    .occlude()
                    .cursor_pointer()
                    .hover(move |s| s.bg(p.subtle))
                    .active(move |s| s.bg(p.subtle2))
                    .on_click(cx.listener(|this, _, window, cx| this.toggle_theme(window, cx)))
                    .child(icon(if dark { "sun" } else { "moon" }, 16., p.text2)),
            )
            .when(show_caps, |d| {
                d.child(
                    div()
                        .flex()
                        .self_start()
                        .when(controls.minimize || is_windows, |d| d.child(cap("cap-min", Cap::Min, "min", 12.)))
                        .when(controls.maximize || is_windows, |d| {
                            d.child(cap("cap-max", Cap::Max, if maximized { "restore" } else { "max" }, 11.))
                        })
                        .child(cap("cap-close", Cap::Close, "close", 12.)),
                )
            })
            .into_any_element()
    }

    // ------------------------------------------------------------ navigation pane

    fn nav_item(
        &self,
        id: impl Into<gpui_kit::ElementId>,
        icon_name: &str,
        label: impl Into<SharedString>,
        selected: bool,
        child: bool,
        pal: &Pal,
    ) -> Stateful<Div> {
        let p = *pal;
        let id: gpui_kit::ElementId = id.into();
        let pill_left = if child { 30. } else { 2. };
        div()
            .id(id.clone())
            .relative()
            .flex()
            .flex_none()
            .items_center()
            .gap(px(12.))
            .w_full()
            .h(px(if child { 34. } else { 36. }))
            .pl(px(if child { 42. } else { 14. }))
            .pr(px(10.))
            .rounded(px(6.))
            .text_size(px(if child { 13. } else { 13.5 }))
            .cursor_pointer()
            .text_color(p.text)
            .when(selected, |d| d.bg(p.subtle2).font_weight(FontWeight::SEMIBOLD))
            .when(!selected, |d| d.hover(move |s| s.bg(p.subtle)))
            .active(move |s| s.bg(p.subtle2))
            .when(selected, |d| {
                let ease = ui::ease_fluent();
                d.child(
                    div()
                        .absolute()
                        .left(px(pill_left))
                        .w(px(3.))
                        .rounded(px(3.))
                        .bg(p.accent)
                        .with_animation(
                            ui::child(&id, self.epoch),
                            Animation::new(Duration::from_millis(300)),
                            move |el, t| {
                                let e = ease(t);
                                let inset = 9. + 9. * (1. - e);
                                el.top(px(inset)).bottom(px(inset)).opacity(e)
                            },
                        ),
                )
            })
            .child(icon(icon_name, if child { 16. } else { 18. }, p.text))
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .overflow_hidden()
                    .whitespace_nowrap()
                    .text_ellipsis()
                    .child(label.into()),
            )
    }

    fn render_nav(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> AnyElement {
        let pal = Pal::get(cx);
        let current_tool = match self.view {
            View::Tool(id) => Some(tool(id)),
            _ => None,
        };

        struct Group {
            key: &'static str,
            label: &'static str,
            icon: &'static str,
            divider: bool,
            open: bool,
            items: Vec<&'static Tool>,
        }
        let mut groups: Vec<Group> = Vec::new();
        let favs = self.favs(cx);
        if !favs.is_empty() {
            let open = *self.open_groups.get("fav").unwrap_or(&true);
            groups.push(Group { key: "fav", label: "Favorites", icon: "star", divider: false, open, items: favs });
        }
        for (i, c) in CATS.iter().enumerate() {
            let items: Vec<&'static Tool> = TOOLS.iter().filter(|t| t.cat == c.id).collect();
            if items.is_empty() {
                continue;
            }
            let default_open = match current_tool {
                Some(t) => t.cat == c.id,
                None => c.id == Cat::Conv,
            };
            let open = *self.open_groups.get(c.key).unwrap_or(&default_open);
            groups.push(Group { key: c.key, label: c.label, icon: c.icon, divider: i == 0 && !groups.is_empty(), open, items });
        }

        let mut list = div()
            .id("nav-list")
            .flex()
            .flex_col()
            .gap(px(2.))
            .flex_1()
            .min_h_0()
            .overflow_y_scroll()
            .pr(px(2.))
            .child(
                self.nav_item("nav-home", "grid", "All tools", self.view == View::Home, false, &pal)
                    .child(
                        div()
                            .text_size(px(11.))
                            .text_color(pal.text3)
                            .px(px(7.))
                            .py(px(1.))
                            .rounded(px(10.))
                            .bg(pal.subtle)
                            .font_weight(FontWeight::NORMAL)
                            .child(TOOLS.len().to_string()),
                    )
                    .on_click(cx.listener(|this, _, window, cx| this.go(View::Home, window, cx))),
            )
            .child(
                self.nav_item("nav-library", "library", "AI Library", self.view == View::Library, false, &pal)
                    .child(
                        div()
                            .text_size(px(11.))
                            .text_color(pal.text3)
                            .px(px(7.))
                            .py(px(1.))
                            .rounded(px(10.))
                            .bg(pal.subtle)
                            .font_weight(FontWeight::NORMAL)
                            .child(self.library.read(cx).count().to_string()),
                    )
                    .on_click(cx.listener(|this, _, window, cx| this.go(View::Library, window, cx))),
            );

        for g in groups {
            let key = g.key;
            let open = g.open;
            let epoch = *self.group_gen.get(key).unwrap_or(&0);
            let p = pal;
            let header = self
                .nav_item(id!("grp-{key}"), g.icon, g.label, false, false, &pal)
                .child(
                    icon("chev-down", 14., p.text3).with_spring(
                        id!("chev-{key}"),
                        SpringAnimation::new(spring_soft()).to(open),
                        |svg, ph| svg.with_transformation(Transformation::rotate(radians(PI * ph.0))),
                    ),
                )
                .on_click(cx.listener(move |this, _, _, cx| {
                    this.open_groups.insert(key, !open);
                    *this.group_gen.entry(key).or_default() += 1;
                    cx.notify();
                }));
            let mut col = div().flex().flex_col().gap(px(2.));
            if g.divider {
                col = col.child(ui::divider(&pal));
            }
            col = col.child(header);
            if open {
                let ease = ui::ease_fluent();
                let items = div()
                    .flex()
                    .flex_col()
                    .gap(px(2.))
                    .children(g.items.iter().map(|t| {
                        let id = t.id;
                        self.nav_item(id!("{key}-{}", t.key), t.icon, t.name, self.view == View::Tool(t.id), true, &pal)
                            .on_click(cx.listener(move |this, _, window, cx| this.go(View::Tool(id), window, cx)))
                    }))
                    .relative()
                    .with_animation(id!("grp-items-{key}-{epoch}"), Animation::new(Duration::from_millis(220)), move |el, t| {
                        let e = ease(t);
                        el.opacity(e).top(px(-4. * (1. - e)))
                    });
                col = col.child(items);
            }
            list = list.child(col);
        }

        div()
            .w(px(280.))
            .flex_none()
            .flex()
            .flex_col()
            .pt(px(4.))
            .px(px(6.))
            .pb(px(8.))
            .child(list)
            .child(ui::divider(&pal))
            .child(
                self.nav_item("nav-settings", "gear", "Settings", self.view == View::Settings, false, &pal)
                    .on_click(cx.listener(|this, _, window, cx| this.go(View::Settings, window, cx))),
            )
            .into_any_element()
    }

    // ------------------------------------------------------------ home

    fn tool_card(&self, t: &'static Tool, fav: bool, cx: &mut Context<Self>) -> AnyElement {
        let p = Pal::get(cx);
        let id = t.id;
        let hovered = self.hovered_card == Some(id);
        div()
            .id(id!("card-{}", t.key))
            .relative()
            .flex()
            .flex_col()
            .items_start()
            .gap(px(12.))
            .h(px(168.))
            .p(px(18.))
            .rounded(px(8.))
            .border_1()
            .cursor_pointer()
            .on_hover(cx.listener(move |this, h: &bool, _, cx| {
                if *h {
                    this.hovered_card = Some(id);
                } else if this.hovered_card == Some(id) {
                    this.hovered_card = None;
                }
                cx.notify();
            }))
            .on_click(cx.listener(move |this, _, window, cx| this.go(View::Tool(id), window, cx)))
            .child(
                div()
                    .size(px(44.))
                    .rounded(px(10.))
                    .flex()
                    .items_center()
                    .justify_center()
                    .child(icon(t.icon, 22., p.accent).with_spring(
                        id!("card-ic-{}", t.key),
                        SpringAnimation::new(ui::spring_bouncy()).to(hovered),
                        move |svg, ph| {
                            let s = ph.interpolate(1.0f32, 1.08);
                            svg.text_color(mix(p.accent, p.accent_text, ph.0.clamp(0., 1.)))
                                .with_transformation(
                                    Transformation::scale(size(s, s)).with_rotation(radians(ph.interpolate(0f32, -0.1047))),
                                )
                        },
                    ))
                    .with_spring(id!("card-tile-{}", t.key), SpringAnimation::new(spring_soft()).to(hovered), move |el, ph| {
                        el.bg(mix(p.accent_soft, p.accent, ph.0.clamp(0., 1.)))
                    }),
            )
            .when(fav, |d| {
                d.child(div().absolute().top(px(16.)).right(px(16.)).child(icon("star.fill", 14., p.star)))
            })
            .child(
                div()
                    .flex()
                    .flex_col()
                    .gap(px(4.))
                    .child(div().text_size(px(14.)).font_weight(FontWeight::SEMIBOLD).child(t.name))
                    .child(div().text_size(px(12.5)).line_height(relative(1.45)).text_color(p.text3).child(t.desc)),
            )
            .child(
                div()
                    .absolute()
                    .bottom(px(16.))
                    .child(icon("arrow", 16., p.accent))
                    .with_spring(id!("card-go-{}", t.key), SpringAnimation::new(spring_soft()).to(hovered), |el, ph| {
                        let v = ph.0.clamp(0., 1.);
                        el.opacity(v).right(px(16. + 6. * (1. - v)))
                    }),
            )
            .with_spring(id!("card-lift-{}", t.key), SpringAnimation::new(spring_soft()).to(hovered), move |el, ph| {
                let v = ph.0.clamp(0., 1.2);
                let mut shadow = p.shadow();
                for s in &mut shadow {
                    s.color.a *= v.min(1.);
                }
                el.top(px(-3. * v))
                    .bg(p.card)
                    .border_color(mix(p.stroke, p.stroke_strong, v.min(1.)))
                    .shadow(shadow)
            })
            .into_any_element()
    }

    fn render_home(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> AnyElement {
        let pal = Pal::get(cx);
        let p = pal;
        let favs = self.favs(cx);
        let home_tools: Vec<&'static Tool> = TOOLS
            .iter()
            .filter(|t| self.home_cat.is_none_or(|c| t.cat == c))
            .collect();
        let epoch = self.epoch;
        let settings = Settings::get(cx).clone();

        let suggestion = self
            .suggestion
            .as_ref()
            .filter(|_| settings.smart)
            .map(|s| {
                let t = tool(s.tool);
                (if t.title.contains(" / ") { t.name } else { t.title }, s.what)
            });
        let cards: Vec<AnyElement> = home_tools.iter().map(|t| self.tool_card(t, settings.is_fav(t.key), cx)).collect();
        let mut chips: Vec<(Option<Cat>, &'static str)> = vec![(None, "All")];
        chips.extend(CATS.iter().map(|c| (Some(c.id), c.label)));

        div()
            .id("home")
            .flex_1()
            .overflow_y_scrollbar()
            .pt(px(30.))
            .px(px(36.))
            .pb(px(36.))
            .flex()
            .flex_col()
            .gap(px(22.))
            .child(ui::enter(
                ("home-h", epoch),
                0,
                div()
                    .flex()
                    .items_end()
                    .justify_between()
                    .gap(px(24.))
                    .child(
                        div()
                            .flex()
                            .flex_col()
                            .gap(px(6.))
                            .child(div().text_size(px(28.)).font_weight(FontWeight::SEMIBOLD).child("All tools"))
                            .child(
                                div()
                                    .text_size(px(13.5))
                                    .text_color(p.text2)
                                    .child("A Swiss Army knife for developers. Every tool runs locally, nothing leaves your machine."),
                            ),
                    ),
            ))
            .when_some(suggestion, |d, (tool_name, what)| {
                d.child(ui::enter(
                    ("home-s", epoch),
                    30,
                    div()
                        .id("smart")
                        .flex()
                        .items_center()
                        .gap(px(12.))
                        .pl(px(16.))
                        .pr(px(8.))
                        .py(px(8.))
                        .rounded(px(8.))
                        .bg(p.accent_soft)
                        .border_1()
                        .border_color(p.stroke)
                        .child(icon("sparkle", 18., p.accent))
                        .child(
                            div()
                                .flex_1()
                                .text_size(px(13.))
                                .child(format!("Your clipboard looks like {what}.")),
                        )
                        .child(ui::btn(
                            "smart-open",
                            Some("arrow"),
                            format!("Open {tool_name}"),
                            BtnKind::Accent,
                            &p,
                            cx.listener(|this, _, window, cx| this.accept_suggestion(window, cx)),
                        ))
                        .child(ui::icon_btn("smart-x", "x", &p, cx.listener(|this, _, _, cx| this.dismiss_suggestion(cx)))),
                ))
            })
            .when(!favs.is_empty(), |d| {
                d.child(ui::enter(
                    ("home-f", epoch),
                    50,
                    div()
                        .flex()
                        .flex_col()
                        .gap(px(10.))
                        .child(ui::section_label("Favorites", &p).m_0())
                        .child(div().flex().flex_wrap().gap(px(8.)).children(favs.iter().map(|t| {
                            let id = t.id;
                            div()
                                .id(id!("fav-{}", t.key))
                                .flex()
                                .items_center()
                                .gap(px(8.))
                                .h(px(34.))
                                .pl(px(10.))
                                .pr(px(14.))
                                .rounded(px(17.))
                                .border_1()
                                .border_color(p.stroke)
                                .bg(p.card)
                                .text_size(px(13.))
                                .cursor_pointer()
                                .hover(move |s| s.border_color(p.accent).shadow(vec![Pal::ring(p.accent_soft, 3.)]))
                                .on_click(cx.listener(move |this, _, window, cx| this.go(View::Tool(id), window, cx)))
                                .child(icon(t.icon, 16., p.accent))
                                .child(t.name)
                        }))),
                ))
            })
            .child(ui::enter(
                ("home-c", epoch),
                50,
                div().flex().flex_wrap().gap(px(8.)).children(chips.into_iter().map(|(c, label)| {
                    let sel = self.home_cat == c;
                    div()
                        .id(id!("chip-{label}"))
                        .flex()
                        .items_center()
                        .h(px(30.))
                        .px(px(14.))
                        .rounded(px(15.))
                        .border_1()
                        .text_size(px(12.5))
                        .cursor_pointer()
                        .when(sel, |d| {
                            d.bg(p.accent).border_color(p.accent).text_color(p.accent_text).font_weight(FontWeight::SEMIBOLD)
                        })
                        .when(!sel, |d| {
                            d.border_color(p.stroke_strong)
                                .text_color(p.text2)
                                .hover(move |s| s.bg(p.subtle).text_color(p.text))
                        })
                        .on_click(cx.listener(move |this, _, _, cx| {
                            this.home_cat = c;
                            cx.notify();
                        }))
                        .child(label)
                })),
            ))
            .child(ui::enter(
                ("home-g", epoch),
                100,
                div()
                    .grid()
                    .grid_cols(4)
                    .gap(px(12.))
                    .children(cards),
            ))
            .when(home_tools.is_empty(), |d| {
                d.child(ui::enter(
                    ("home-e", epoch),
                    0,
                    div().p(px(48.)).flex().justify_center().text_size(px(14.)).text_color(p.text3).child("No tools in this category."),
                ))
            })
            .into_any_element()
    }

    // ------------------------------------------------------------ settings

    fn render_settings(&mut self, window: &mut Window, cx: &mut Context<Self>) -> AnyElement {
        let pal = Pal::get(cx);
        let s = Settings::get(cx).clone();
        let epoch = self.epoch;
        let fav_n = self.favs(cx).len();
        let fav_text: SharedString = if fav_n > 0 {
            format!("{} pinned to the navigation", crate::logic::plural(fav_n, "tool")).into()
        } else {
            "No favorite tools yet".into()
        };
        const SIZES: &[&str] = &["12 px", "13 px", "14 px", "15 px"];
        let on_font = tools::on_index(cx, |_, i, _, cx| {
            Settings::update(cx, |s| s.font_size = 12 + i as u8);
            cx.notify();
        });

        div()
            .id("settings")
            .flex_1()
            .overflow_y_scrollbar()
            .child(
                div()
                    .pt(px(30.))
                    .px(px(36.))
                    .pb(px(36.))
                    .flex()
                    .flex_col()
                    .gap(px(8.))
                    .max_w(px(860.))
                    .child(ui::enter(
                        ("set-h", epoch),
                        0,
                        div().mb(px(14.)).text_size(px(28.)).font_weight(FontWeight::SEMIBOLD).child("Settings"),
                    ))
                    .child(ui::enter(
                        ("set-a", epoch),
                        50,
                        div()
                            .flex()
                            .flex_col()
                            .gap(px(4.))
                            .child(ui::section_label("Appearance", &pal))
                            .child(ui::setting(
                                "s-theme",
                                ui::setting_icon("contrast", &pal),
                                "App theme",
                                Some("Select which app theme to display".into()),
                                ui::seg("s-theme-seg", &["Light", "Dark", "System"], if s.follow_system { 2 } else { s.dark as usize }, &pal, tools::on_index(cx, |this, i, w, cx| {
                                    this.set_theme_mode(i, w, cx)
                                })),
                                &pal,
                            ))
                            .child(ui::setting(
                                "s-font",
                                ui::setting_icon("fontsize", &pal),
                                "Editor font size",
                                Some("Text size used in every input and output editor".into()),
                                ui::dropdown(
                                    "s-font-dd",
                                    SIZES,
                                    (s.font_size.clamp(12, 15) - 12) as usize,
                                    &pal,
                                    window,
                                    cx,
                                    on_font,
                                ),
                                &pal,
                            )),
                    ))
                    .child(ui::enter(
                        ("set-b", epoch),
                        100,
                        div()
                            .mt(px(12.))
                            .flex()
                            .flex_col()
                            .gap(px(4.))
                            .child(ui::section_label("Behavior", &pal))
                            .child(ui::setting(
                                "s-wrap",
                                ui::setting_icon("wrap", &pal),
                                "Wrap long lines",
                                Some("Soft-wrap text in editors instead of scrolling horizontally".into()),
                                ui::toggle_labeled("s-wrap-tg", s.wrap, &pal, cx.listener(|_, _, _, cx| {
                                    Settings::update(cx, |s| s.wrap = !s.wrap);
                                    cx.notify();
                                })),
                                &pal,
                            ))
                            .child(ui::setting(
                                "s-smart",
                                ui::setting_icon("sparkle", &pal),
                                "Smart detection",
                                Some("Suggest the right tool based on what is on your clipboard".into()),
                                ui::toggle_labeled("s-smart-tg", s.smart, &pal, cx.listener(|this, _, _, cx| {
                                    Settings::update(cx, |s| s.smart = !s.smart);
                                    this.check_clipboard(cx);
                                    cx.notify();
                                })),
                                &pal,
                            ))
                            .child(ui::setting(
                                "s-hotkey",
                                ui::setting_icon("keyboard", &pal),
                                "Global shortcut",
                                Some(match &self.hotkey.error {
                                    Some(e) if s.hotkey => e.clone().into(),
                                    _ => format!("Press {} in any app to open SideKit with your clipboard", hotkey::LABEL).into(),
                                }),
                                ui::toggle_labeled("s-hotkey-tg", s.hotkey, &pal, cx.listener(|this, _, _, cx| {
                                    Settings::update(cx, |s| s.hotkey = !s.hotkey);
                                    this.hotkey.set(Settings::get(cx).hotkey);
                                    cx.notify();
                                })),
                                &pal,
                            ))
                            .when(tray::SUPPORTED, |d| {
                                d.child(ui::setting(
                                    "s-tray",
                                    ui::setting_icon("tray", &pal),
                                    "Keep running when closed",
                                    Some(format!("Closing the window leaves SideKit in the {} so the shortcut still works", tray::PLACE).into()),
                                    ui::toggle_labeled("s-tray-tg", s.tray, &pal, cx.listener(|this, _, _, cx| {
                                        Settings::update(cx, |s| s.tray = !s.tray);
                                        this.tray.set(Settings::get(cx).tray);
                                        cx.notify();
                                    })),
                                    &pal,
                                ))
                            })
                            .child(ui::setting(
                                "s-favs",
                                ui::setting_icon("star", &pal),
                                "Favorites",
                                Some(fav_text),
                                ui::btn("clear-favs", None, "Clear favorites", BtnKind::Normal, &pal, cx.listener(|_, _, _, cx| {
                                    Settings::update(cx, |s| s.favorites.clear());
                                    cx.notify();
                                })),
                                &pal,
                            )),
                    ))
                    .child(ui::enter(
                        ("set-c", epoch),
                        100,
                        div()
                            .mt(px(12.))
                            .flex()
                            .flex_col()
                            .gap(px(4.))
                            .child(ui::section_label("About", &pal))
                            .child(ui::setting(
                                "s-about",
                                div()
                                    .size(px(20.))
                                    .rounded(px(5.))
                                    .bg(pal.accent)
                                    .flex()
                                    .items_center()
                                    .justify_center()
                                    .child(icon("logo@2.6", 12., pal.accent_text)),
                                APP_NAME,
                                Some(format!("Version {} · Open source · Runs fully offline", env!("SIDEKIT_VERSION")).into()),
                                div(),
                                &pal,
                            )),
                    )),
            )
            .into_any_element()
    }

    // ------------------------------------------------------------ tool page

    fn render_tool(&mut self, id: ToolId, _window: &mut Window, cx: &mut Context<Self>) -> AnyElement {
        let p = Pal::get(cx);
        let t = tool(id);
        let fav = Settings::get(cx).is_fav(t.key);
        let key = t.key;
        let view = self.tool_views.get(&id).cloned();
        let epoch = self.epoch;
        let star = if fav {
            icon("star.fill", 20., p.star)
                .with_animation(
                    id!("starpop-{epoch}-{fav}"),
                    Animation::new(Duration::from_millis(450)).with_easing(ui::bezier(0.3, 1.8, 0.5, 1.)),
                    |svg, t| {
                        let s = 0.5 + 0.5 * t;
                        svg.with_transformation(
                            Transformation::scale(size(s, s)).with_rotation(radians((-40f32).to_radians() * (1. - t))),
                        )
                    },
                )
                .into_any_element()
        } else {
            icon("star", 20., p.text3).into_any_element()
        };

        div()
            .flex_1()
            .min_h_0()
            .flex()
            .flex_col()
            .child(
                div()
                    .flex()
                    .items_center()
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
                            .child(
                                div()
                                    .flex()
                                    .items_center()
                                    .gap(px(6.))
                                    .text_size(px(12.))
                                    .text_color(p.text3)
                                    .child(cat(t.cat).label)
                                    .child(icon("chev-right", 10., p.text3))
                                    .child(t.name),
                            )
                            .child(div().text_size(px(26.)).font_weight(FontWeight::SEMIBOLD).child(t.title))
                            .child(div().text_size(px(13.)).text_color(p.text2).child(t.desc)),
                    )
                    .child(
                        div()
                            .id("fav-star")
                            .size(px(38.))
                            .flex()
                            .items_center()
                            .justify_center()
                            .rounded(px(6.))
                            .cursor_pointer()
                            .hover(move |s| s.bg(p.subtle2))
                            .child(star)
                            .on_click(cx.listener(move |this, _, _, cx| this.toggle_fav(key, cx))),
                    ),
            )
            .child(
                div()
                    .id(id!("tool-body-{key}"))
                    .flex_1()
                    .min_h_0()
                    .overflow_y_scrollbar()
                    .child(
                        div()
                            .min_h_full()
                            .pt(px(4.))
                            .px(px(36.))
                            .pb(px(28.))
                            .flex()
                            .flex_col()
                            .gap(px(6.))
                            .child(ui::enter(id!("tool-{key}-{epoch}"), 0, div().flex().flex_col().flex_1().when_some(view, |d, v| d.child(v)))),
                    ),
            )
            .into_any_element()
    }
}

fn kbd(text: &'static str, p: &Pal) -> Div {
    div()
        .flex_none()
        .text_size(px(11.))
        .px(px(6.))
        .py(px(2.))
        .rounded(px(4.))
        .border_1()
        .border_color(p.stroke_strong)
        .text_color(p.text3)
        .child(text)
}

pub fn kbd_el(text: &'static str, p: &Pal) -> Div {
    kbd(text, p)
}

impl Focusable for SideKit {
    fn focus_handle(&self, _: &App) -> FocusHandle {
        self.focus.clone()
    }
}

impl Render for SideKit {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let pal = Pal::get(cx);
        let view = self.view;
        let content: AnyElement = match view {
            View::Home => self.render_home(window, cx),
            View::Library => div().flex_1().min_h_0().flex().flex_col().child(self.library.clone()).into_any_element(),
            View::Settings => self.render_settings(window, cx),
            View::Tool(id) => self.render_tool(id, window, cx),
        };

        div()
            .id("app")
            .key_context("Shell")
            .track_focus(&self.focus)
            .on_action(cx.listener(|this, _: &OpenPalette, window, cx| this.open_palette(window, cx)))
            .on_action(cx.listener(|this, _: &FocusSearch, window, cx| this.open_palette(window, cx)))
            .on_action(cx.listener(|this, _: &GoBack, window, cx| this.back(window, cx)))
            .on_action(cx.listener(|this, _: &ToggleTheme, window, cx| this.toggle_theme(window, cx)))
            .on_action(cx.listener(|this, _: &OpenSettings, window, cx| this.go(View::Settings, window, cx)))
            .on_mouse_down(gpui_kit::MouseButton::Navigate(gpui_kit::NavigationDirection::Back), cx.listener(|this, _, window, cx| this.back(window, cx)))
            .relative()
            .size_full()
            .flex()
            .flex_col()
            .overflow_hidden()
            .bg(pal.mica)
            .text_color(pal.text)
            .font_family(theme::UI_FONT)
            .text_size(px(14.))
            .line_height(relative(1.3))
            .child(self.render_title_bar(window, cx))
            .child(
                div()
                    .flex()
                    .flex_1()
                    .min_h_0()
                    .child(self.render_nav(window, cx))
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .flex()
                            .flex_col()
                            .bg(pal.layer)
                            .border_t_1()
                            .border_l_1()
                            .border_color(pal.stroke)
                            .rounded_tl(px(10.))
                            .overflow_hidden()
                            .child(content),
                    ),
            )
            .when_some(self.palette.clone(), |d, p| d.child(p))
    }
}
