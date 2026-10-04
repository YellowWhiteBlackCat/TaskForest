# bevy_clipboard patch

## Role

Vendored Bevy 0.20 clipboard with the upstream public API. Linux text operations use
`wl-clipboard-rs`; Windows and macOS retain the upstream arboard backend.

## Boundary

Linux has no X11 dependency or fallback. A write returns success only after Wayland
accepted the selection source; absent protocol/compositor support returns an error.
Linux image operations return `ClipboardNotSupported`. TaskForest uses text only.
The patch is safe Rust and remains toolkit output infrastructure, separate from telemetry.

## Contract and verification

The workspace frontend dependency-closure gate rejects X11 packages. Frontend clipboard
behavior tests inject writers and verify exact payload, completed success and honest failure.
Licensing remains upstream MIT OR Apache-2.0 with both license texts retained.
