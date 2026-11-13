//! Windows: embed the app icon, version information and an application manifest
//! (per-monitor DPI awareness, Common Controls 6 for the native file dialogs) so the
//! release `.exe` is a normal Windows program.

fn main() {
    println!("cargo:rerun-if-changed=build.rs");
    println!("cargo:rerun-if-changed=../../packaging/icons/kidpix-revival.ico");
    println!("cargo:rerun-if-changed=../../packaging/windows/kidpix-revival.manifest");
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() != Ok("windows") {
        return;
    }
    let version = std::env::var("CARGO_PKG_VERSION").unwrap_or_else(|_| "0.1.0".into());
    let mut res = winresource::WindowsResource::new();
    res.set_icon("../../packaging/icons/kidpix-revival.ico")
        .set_manifest_file("../../packaging/windows/kidpix-revival.manifest")
        .set("ProductName", "Kid Pix Revival")
        .set("FileDescription", "Kid Pix Revival")
        .set("InternalName", "kidpix-revival")
        .set("OriginalFilename", "kidpix-revival.exe")
        .set("LegalCopyright", "Kid Pix Revival contributors (MIT OR Apache-2.0). Kid Pix is a trademark of its owners.")
        .set("ProductVersion", &version)
        .set("FileVersion", &version);
    // cross builds from Linux use the mingw resource compiler
    if std::env::var("CARGO_CFG_TARGET_ENV").as_deref() == Ok("gnu") && !cfg!(windows) {
        res.set_windres_path("x86_64-w64-mingw32-windres").set_ar_path("x86_64-w64-mingw32-ar");
    }
    if let Err(e) = res.compile() {
        println!("cargo:warning=could not embed the Windows icon/manifest: {e}");
    }
}
