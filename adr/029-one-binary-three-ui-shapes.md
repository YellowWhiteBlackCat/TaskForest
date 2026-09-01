# ADR-029: One binary, three UI shapes — feature-gated frontends and a unified CLI

Status: superseded by ADR-051 (the unified CLI survives in `taskmanager-cli`; the feature matrix, `build.rs` arbiter, and one-binary dispatch are deleted)

## Context

The project compiles three frontends — the GPUI desktop shell
(`crates/taskmanager-gpui/src/gpui_app`),
the ratatui TUI (`taskmanager-tui`) and the iced frontend (`taskmanager-iced`)
— from one workspace on one neutral core (core/application/shell/theme/
ui-contract, ADR-017/026/027/028). Until now each frontend was its own
binary: `taskmanager`, `taskmanager-tui`, `taskmanager-iced`.

The product matrix is 3 frontends × 3 platforms (Linux/macOS/Windows) = 9
shapes. The platform dimension was already conditional compilation
(`cfg(target_os)` in `taskmanager-platform-native`); the frontend dimension
was three independent binaries. That left the CLI split: only the GPUI
binary had `--json`/`--suggest-thresholds`/`--gpu-engines`; the TUI and iced
binaries each had their own ad-hoc flags. This ADR unifies the CLI and
makes the frontend dimension conditional compilation too: **one binary,
three feature-gated shapes**.

## Decision

### Features (root `taskmanager` package)

- `ui-gpui` (default) — the `taskmanager-gpui` desktop frontend. That peer
  crate owns `gpui`, `taskmanager-ui`, `taskmanager-icons`,
  `taskmanager-theme` (with its `gpui` binding feature), `taskmanager-assets`,
  and `taskmanager-accessibility-linux`; the root feature is only the optional
  dispatch edge.
- `ui-tui` — `taskmanager-tui` (ratatui), its binary target removed.
- `ui-iced` — `taskmanager-iced` (iced 0.14), its binary target removed.

Exactly ONE `ui-*` feature is enabled per build; `build.rs` fails fast with
a clear message on zero or multiple (it also gates the Windows icon embed to
the GPUI shape). `default = ["hardware-all", "ui-gpui"]`; the other shapes
build with `--no-default-features --features ui-tui|ui-iced`.

The UI-neutral CLI modes are compiled in every shape: `--json`,
`--suggest-thresholds`, `--gpu-engines`, `--help`, plus the new `--demo`
(fixture data, no host I/O — supported by the TUI/iced shapes; the GPUI
shape reports it is not yet supported) and `--snapshot [W H]` (headless
terminal text-frame evidence, TUI shape only, reporting "not supported"
elsewhere). `--app-id` remains GPUI-only in effect but is accepted by every
shape so the CLI surface is uniform.

### Frontend crates

- `taskmanager-tui` was already lib-only; it keeps `run_live`/`run_demo`/
  `snapshot_text`.
- `taskmanager-iced` loses its `[[bin]]`; its main becomes
  `taskmanager_iced::run(demo: bool) -> iced::Result` (`src/run.rs`), keeping
  the platform-client hand-off and window `application_id`.
- `src/main.rs` dispatches `CliMode::Gui` through the root `frontend` module;
  the GPUI composition edge is `taskmanager_gpui::run`, owned by the
  `taskmanager-gpui` crate.

### Shared native app host

The platform and UI axes remain orthogonal through one toolkit-neutral peer
crate, `taskmanager-app-host`. It owns the native configuration/history paths,
the `NativePlatformRuntime` factory, and `PlatformClient` construction. The
three UI launchers receive this seam; they do not depend directly on
`taskmanager-platform-native` or reimplement native path selection. Their
`run`/`runtime` modules still own the toolkit-specific event loop, window or
terminal setup, subscriptions, and renderer state.

### Tests

- `tests/gui.rs` and `tests/performance.rs` are gated `#![cfg(feature = "ui-gpui")]`
  (they exercise the GPUI frontend).
- `tests/logic.rs`: the 16 modules that reference `gpui_app` are gated with
  `#[cfg(feature = "ui-gpui")]`; architecture gates that must run in every
  shape stay ungated.
- The dependency firewall tracks the three optional frontend edges
  (`taskmanager-gpui`/`taskmanager-tui`/`taskmanager-iced`) on the root package,
  and the
  hardware-artifact gates assert the new default feature set.

The CI matrix keeps this invariant executable: the blocking Linux job tests the
default `hardware-all,ui-gpui` shape, the NVIDIA fallback explicitly adds
`hardware-all,nvidia,ui-gpui`, and a non-GPUI matrix runs
`--workspace --all-targets` plus nextest for both `ui-tui` and `ui-iced`. A
zero-UI invocation is therefore a CI configuration error, not an implied
headless product shape.

### Rules

1. **Exactly one `ui-*` feature per artifact.** build.rs enforces it; a
   shape with two UI toolkits linked is a build error, not a warning.
2. **The UI-neutral CLI is identical in every shape.** No per-frontend flag
   dialects: new modes go into `src/cli.rs`, and frontends that cannot
   implement a mode report "not supported" honestly instead of inventing a
   different flag.
3. **Frontends stay feature-gated crates.** `ui-tui`/`ui-iced` link their
   own toolkit chains and nothing else (verified: the TUI artifact contains
   no gpui/wgpu/iced symbols); the neutral core is shared by all shapes.
4. **Release artifacts are 9 shapes** (3 UI features × 3 platforms), each a
   separate build of the one binary; the platform dimension remains
   `cfg(target_os)` selection inside `taskmanager-platform-native`.
5. **Native composition has one owner.** `taskmanager-app-host` is the shared
   composition seam between the selected UI crate and
   `taskmanager-platform-native`; there are three thin toolkit launchers, not
   three independent application graphs or nine platform/UI crates.

## Consequences

- One CLI surface: scripts and users invoke `taskmanager --json` (any
  shape), `taskmanager --demo` (TUI/iced), `taskmanager --snapshot` (TUI);
  capture scripts build the shape with `--no-default-features --features
  ui-tui|ui-iced` and run `target/debug/taskmanager`.
- Each artifact is smaller and cleaner (no unused UI chain linked); the
  dependency firewall now models the feature-gated edges explicitly.
- Pixel evidence is frontend-scoped: changing `taskmanager-gpui` invalidates
  only the GPUI receipt; Iced/TUI receipts are not re-captured merely because
  the root dispatch or another frontend changed. The source-manifest generator
  follows the selected production dependency graph and excludes dev-only test
  helpers and target-inapplicable OS adapters.
- Developers switching shapes recompile the other UI chain (no sccache
  shortcut); the shared `target/` keeps both shapes' artifacts.
- Current references: `docs/ARCH.md`, `docs/UI_COMPONENT_ARCHITECTURE.md`, and
  the three frontend crate READMEs.

## Current ownership: GPUI crate

The GPUI implementation is now a first-class peer package,
`crates/taskmanager-gpui`. It owns `gpui_app`, the GPUI composition edge, and
its crate-local unit tests. The root `taskmanager` package retains the single
binary host, CLI-neutral modules, and the `ui-gpui` feature edge only. This
does not create a second binary or change the three-shape feature contract;
it makes the ownership boundary explicit and lets GPUI source provenance be
scoped independently from Iced and TUI.

## Current ownership: shared app host

The native composition seam is now `crates/taskmanager-app-host`. The root
binary and all three UI shapes use its typed `NativeAppHost` instead of
constructing `NativePlatformRuntime`, native config paths, and history paths in
parallel. This changes no renderer contract and does not merge the three
toolkit event loops; it only removes duplicated application wiring from the
9-shape matrix.
