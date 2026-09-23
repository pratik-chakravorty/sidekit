//! Stroke icons from the design, served through gpui's asset pipeline.
//!
//! Every icon is a 24×24 stroke path. Paths are addressed as `dt/<name>` and
//! turned into a tiny SVG document on demand, so nothing is read from disk.

use std::borrow::Cow;

use gpui_kit::{AssetSource, Hsla, Result, SharedString, Svg, prelude::*, px, svg};

const PATHS: &[(&str, &str)] = &[
    // Categories
    ("conv", "M7 4L3 8l4 4M3 8h14M17 12l4 4-4 4M21 16H7"),
    ("enc", "M4 6h16v12H4zM8 10h2M13 10h3M8 14h8"),
    ("fmt", "M8 4c-2 0-2 1.5-2 3v2c0 1.5-1 3-2 3 1 0 2 1.5 2 3v2c0 1.5 0 3 2 3M16 4c2 0 2 1.5 2 3v2c0 1.5 1 3 2 3-1 0-2 1.5-2 3v2c0 1.5 0 3-2 3"),
    ("gen", "M12 3l1.8 5.2L19 10l-5.2 1.8L12 17l-1.8-5.2L5 10l5.2-1.8zM19 16l.8 2.2L22 19l-2.2.8L19 22l-.8-2.2L16 19l2.2-.8z"),
    ("gfx", "M12 3a9 9 0 1 0 0 18c1 0 1.6-.7 1.6-1.6 0-1.1-1-1.4-1-2.4s.8-1.6 2-1.6H17a4 4 0 0 0 4-4C21 7 17 3 12 3zM7.5 11.5h.01M10 7.5h.01M15 7.5h.01"),
    ("test", "M9 3h6M10 3v6L4.6 18.2A1.8 1.8 0 0 0 6.2 21h11.6a1.8 1.8 0 0 0 1.6-2.8L14 9V3M7 15h10"),
    ("text", "M5 5h14M12 5v14M9 19h6"),
    // Tools
    ("star", "M12 3.5l2.6 5.3 5.9.9-4.3 4.1 1 5.8-5.2-2.7-5.2 2.7 1-5.8-4.3-4.1 5.9-.9z"),
    ("swap", "M7 4L3 8l4 4M3 8h14M17 12l4 4-4 4M21 16H7"),
    ("hash", "M4 9h16M4 15h16M10 3L8 21M16 3l-2 18"),
    ("cal", "M4 6h16v14H4zM4 10h16M8 3v4M16 3v4M8 14h2M14 14h2"),
    ("b64", "M7 8l-4 4 4 4M17 8l4 4-4 4M14 5l-4 14"),
    ("link", "M10 14a4 4 0 0 0 5.7 0l3-3a4 4 0 0 0-5.7-5.7l-1 1M14 10a4 4 0 0 0-5.7 0l-3 3a4 4 0 0 0 5.7 5.7l1-1"),
    ("html", "M8 7l-5 5 5 5M16 7l5 5-5 5"),
    ("shield", "M12 3l8 3v6c0 4.5-3.4 8-8 9-4.6-1-8-4.5-8-9V6zM9 12l2 2 4-4"),
    ("braces", "M8 4c-2 0-2 1.5-2 3v2c0 1.5-1 3-2 3 1 0 2 1.5 2 3v2c0 1.5 0 3 2 3M16 4c2 0 2 1.5 2 3v2c0 1.5 1 3 2 3-1 0-2 1.5-2 3v2c0 1.5 0 3-2 3"),
    ("finger", "M12 4a8 8 0 0 1 8 8M4 12a8 8 0 0 1 3.5-6.6M12 8a4 4 0 0 1 4 4v2M8 12a4 4 0 0 1 1.5-3.1M12 12v3a7 7 0 0 1-1.2 4M8 15a10 10 0 0 1-.8 3.5M16 17.5a13 13 0 0 1-.5 2.5"),
    ("uuid", "M4 4h16v16H4zM8 8h3v3H8zM13 13h3v3h-3zM13 8h3M8 16h3"),
    ("lock", "M5 11h14v10H5zM8 11V7a4 4 0 0 1 8 0v4M12 15v2"),
    ("lines", "M4 6h16M4 10h16M4 14h16M4 18h10"),
    ("drop", "M12 3s6 6.5 6 11a6 6 0 0 1-12 0c0-4.5 6-11 6-11z"),
    ("regex", "M15 3v9M11 5.2l8 4.6M19 5.2l-8 4.6M6 19h.01"),
    ("case", "M3 18L7 6l4 12M4.5 14h5M14 18V8h3.5a2.5 2.5 0 0 1 0 5H14h4a2.5 2.5 0 0 1 0 5z"),
    ("slash", "M7 4l10 16"),
    // Chrome
    ("back", "M19 12H5M11 18l-6-6 6-6"),
    ("logo", "M8 5l-5 7 5 7M16 5l5 7-5 7"),
    ("search", "M11 4a7 7 0 1 0 0 14 7 7 0 0 0 0-14zM20 20l-4-4"),
    ("x", "M6 6l12 12M18 6L6 18"),
    ("moon", "M20 14.5A8 8 0 1 1 9.5 4a6.5 6.5 0 0 0 10.5 10.5z"),
    ("sun", "M12 8a4 4 0 1 0 0 8 4 4 0 0 0 0-8zM12 2v2M12 20v2M2 12h2M20 12h2M4.9 4.9l1.4 1.4M17.7 17.7l1.4 1.4M4.9 19.1l1.4-1.4M17.7 6.3l1.4-1.4"),
    ("min", "M4 12h16"),
    ("max", "M4 4h16v16H4z"),
    ("restore", "M7 7h13v13H7zM4 16V4h12"),
    ("expand", "M8 3H3v5M16 3h5v5M21 16v5h-5M8 21H3v-5M3 3l6 6M21 3l-6 6M21 21l-6-6M3 21l6-6"),
    ("shrink", "M3 8h5V3M21 8h-5V3M16 21v-5h5M8 21v-5H3M8 8L3 3M16 8l5-5M16 16l5 5M8 16l-5 5"),
    ("close", "M5 5l14 14M19 5L5 19"),
    ("grid", "M4 4h6v6H4zM14 4h6v6h-6zM4 14h6v6H4zM14 14h6v6h-6z"),
    ("chev-down", "M6 9l6 6 6-6"),
    ("chev-up", "M6 15l6-6 6 6"),
    ("chev-right", "M9 6l6 6-6 6"),
    ("gear", "M12 9a3 3 0 1 0 0 6 3 3 0 0 0 0-6zM12 2.5v3M12 18.5v3M2.5 12h3M18.5 12h3M5.3 5.3l2.1 2.1M16.6 16.6l2.1 2.1M5.3 18.7l2.1-2.1M16.6 7.4l2.1-2.1"),
    ("arrow", "M5 12h14M13 6l6 6-6 6"),
    ("paste", "M9 3h6v3H9zM8 4.5H5.5v16h13v-16H16"),
    ("copy", "M9 9h11v11H9zM5 15H4V4h11v1"),
    ("check", "M5 12l5 5 9-10"),
    ("refresh", "M20 11a8 8 0 1 0-2.3 5.7M20 4v7h-7"),
    ("clock", "M12 7v5l3 2M12 3a9 9 0 1 0 9 9"),
    ("enter", "M20 5v7a3 3 0 0 1-3 3H5M9 11l-4 4 4 4"),
    ("updown", "M8 9l4-4 4 4M8 15l4 4 4-4"),
    // Settings / configuration rows
    ("contrast", "M12 3a9 9 0 1 0 0 18V3zM12 3a9 9 0 0 1 0 18"),
    ("fontsize", "M4 7V5h12v2M10 5v14M7 19h6M15 13v-1h6v1M18 12v7M16.5 19h3"),
    ("wrap", "M4 7h16M4 12h13a3 3 0 0 1 0 6h-4M15 16l-2 2 2 2M4 17h5"),
    ("sparkle", "M12 3v3M12 18v3M3 12h3M18 12h3M6.3 6.3l2 2M15.7 15.7l2 2M6.3 17.7l2-2M15.7 8.3l2-2"),
    ("indent", "M4 6h16M10 10h10M10 14h10M4 18h16M4 10l3 2-3 2"),
    ("sort", "M4 6h9M4 12h6M4 18h3M17 5v14M14 16l3 3 3-3"),
    ("group", "M4 12h3M10 12h4M17 12h3M4 7v10M20 7v10"),
    ("globe", "M12 3a9 9 0 1 0 0 18 9 9 0 0 0 0-18zM3 12h18M12 3c2.5 2.5 3.5 5.5 3.5 9s-1 6.5-3.5 9c-2.5-2.5-3.5-5.5-3.5-9s1-6.5 3.5-9z"),
    ("upper", "M4 18L8 6l4 12M5.5 14h5M15 18V8h3.5a2.5 2.5 0 0 1 0 5H15h4a2.5 2.5 0 0 1 0 5z"),
    ("dashes", "M3 12h4M10 12h4M17 12h4"),
    ("version", "M4 4h16v16H4zM8 8h3v3H8zM13 13h3v3h-3z"),
    ("length", "M3 12h18M6 9l-3 3 3 3M18 9l3 3-3 3"),
];

fn path_data(name: &str) -> Option<&'static str> {
    PATHS.iter().find(|(n, _)| *n == name).map(|(_, d)| *d)
}

/// Build the SVG document for `dt/<name>[@<stroke>][.fill]`.
fn document(spec: &str) -> Option<String> {
    let (spec, fill) = match spec.strip_suffix(".fill") {
        Some(s) => (s, true),
        None => (spec, false),
    };
    let (name, stroke) = match spec.split_once('@') {
        Some((n, w)) => (n, w),
        None => (spec, "1.6"),
    };
    let d = path_data(name)?;
    let fill = if fill { "#000" } else { "none" };
    Some(format!(
        r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" fill="{fill}" stroke="#000" stroke-width="{stroke}" stroke-linecap="round" stroke-linejoin="round"><path d="{d}"/></svg>"##
    ))
}

/// Our icons first, then the Lucide catalog gpui-component relies on.
pub struct Assets;

impl AssetSource for Assets {
    fn load(&self, path: &str) -> Result<Option<Cow<'static, [u8]>>> {
        if let Some(spec) = path.strip_prefix("dt/") {
            return Ok(document(spec).map(|s| Cow::Owned(s.into_bytes())));
        }
        gpui_kit::assets::Assets.load(path)
    }

    fn list(&self, path: &str) -> Result<Vec<SharedString>> {
        gpui_kit::assets::Assets.list(path)
    }
}

/// A design icon at `size` px. SVGs do not inherit text color, so it is explicit.
pub fn icon(name: &str, size: f32, color: impl Into<Hsla>) -> Svg {
    svg()
        .path(SharedString::from(format!("dt/{name}")))
        .size(px(size))
        .flex_none()
        .text_color(color)
}
