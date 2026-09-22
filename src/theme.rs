//! Design tokens from the mock-up (`.app.light` / `.app.dark`) and the bridge
//! that projects them onto gpui-component's theme so its inputs match.

use gpui_kit::component::{Theme, ThemeMode};
use gpui_kit::{App, BoxShadow, Global, Hsla, Window, hsla, point, px, rgb};

pub const UI_FONT: &str = "Segoe UI Variable Text";
pub const MONO_FONT: &str = "JetBrains Mono";

#[derive(Clone, Copy)]
pub struct Pal {
    pub dark: bool,
    pub mica: Hsla,
    pub layer: Hsla,
    pub card: Hsla,
    pub card_hover: Hsla,
    pub stroke: Hsla,
    pub stroke_strong: Hsla,
    pub text: Hsla,
    pub text2: Hsla,
    pub text3: Hsla,
    pub accent: Hsla,
    pub accent_hover: Hsla,
    pub accent_text: Hsla,
    pub accent_soft: Hsla,
    pub subtle: Hsla,
    pub subtle2: Hsla,
    pub editor: Hsla,
    pub danger: Hsla,
    pub danger_soft: Hsla,
    pub ok: Hsla,
    pub ok_soft: Hsla,
    pub warn: Hsla,
    pub mark: Hsla,
    pub star: Hsla,
    pub tok_h: Hsla,
    pub tok_p: Hsla,
    pub tok_s: Hsla,
    shadow_a: f32,
    shadow_b: f32,
}

impl Global for Pal {}

fn c(hex: u32) -> Hsla {
    rgb(hex).into()
}

fn black(a: f32) -> Hsla {
    hsla(0., 0., 0., a)
}

fn white(a: f32) -> Hsla {
    hsla(0., 0., 1., a)
}

fn alpha(hex: u32, a: f32) -> Hsla {
    let mut h = c(hex);
    h.a = a;
    h
}

impl Pal {
    pub fn light() -> Self {
        Self {
            dark: false,
            mica: c(0xeef0f3),
            layer: c(0xf9f9fa),
            card: c(0xffffff),
            card_hover: c(0xf6f7f8),
            stroke: black(0.075),
            stroke_strong: black(0.14),
            text: c(0x1a1a1a),
            text2: c(0x555a60),
            text3: c(0x6b7076),
            accent: c(0x005fb8),
            accent_hover: c(0x0a6cc7),
            accent_text: c(0xffffff),
            accent_soft: alpha(0x005fb8, 0.10),
            subtle: black(0.04),
            subtle2: black(0.07),
            editor: c(0xffffff),
            danger: c(0xb3261e),
            danger_soft: alpha(0xb3261e, 0.08),
            ok: c(0x0f7b0f),
            ok_soft: alpha(0x0f7b0f, 0.10),
            warn: c(0x9a5b00),
            mark: alpha(0x005fb8, 0.16),
            star: c(0xc98a00),
            tok_h: c(0xd1364a),
            tok_p: c(0x8b3fd9),
            tok_s: c(0x0a7ec2),
            shadow_a: 0.08,
            shadow_b: 0.06,
        }
    }

    pub fn dark() -> Self {
        Self {
            dark: true,
            mica: c(0x1c1c1e),
            layer: c(0x262628),
            card: c(0x2d2d30),
            card_hover: c(0x343437),
            stroke: white(0.07),
            stroke_strong: white(0.13),
            text: c(0xf3f3f3),
            text2: c(0xc4c6c9),
            text3: c(0xa2a5aa),
            accent: c(0x60cdff),
            accent_hover: c(0x7fd6ff),
            accent_text: c(0x001a28),
            accent_soft: alpha(0x60cdff, 0.12),
            subtle: white(0.05),
            subtle2: white(0.08),
            editor: c(0x1f1f21),
            danger: c(0xff9a93),
            danger_soft: alpha(0xff9a93, 0.10),
            ok: c(0x7fd67a),
            ok_soft: alpha(0x7fd67a, 0.10),
            warn: c(0xffc46b),
            mark: alpha(0x60cdff, 0.22),
            star: c(0xffcf4a),
            tok_h: c(0xff8a9a),
            tok_p: c(0xc79bff),
            tok_s: c(0x6fd0ff),
            shadow_a: 0.35,
            shadow_b: 0.30,
        }
    }

    pub fn get(cx: &App) -> Pal {
        *cx.global::<Pal>()
    }

    /// `--shadow`: the lifted-card shadow.
    pub fn shadow(&self) -> Vec<BoxShadow> {
        vec![
            BoxShadow {
                color: black(self.shadow_a),
                offset: point(px(0.), px(8.)),
                blur_radius: px(24.),
                spread_radius: px(0.),
                inset: false,
            },
            BoxShadow {
                color: black(self.shadow_b),
                offset: point(px(0.), px(1.)),
                blur_radius: px(3.),
                spread_radius: px(0.),
                inset: false,
            },
        ]
    }

    /// A focus/hover halo like `box-shadow: 0 0 0 3px var(--accent-soft)`.
    pub fn ring(color: Hsla, width: f32) -> BoxShadow {
        BoxShadow {
            color,
            offset: point(px(0.), px(0.)),
            blur_radius: px(0.),
            spread_radius: px(width),
            inset: false,
        }
    }

    /// The Fluent input underline: `box-shadow: inset 0 -Npx 0 color`.
    pub fn underline(color: Hsla, width: f32) -> BoxShadow {
        BoxShadow {
            color,
            offset: point(px(0.), px(-width)),
            blur_radius: px(0.),
            spread_radius: px(0.),
            inset: true,
        }
    }
}

/// Switch light/dark and push the palette into gpui-component's theme.
pub fn apply(dark: bool, window: Option<&mut Window>, cx: &mut App) {
    let pal = if dark { Pal::dark() } else { Pal::light() };
    cx.set_global(pal);
    Theme::change(if dark { ThemeMode::Dark } else { ThemeMode::Light }, None, cx);

    let t = Theme::global_mut(cx);
    t.font_family = UI_FONT.into();
    t.mono_font_family = MONO_FONT.into();
    t.font_size = px(14.);
    t.mono_font_size = px(13.);
    t.radius = px(6.);
    t.radius_lg = px(8.);
    t.focus_ring = false;
    t.shadow = true;

    let k = &mut t.colors;
    k.background = pal.editor;
    k.foreground = pal.text;
    k.muted = pal.subtle;
    k.muted_foreground = pal.text3;
    k.border = pal.stroke_strong;
    k.input = pal.stroke_strong;
    k.ring = pal.accent;
    k.caret = pal.text;
    k.selection = pal.mark;
    k.primary = pal.accent;
    k.primary_hover = pal.accent_hover;
    k.primary_active = pal.accent;
    k.primary_foreground = pal.accent_text;
    k.accent = pal.subtle2;
    k.accent_foreground = pal.text;
    k.popover = pal.card;
    k.popover_foreground = pal.text;
    k.list = pal.card;
    k.list_hover = pal.subtle;
    k.list_active = pal.accent_soft;
    k.list_active_border = pal.accent;
    k.secondary = pal.card;
    k.secondary_hover = pal.card_hover;
    k.secondary_active = pal.subtle2;
    k.secondary_foreground = pal.text;
    k.slider_bar = pal.accent;
    k.slider_thumb = pal.accent;
    k.scrollbar = gpui_kit::transparent_black();
    k.scrollbar_thumb = pal.subtle2;
    k.scrollbar_thumb_hover = pal.text3;
    k.danger = pal.danger;
    k.success = pal.ok;
    k.warning = pal.warn;
    k.link = pal.accent;
    k.title_bar = pal.mica;

    let tk = &mut t.tokens;
    tk.slider_bar = pal.accent.into();
    tk.slider_thumb = pal.accent.into();
    tk.scrollbar_thumb = pal.subtle2.into();
    tk.scrollbar_thumb_hover = pal.text3.into();

    Theme::sync_base(cx);
    if let Some(window) = window {
        window.refresh();
    }
}
