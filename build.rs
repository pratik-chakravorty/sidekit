//! Embeds the app icon and version metadata into the Windows executable.
//! GPUI loads the window/taskbar icon from resource id 1, which `set_icon` writes.

fn main() {
    println!("cargo:rerun-if-changed=assets/icon/icon.ico");
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("windows") {
        let mut res = winresource::WindowsResource::new();
        res.set_icon("assets/icon/icon.ico")
            .set("ProductName", "SideKit")
            .set("FileDescription", "SideKit")
            .set("CompanyName", "SideKit");
        if let Err(e) = res.compile() {
            println!("cargo:warning=could not embed the Windows icon: {e}");
        }
    }
}
