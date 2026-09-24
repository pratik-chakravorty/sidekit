//! Tray (Windows) / menu bar (macOS) icon, so closing the window can keep
//! SideKit running for the global shortcut.
//!
//! Linux has no tray here: `tray-icon` needs a GTK main loop there, and GPUI
//! does not run one. Closing the window still quits on Linux.

use gpui_kit::{App, Window};

pub const SUPPORTED: bool = cfg!(any(windows, target_os = "macos"));

/// Where the icon lives, for Settings copy.
pub const PLACE: &str = if cfg!(target_os = "macos") { "menu bar" } else { "system tray" };

pub enum TrayMsg {
    Open,
    Quit,
}

#[cfg(any(windows, target_os = "macos"))]
mod imp {
    use tray_icon::menu::{Menu, MenuEvent, MenuItem, PredefinedMenuItem};
    use tray_icon::{Icon, MouseButton, MouseButtonState, TrayIcon, TrayIconBuilder, TrayIconEvent};

    use super::TrayMsg;

    #[derive(Default)]
    pub struct Tray {
        icon: Option<TrayIcon>,
    }

    /// Tray clicks and menu picks. The handlers are process-wide and can be
    /// set only once, so call this once.
    pub fn events() -> async_channel::Receiver<TrayMsg> {
        let (tx, rx) = async_channel::unbounded();
        let clicks = tx.clone();
        TrayIconEvent::set_event_handler(Some(move |ev: TrayIconEvent| {
            if let TrayIconEvent::Click { button: MouseButton::Left, button_state: MouseButtonState::Up, .. } = ev {
                let _ = clicks.try_send(TrayMsg::Open);
            }
        }));
        MenuEvent::set_event_handler(Some(move |ev: MenuEvent| {
            let msg = match ev.id.as_ref() {
                "open" => TrayMsg::Open,
                "quit" => TrayMsg::Quit,
                _ => return,
            };
            let _ = tx.try_send(msg);
        }));
        rx
    }

    impl Tray {
        pub fn set(&mut self, on: bool) {
            if on == self.icon.is_some() {
                return;
            }
            self.icon = if on { build() } else { None };
        }
    }

    fn build() -> Option<TrayIcon> {
        let menu = Menu::new();
        let open = MenuItem::with_id("open", format!("Open SideKit\t{}", crate::hotkey::LABEL), true, None);
        let quit = MenuItem::with_id("quit", "Quit SideKit", true, None);
        menu.append_items(&[&open, &PredefinedMenuItem::separator(), &quit]).ok()?;
        TrayIconBuilder::new()
            .with_icon(icon()?)
            .with_tooltip(format!("SideKit · {}", crate::hotkey::LABEL))
            .with_menu(Box::new(menu))
            .with_menu_on_left_click(false)
            .build()
            .ok()
    }

    /// The executable's embedded icon (resource 1, see build.rs).
    #[cfg(windows)]
    fn icon() -> Option<Icon> {
        Icon::from_resource(1, None).ok()
    }

    #[cfg(target_os = "macos")]
    fn icon() -> Option<Icon> {
        static PNG: &[u8] = include_bytes!("../assets/icon/icon-32.png");
        let mut reader = png::Decoder::new(std::io::Cursor::new(PNG)).read_info().ok()?;
        let mut buf = vec![0; reader.output_buffer_size()?];
        let info = reader.next_frame(&mut buf).ok()?;
        buf.truncate(info.buffer_size());
        Icon::from_rgba(buf, info.width, info.height).ok()
    }
}

#[cfg(not(any(windows, target_os = "macos")))]
mod imp {
    use super::TrayMsg;

    #[derive(Default)]
    pub struct Tray;

    pub fn events() -> async_channel::Receiver<TrayMsg> {
        // Keep the sender alive so the listener simply waits forever.
        let (tx, rx) = async_channel::unbounded();
        std::mem::forget(tx);
        rx
    }

    impl Tray {
        pub fn set(&mut self, _on: bool) {}
    }
}

pub use imp::{Tray, events};

/// Take the window off screen and out of the taskbar without closing it.
pub fn hide(window: &mut Window, cx: &mut App) {
    #[cfg(windows)]
    {
        let _ = cx;
        if let Some(hwnd) = hwnd(window) {
            use windows_sys::Win32::UI::WindowsAndMessaging::{SW_HIDE, ShowWindow};
            unsafe { ShowWindow(hwnd, SW_HIDE) };
        }
    }
    #[cfg(target_os = "macos")]
    {
        let _ = window;
        cx.hide();
    }
    #[cfg(not(any(windows, target_os = "macos")))]
    window.minimize_window();
}

/// Undo `hide` and bring the window to the front.
pub fn show(window: &mut Window, cx: &mut App) {
    #[cfg(windows)]
    if let Some(hwnd) = hwnd(window) {
        use windows_sys::Win32::UI::WindowsAndMessaging::{IsWindowVisible, SW_SHOW, ShowWindow};
        unsafe {
            if IsWindowVisible(hwnd) == 0 {
                ShowWindow(hwnd, SW_SHOW);
            }
        }
    }
    cx.activate(true);
    window.activate_window();
}

#[cfg(windows)]
fn hwnd(window: &Window) -> Option<windows_sys::Win32::Foundation::HWND> {
    use raw_window_handle::{HasWindowHandle, RawWindowHandle};
    match HasWindowHandle::window_handle(window).ok()?.as_raw() {
        RawWindowHandle::Win32(h) => Some(h.hwnd.get() as _),
        _ => None,
    }
}
