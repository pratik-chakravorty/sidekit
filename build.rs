//! Embeds the app icon and version metadata into the Windows executable.
//! GPUI loads the window/taskbar icon from resource id 1, which `set_icon` writes.
//!
//! Also sets `SIDEKIT_VERSION`, the version shown in the app, from the git tag
//! so it cannot drift from releases: `SIDEKIT_VERSION` in the environment
//! (CI passes the release tag) wins, then `git describe --tags`, then
//! Cargo.toml. A leading `v` is dropped either way.

use std::process::Command;

fn main() {
    let version = version();
    println!("cargo:rustc-env=SIDEKIT_VERSION={version}");

    println!("cargo:rerun-if-changed=assets/icon/icon.ico");
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("windows") {
        let mut res = winresource::WindowsResource::new();
        res.set_icon("assets/icon/icon.ico")
            .set("ProductName", "SideKit")
            .set("FileDescription", "SideKit")
            .set("CompanyName", "SideKit")
            .set("ProductVersion", &version);
        if let Err(e) = res.compile() {
            println!("cargo:warning=could not embed the Windows icon: {e}");
        }
    }
}

fn version() -> String {
    println!("cargo:rerun-if-env-changed=SIDEKIT_VERSION");
    if let Some(v) = std::env::var("SIDEKIT_VERSION").ok().filter(|v| !v.is_empty()) {
        return v.trim_start_matches('v').to_string();
    }
    // New commits and tags change these, so describe runs again.
    if let Some(dir) = git(&["rev-parse", "--git-dir"]) {
        for f in ["HEAD", "refs/tags", "refs/heads", "packed-refs"] {
            println!("cargo:rerun-if-changed={dir}/{f}");
        }
    }
    // On a tag: "0.1.2". Past it: "0.1.2-3-g1a2b3c4" (3 commits later).
    git(&["describe", "--tags"])
        .map(|v| v.trim_start_matches('v').to_string())
        .unwrap_or_else(|| std::env::var("CARGO_PKG_VERSION").unwrap())
}

fn git(args: &[&str]) -> Option<String> {
    let out = Command::new("git").args(args).output().ok()?;
    let s = String::from_utf8(out.stdout).ok()?;
    (out.status.success() && !s.trim().is_empty()).then(|| s.trim().to_string())
}
