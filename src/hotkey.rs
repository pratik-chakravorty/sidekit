//! The system-wide shortcut that brings SideKit forward from any app.
//!
//! Registration has to happen on the main thread: Windows ties the hotkey to
//! that thread's message loop and macOS to the main run loop. Linux works
//! under X11 only; Wayland has no global shortcut API for this.

use global_hotkey::hotkey::{Code, HotKey, Modifiers};
use global_hotkey::{GlobalHotKeyEvent, GlobalHotKeyManager, HotKeyState};

/// Shown in Settings; keep in step with `combo`.
pub const LABEL: &str = if cfg!(target_os = "macos") { "⌃ ⌥ K" } else { "Ctrl + Alt + K" };

/// The in-app palette is Ctrl+K; adding Alt makes it reach from any app.
/// (Ctrl+Alt+Space looks natural but other launchers, Claude among them, own it.)
fn combo() -> HotKey {
    HotKey::new(Some(Modifiers::CONTROL | Modifiers::ALT), Code::KeyK)
}

pub struct Hotkey {
    manager: Option<GlobalHotKeyManager>,
    key: HotKey,
    on: bool,
    /// Why the last attempt to turn it on failed, e.g. another app owns the combo.
    pub error: Option<String>,
}

impl Hotkey {
    /// Presses arrive on the returned channel, one `()` per key-down.
    pub fn new() -> (Self, async_channel::Receiver<()>) {
        let (tx, rx) = async_channel::unbounded();
        GlobalHotKeyEvent::set_event_handler(Some(move |ev: GlobalHotKeyEvent| {
            if ev.state() == HotKeyState::Pressed {
                let _ = tx.try_send(());
            }
        }));
        let (manager, error) = match GlobalHotKeyManager::new() {
            Ok(m) => (Some(m), None),
            Err(e) => (None, Some(e.to_string())),
        };
        (Self { manager, key: combo(), on: false, error }, rx)
    }

    pub fn set(&mut self, on: bool) {
        let Some(m) = &self.manager else { return };
        if on == self.on {
            return;
        }
        let result = if on { m.register(self.key) } else { m.unregister(self.key) };
        match result {
            Ok(()) => {
                self.on = on;
                self.error = None;
            }
            Err(e) if on => self.error = Some(format!("Could not register {LABEL}: {e}")),
            Err(_) => self.on = false,
        }
    }
}
