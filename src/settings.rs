//! User preferences, persisted as a small JSON file next to other app data.

use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::sync::atomic::{AtomicU64, Ordering};

use gpui_kit::{App, AppContext, Global};
use serde::{Deserialize, Serialize};

/// Counts changes, so every save knows how recent its snapshot is.
static REVISION: AtomicU64 = AtomicU64::new(0);
/// The newest revision on disk. Saves run on background threads in no fixed
/// order; holding this takes them one at a time and lets a stale one step
/// aside instead of overwriting a newer one.
static SAVED: Mutex<u64> = Mutex::new(0);

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
        let rev = REVISION.fetch_add(1, Ordering::Relaxed) + 1;
        cx.background_spawn(async move {
            if let Some(p) = path() {
                snapshot.save(rev, &SAVED, &p);
            }
        })
        .detach();
    }

    /// A change made just before quitting may still be queued on a background
    /// thread; write it out before the process exits.
    pub fn save_on_quit(cx: &mut App) {
        cx.on_app_quit(|cx| {
            if let Some(p) = path() {
                Settings::get(cx).save(REVISION.load(Ordering::Relaxed), &SAVED, &p);
            }
            async {}
        })
        .detach();
    }

    /// Write revision `rev` unless that one or a newer one is already on disk.
    fn save(&self, rev: u64, saved: &Mutex<u64>, path: &Path) {
        let mut saved = saved.lock().unwrap_or_else(|e| e.into_inner());
        if rev > *saved && self.write(path).is_ok() {
            *saved = rev;
        }
    }

    /// Replace the file in one step: a half-written one would fail to parse
    /// and silently load as the defaults.
    fn write(&self, path: &Path) -> std::io::Result<()> {
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir)?;
        }
        let tmp = path.with_extension("json.tmp");
        std::fs::write(&tmp, serde_json::to_vec_pretty(self)?)?;
        std::fs::rename(&tmp, path)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_file(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("sidekit-settings-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        dir.join("settings.json")
    }

    fn read(path: &Path) -> Settings {
        serde_json::from_slice(&std::fs::read(path).unwrap()).unwrap()
    }

    #[test]
    fn stale_snapshot_does_not_overwrite_newer() {
        let (path, saved) = (temp_file("stale"), Mutex::new(0));
        let light = Settings::default();
        let dark = Settings { dark: true, ..Settings::default() };
        // The newer snapshot reaches the disk first; the older one must step aside.
        dark.save(2, &saved, &path);
        light.save(1, &saved, &path);
        assert!(read(&path).dark);
        light.save(3, &saved, &path);
        assert!(!read(&path).dark);
        let _ = std::fs::remove_dir_all(path.parent().unwrap());
    }

    #[test]
    fn racing_saves_keep_the_latest() {
        let (path, saved) = (temp_file("race"), Mutex::new(0));
        for round in 0..200u64 {
            // Switching the theme used to queue two saves, the first one stale.
            let (stale, fresh) = (round * 2 + 1, round * 2 + 2);
            let light = Settings::default();
            let dark = Settings { dark: true, ..Settings::default() };
            std::thread::scope(|s| {
                s.spawn(|| light.save(stale, &saved, &path));
                s.spawn(|| dark.save(fresh, &saved, &path));
            });
            assert!(read(&path).dark, "round {round}");
        }
        let _ = std::fs::remove_dir_all(path.parent().unwrap());
    }
}
