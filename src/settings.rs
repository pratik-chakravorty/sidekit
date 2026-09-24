//! User preferences, persisted as a small JSON file next to other app data.

use std::path::PathBuf;

use gpui_kit::{App, AppContext, Global};
use serde::{Deserialize, Serialize};

#[derive(Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    pub dark: bool,
    /// Follow the operating system's light / dark setting; `dark` then
    /// mirrors it.
    pub follow_system: bool,
    pub favorites: Vec<String>,
    pub wrap: bool,
    pub smart: bool,
    pub font_size: u8,
    /// System-wide shortcut that brings SideKit forward.
    pub hotkey: bool,
    /// Closing the window hides it to the tray instead of quitting.
    pub tray: bool,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            dark: false,
            follow_system: false,
            favorites: ["jsonfmt", "base64", "jwt", "regex"].map(String::from).to_vec(),
            wrap: false,
            smart: true,
            font_size: 13,
            hotkey: true,
            tray: true,
        }
    }
}

impl Global for Settings {}

pub fn config_dir() -> Option<PathBuf> {
    std::env::var_os("APPDATA")
        .or_else(|| std::env::var_os("XDG_CONFIG_HOME"))
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".config")))
}

fn path() -> Option<PathBuf> {
    Some(config_dir()?.join("SideKit").join("settings.json"))
}

/// Settings written before the app was renamed from ToyDev.
fn legacy_path() -> Option<PathBuf> {
    Some(config_dir()?.join("ToyDev").join("settings.json"))
}

impl Settings {
    pub fn load() -> Self {
        path()
            .and_then(|p| std::fs::read(p).ok())
            .or_else(|| legacy_path().and_then(|p| std::fs::read(p).ok()))
            .and_then(|b| serde_json::from_slice(&b).ok())
            .unwrap_or_default()
    }

    pub fn get(cx: &App) -> &Settings {
        cx.global::<Settings>()
    }

    pub fn is_fav(&self, key: &str) -> bool {
        self.favorites.iter().any(|f| f == key)
    }

    /// Mutate the global settings and write them out off the UI thread.
    pub fn update(cx: &mut App, f: impl FnOnce(&mut Settings)) {
        f(cx.global_mut::<Settings>());
        let snapshot = cx.global::<Settings>().clone();
        cx.background_spawn(async move {
            if let Some(p) = path() {
                if let Some(dir) = p.parent() {
                    let _ = std::fs::create_dir_all(dir);
                }
                if let Ok(json) = serde_json::to_vec_pretty(&snapshot) {
                    let _ = std::fs::write(p, json);
                }
            }
        })
        .detach();
    }
}
