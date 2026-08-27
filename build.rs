//! Build script for the `taskmanager` binary.
//!
//! Responsibilities:
//! 1. Enforce the exactly-one-UI invariant (ADR-029): `ui-gpui` / `ui-tui` /
//!    `ui-iced` are mutually exclusive product shapes. A build that enables
//!    zero or two+ UI features fails fast with a clear message instead of
//!    producing a shape that does not exist.
//! 2. Embed the TaskForest app icon into the Windows executable (GPUI shape
//!    only) so its taskbar / alt-tab / titlebar + Explorer file icon show the
//!    real logo instead of a generic icon. See `packaging/windows/taskmanager.rc`
//!    for why the icon is pinned to resource id 1 (gpui 0.2.2 reads exactly
//!    that).
//!
//! Target-gated: `embed_resource` drives rc.exe, windres or LLVM-RC only when
//! Cargo is producing the Windows GPUI shape. Native and cross builds therefore
//! share the same fail-closed icon contract; non-Windows targets do not invoke
//! a resource compiler.

fn main() {
    let ui_features: Vec<&str> = [
        (
            "ui-gpui",
            std::env::var_os("CARGO_FEATURE_UI_GPUI").is_some(),
        ),
        ("ui-tui", std::env::var_os("CARGO_FEATURE_UI_TUI").is_some()),
        (
            "ui-iced",
            std::env::var_os("CARGO_FEATURE_UI_ICED").is_some(),
        ),
    ]
    .into_iter()
    .filter_map(|(name, enabled)| enabled.then_some(name))
    .collect();
    match ui_features.as_slice() {
        [single] => {
            println!("cargo:rustc-env=TASKMANAGER_UI_FEATURE={single}");
        }
        [] => {
            panic!(
                "exactly one UI feature must be enabled (ADR-029): \
                 pass --features ui-gpui, ui-tui or ui-iced"
            );
        }
        many => {
            panic!(
                "exactly one UI feature must be enabled (ADR-029); got: {}",
                many.join(", ")
            );
        }
    }

    let windows_target = std::env::var_os("CARGO_CFG_TARGET_OS").is_some_and(|os| os == "windows");
    if windows_target && std::env::var_os("CARGO_FEATURE_UI_GPUI").is_some() {
        // The app icon is part of the installed product identity, not a
        // best-effort decoration. Refuse to produce a Windows GPUI executable
        // when rc.exe/windres/LLVM-RC cannot compile or link resource id 1;
        // otherwise the MSI would install a generic taskbar/Alt-Tab icon.
        embed_resource::compile("packaging/windows/taskmanager.rc", embed_resource::NONE)
            .manifest_required()
            .unwrap_or_else(|error| {
                panic!("TaskForest Windows icon resource is required: {error}")
            });
    }
    println!("cargo:rerun-if-changed=packaging/windows/taskmanager.rc");
    println!("cargo:rerun-if-changed=packaging/windows/taskmanager.ico");
}
