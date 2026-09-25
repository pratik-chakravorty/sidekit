#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod app;
mod hotkey;
mod icons;
mod library;
mod library_view;
mod logic;
mod palette;
mod registry;
mod settings;
mod theme;
mod tools;
mod workflow_view;
mod workflows;
mod tray;
mod ui;

use std::borrow::Cow;
use std::time::Instant;

use gpui_kit::component::Root;
use gpui_kit::{
    App, AppContext, Bounds, TitlebarOptions, WindowBackgroundAppearance, WindowBounds,
    WindowDecorations, WindowOptions, point, px, size,
};

use crate::settings::Settings;

static MONO_REGULAR: &[u8] = include_bytes!("../assets/fonts/JetBrainsMono-Regular.ttf");
static MONO_MEDIUM: &[u8] = include_bytes!("../assets/fonts/JetBrainsMono-Medium.ttf");
static MONO_SEMIBOLD: &[u8] = include_bytes!("../assets/fonts/JetBrainsMono-SemiBold.ttf");

fn main() {
    let started = Instant::now();
    let trace = std::env::var_os("SIDEKIT_TRACE").is_some();

    let mark = move |what: &str| {
        if trace {
            eprintln!("{what:>14}: {:?}", started.elapsed());
        }
    };
    let app = gpui_kit::application().with_assets(icons::Assets);
    mark("platform");
    app.run(move |cx: &mut App| {
        mark("run");
        let _ = cx.text_system().add_fonts(vec![
            Cow::Borrowed(MONO_REGULAR),
            Cow::Borrowed(MONO_MEDIUM),
            Cow::Borrowed(MONO_SEMIBOLD),
        ]);
        mark("fonts");
        gpui_kit::init(cx);
        mark("kit init");

        let mut settings = Settings::load();
        if settings.follow_system {
            settings.dark = theme::system_dark(cx);
        }
        let dark = settings.dark;
        cx.set_global(settings);
        theme::apply(dark, None, cx);
        app::bind_keys(cx);
        palette::bind_keys(cx);
        mark("theme+keys");

        // One window, one process: closing it quits on every platform.
        cx.on_window_closed(|cx, _| {
            if cx.windows().is_empty() {
                cx.quit();
            }
        })
        .detach();

        let bounds = Bounds::centered(None, size(px(1320.), px(860.)), cx);
        let options = WindowOptions {
            window_bounds: Some(WindowBounds::Windowed(bounds)),
            titlebar: Some(TitlebarOptions {
                title: Some(app::APP_NAME.into()),
                appears_transparent: true,
                // Centre macOS traffic lights in the 48px title bar.
                traffic_light_position: Some(point(px(18.), px(18.))),
            }),
            // Linux: draw our own title bar and caption buttons where the compositor
            // allows it; server-side decorations are still honoured if it insists.
            window_decorations: Some(WindowDecorations::Client),
            app_owns_titlebar_drag: true,
            window_min_size: Some(size(px(960.), px(600.))),
            window_background: WindowBackgroundAppearance::Opaque,
            app_id: Some("sidekit".into()),
            ..Default::default()
        };

        cx.open_window(options, |window, cx| {
            mark("window");
            let view = cx.new(|cx| app::SideKit::new(window, cx));
            if trace {
                window.on_next_frame(move |_, _| {
                    eprintln!("first frame after {:?}", started.elapsed());
                });
            }
            cx.new(|cx| Root::new(view, window, cx))
        })
        .expect("failed to open the main window");
        cx.activate(true);
    });
}
