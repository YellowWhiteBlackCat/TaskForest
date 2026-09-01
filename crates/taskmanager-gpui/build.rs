//! Build script for the `taskforest-g` product binary (ADR-051).
//!
//! Embeds the TaskForest app icon into the Windows executable so its taskbar /
//! alt-tab / titlebar + Explorer file icon show the real logo instead of a
//! generic icon. See `packaging/windows/taskmanager.rc` for why the icon is
//! pinned to resource id 1 (gpui 0.2.2 reads exactly that).
//!
//! Target-gated: `embed_resource` drives rc.exe, windres or LLVM-RC only when
//! Cargo is producing the Windows GPUI product. Native and cross builds
//! therefore share the same fail-closed icon contract; non-Windows targets do
//! not invoke a resource compiler.

fn main() {
    let windows_target = std::env::var_os("CARGO_CFG_TARGET_OS").is_some_and(|os| os == "windows");
    if windows_target {
        // The app icon is part of the installed product identity, not a
        // best-effort decoration. Refuse to produce a Windows GPUI executable
        // when rc.exe/windres/LLVM-RC cannot compile or link resource id 1;
        // otherwise the MSI would install a generic taskbar/Alt-Tab icon.
        embed_resource::compile(
            "../../packaging/windows/taskmanager.rc",
            embed_resource::NONE,
        )
        .manifest_required()
        .unwrap_or_else(|error| panic!("TaskForest Windows icon resource is required: {error}"));
    }
    println!("cargo:rerun-if-changed=../../packaging/windows/taskmanager.rc");
    println!("cargo:rerun-if-changed=../../packaging/windows/taskmanager.ico");
}
