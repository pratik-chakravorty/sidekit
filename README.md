# SideKit

A fast, native developer toolbox built in Rust on [GPUI](https://gpui.rs/) and
[GPUI Kit](https://gpui-kit.com) (gpui-component). Sixteen everyday tools —
formatters, encoders, generators, converters and testers — in one window that
opens in about a quarter of a second and runs entirely offline.

![SideKit home screen in light mode](docs/screenshot.png)

![SideKit JWT Decoder in dark mode](docs/screenshot-dark.png)

## Run

```sh
cargo run --release
```

The optimized binary lands in `target/release/sidekit` (`sidekit.exe` on
Windows) and is self-contained: fonts and icons are embedded.

The toolchain is pinned in `rust-toolchain.toml` (GPUI needs Rust ≥ 1.93).
On Windows the Windows SDK's `fxc.exe` must be installed; GPUI compiles its
shaders with it at build time.

## Keyboard

| Keys              | Action                               |
| ----------------- | ------------------------------------ |
| `Ctrl K` / `⌘ K`  | Command palette (tools and commands) |
| `Ctrl F`          | Focus the tool search                |
| `Alt ←`           | Back                                 |
| `Ctrl Shift T`    | Toggle light / dark                  |
| `Ctrl ,`          | Settings                             |

In the palette: `↑` / `↓` to move, `Enter` to open, `Esc` to close.

## Tools

| Category            | Tools                                              |
| ------------------- | -------------------------------------------------- |
| Converters          | JSON → YAML, Number base, Date / Unix timestamp    |
| Encoders / Decoders | Base64 text, URL, HTML entities, JWT decoder       |
| Formatters          | JSON (indent, minify, sort, validate)              |
| Generators          | Hash (MD5, SHA-1/256/384/512), UUID v4, Password, Lorem ipsum |
| Graphic             | Color converter with WCAG contrast and shades      |
| Testers             | Regular expression tester with match details       |
| Text                | Text analyzer & case converter, Escape / unescape  |

## Settings

- **App theme** — light or dark.
- **Editor font size** — 12–15 px for every input and output editor.
- **Wrap long lines** — soft-wrap editors instead of scrolling sideways.
- **Smart detection** — when SideKit starts or regains focus it looks at the
  clipboard; JWTs, JSON, Unix timestamps, colors, Base64 and URL-encoded text
  get an "Open …" suggestion on the home page that loads the clipboard straight
  into the matching tool.
- **Favorites** — star any tool to pin it to the navigation and home page.

Preferences are stored in `%APPDATA%\SideKit\settings.json` on Windows and
`$XDG_CONFIG_HOME/SideKit/settings.json` (or `~/.config/SideKit/`) elsewhere.
Settings from the earlier ToyDev name are picked up automatically.

## Platforms

The title bar is drawn by the app on every platform:

- **Windows** — caption buttons map to native hit-test areas, so Snap Layouts
  and window dragging behave like any Windows app.
- **Linux** — client-side decorations where the compositor allows them, with
  working minimize / maximize / close, drag, double-click to maximize and the
  right-click window menu; server-side decorations are respected otherwise.
- **macOS** — the system traffic lights sit in the title bar and the app
  handles dragging and double-click zoom.

## Project layout

| Path              | What                                                    |
| ----------------- | ------------------------------------------------------- |
| `src/main.rs`     | Window setup, fonts, theme, key bindings                |
| `src/app.rs`      | Shell: title bar, navigation, home, settings, tool page |
| `src/palette.rs`  | Ctrl+K command palette with fuzzy ranking               |
| `src/ui.rs`       | Design components (settings rows, toggles, panes, …)    |
| `src/theme.rs`    | Design tokens and the bridge to gpui-component's theme  |
| `src/logic.rs`    | Pure conversion logic and clipboard detection, tested   |
| `src/tools/`      | One view per tool, created lazily on first open         |

Run the tests with `cargo test`.

## Start-up

Set `SIDEKIT_TRACE=1` to print start-up phases. On the development machine
(RTX 4080 laptop) the first frame lands about 220 ms after launch in a release
build; roughly 170 ms of that is GPUI's own Direct3D device and DirectWrite
initialization, the app itself adds ~50 ms. Tool views are only built when
first opened.
