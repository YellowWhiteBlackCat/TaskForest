# ADR-050: Wayland current-window PNG capture and PipeWire evolution

## Status

Accepted for the current GPUI release surface.

## Context

The product needs a user-triggered capture of the current TaskForest window,
written once as a PNG. GPUI's Linux `screen-capture` hook is currently a stream
adapter whose Wayland implementation reports unsupported, and the bundled
`zed-scap` path selects a default display stream rather than the current
application window. The current KDE Plasma session has XDG Screenshot Portal
version 2 with no active-window target advertised.

## Decision

1. The first product path is a Linux native adapter using KDE Spectacle's fixed
   `--activewindow --background --output` argument vector. The executable is
   optional; absence, denial, timeout, malformed PNG and provider failure remain
   distinct typed outcomes.
2. `taskmanager-platform-contract` owns the in-process renderer hook, native
   adapter trait, `WindowCaptureReceipt`, backend provenance and bounded failure
   vocabulary. `taskmanager-application` owns the `Closed | Queued | Running |
   Ready | Failed` request lifecycle.
3. `taskmanager-app-host` owns one bounded worker per process. It allocates a
   private staging file, asks the native adapter to fill it, validates the PNG
   header/dimensions, and atomically renames it to the requested `.png` path.
   Frontends never invoke a provider, inspect OS capture state, or perform file
   I/O.
4. GPUI exposes the current-window action on every product target and reports
   the committed receipt through the shared shell feedback path. The host tries
   the registered Blade readback hook first, then the selected native adapter:
   Linux uses Spectacle, Windows uses Windows.Graphics.Capture, and macOS keeps
   a typed `Unsupported` fallback when no renderer hook is available. The
   default name is `taskforest-window.png` in the process working directory.

## Evolution path

The backend enum already reserves `PortalScreenshot` and `PipeWireScreenCast`;
ScreenCaptureKit remains a future macOS backend value.
When the Screenshot Portal advertises the version-3 active-window target, add a
Portal adapter and prefer it before the Spectacle fallback; do not change the
application lifecycle or publication transaction. The official portal contract
returns a URI for one screenshot and uses target bit 8 for the active window:
<https://flatpak.github.io/xdg-desktop-portal/docs/doc-org.freedesktop.portal.Screenshot.html>.

Continuous capture is a separate stream contract, not a loop around the one-shot
button. It may reuse the bounded PNG validation, receipt provenance and app-host
shutdown/error machinery. Its native path is XDG ScreenCast source selection plus
`OpenPipeWireRemote`, with GPUI's `screen-capture`/`zed-scap` bridge consuming the
selected stream only after the Wayland feature closure and current-window source
identity are implemented:
<https://flatpak.github.io/xdg-desktop-portal/docs/doc-org.freedesktop.portal.ScreenCast.html>.

The one-shot path remains available as a fallback while a stream is unavailable;
no fake success, raw Wayland object, D-Bus connection or PipeWire handle crosses
the shared contracts.
