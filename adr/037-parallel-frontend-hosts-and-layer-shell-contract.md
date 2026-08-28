# ADR-037: Parallel frontend hosts and a neutral layer-shell contract

Status: accepted

## Context

TaskForest shares one typed core, application state and renderer-neutral
projection across GPUI, Iced, Ratatui and Bevy. The graphical frontends now use
different windowing owners: GPUI owns a direct Wayland backend while Iced and
Bevy reach Wayland through Winit. Wayland layer-shell is a separate surface
role, not a setting that can be added to an existing `xdg_toplevel`.

Replacing the existing standalone path would remove normal desktop-window
semantics and would make unsupported compositors a product failure. A single
raw layer-shell host also cannot own all three toolkit event loops and surface
lifetime models without leaking toolkit or native objects across the shared
contract.

## Decision

1. Keep the existing standalone host path for every frontend and supported OS.
2. Add a parallel layer-shell host path inside each graphical frontend when that
   frontend is ready; do not duplicate core, application, shell projection,
   theme or page ownership.
3. Use `taskmanager-app-host` as the composition seam for the public,
   toolkit-neutral `WindowPresentation` and `LayerShellSpec` value contract.
4. Keep raw Wayland protocol objects, event queues, surface lifetimes and
   renderer handles inside the selected frontend adapter or an explicitly
   audited native boundary.
5. Treat role selection as per-surface. A process may eventually compose a
   standalone main window and a layer-shell panel.
6. Probe layer-shell at runtime. The default policy may fall back to the
   existing standalone host; strict callers receive a typed unavailable result.
7. Layer-shell adapters must expose truthful limitations. Normal-window
   operations that the role cannot implement are unavailable, never silent
   success.

## Consequences

- Existing desktop behavior remains stable while layer-shell is introduced.
- GPUI can add its adapter near the direct Wayland backend; Iced and Bevy can
  independently choose a runner/plugin, fork or third-party shell strategy.
- The shared contract stays free of GPUI, Iced, Bevy, Ratatui and Wayland types.
- The three layer-shell implementations may temporarily differ in protocol
  coverage and capability reporting; parity is proven through behavior rather
  than forced by a shared raw backend.
- Layer-shell remains a Linux/Wayland presentation option, not a replacement
  for the main desktop window on every platform.

## Verification

The contract is verified with safe headless tests for validation, fallback and
role separation. Each native adapter must separately prove the Wayland
configure/ack lifecycle, resize, input, output disconnect, `closed` and GPU
surface lifetime on a real compositor. Public architecture and crate READMEs
must continue to state that compilation or fixture success does not prove a
target compositor.
