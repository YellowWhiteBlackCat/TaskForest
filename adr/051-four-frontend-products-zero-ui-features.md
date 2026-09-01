# ADR-051: Four frontend products, zero UI features

Status: accepted (supersedes ADR-029's feature matrix; amends ADR-026's
feature-gated-bindings rule and ADR-046's one-binary note)

## Context

ADR-029 unified the CLI and made the frontend dimension conditional
compilation: one root `taskmanager` binary, three mutually exclusive `ui-*`
features, a `build.rs` arbiter enforcing "exactly one", and `src/frontend.rs`
dispatching through `#[cfg]`. The Bevy frontend (ADR-046) joined as a peer
crate but stays outside that matrix with its own ad-hoc `--demo`-only binary.

Costs observed in the 0.1.0 line:

- The root package is a dispatch shell: ~30 feature `cfg`s across `main.rs`/
  `cli.rs`/`frontend.rs`, plus a build-script feature arbiter. Every new
  frontend either joins the matrix or becomes a second-class side binary.
- Shared lower layers leaked UI conditionals: `taskmanager-theme` compiled
  `gpui.rs`/`iced.rs` behind crate features (an orphan-rule workaround), and
  `taskmanager-icons` enabled a `gpui` feature by default. The lower layers
  knew how many frontends exist — a dependency-direction violation.
- A product's real dependency closure was decided by feature unification, not
  by a manifest: proving "a TUI build contains no gpui" required simulating
  feature sets (`cargo tree --no-default-features --features ui-tui`).
- The platform axis already has the correct shape: per-OS crates plus ONE
  `cfg(target_os)` composition point (`taskmanager-platform-native`,
  ADR-009). Conditional compilation belongs to that axis because a build
  targets exactly one OS. A build does not target "one UI" — the four
  frontends are four independently shippable products.

## Decision

1. **cfg is a platform-axis mechanism only.** A frontend is a crate and a
   binary, never a feature. The workspace carries zero `ui-*` features; no
   shared crate may gate code on a frontend identity.
2. **Four products, four binaries**, each a self-contained product crate:
   `taskforest-g` (`taskmanager-gpui`), `taskforest-i` (`taskmanager-iced`),
   `taskmanager-tui` (`taskmanager-tui`), `taskforest-b`
   (`taskmanager-bevy-ui`). Each `[[bin]]` is thin: parse nothing, decide
   nothing — it hands its capability set to the shared CLI harness.
3. **`taskmanager-cli` is the shared CLI composition crate** (new). It owns
   argv parsing, the UI-neutral modes (`--json`, `--suggest-thresholds`,
   `--gpu-engines`, `--memory-smbios`, `--package-power`, `--msr`, `--help`),
   tracing initialization, and the help text. Shape differences are injected
   as plain values: a required `run_gui` handler plus optional
   `snapshot_text` and `capture_window` handlers. Availability is reported
   from the capability value (help omits or reports "unsupported"), never
   from `cfg`.
4. **Shared layers lose every frontend conditional.** `taskmanager-theme`
   ships no toolkit bindings: the `gpui`/`iced` binding modules move into the
   respective frontend crates as plain adapter functions (free functions need
   no trait impl, so the orphan rule no longer forces a feature gate).
   `taskmanager-icons` drops the default `gpui` feature; its gpui adapter
   moves into the gpui crate.
5. **The root package becomes `taskmanager-gates`**, the cross-crate
   conformance host (the `tests/logic` and `tests/performance` gate suites
   and their fixtures). It ships no binary, no product feature, and no
   public API. Product-scoped tests move to their owners: the GPUI GUI
   interaction suite lives in `taskmanager-gpui`.
6. **`hardware-all`/`nvidia` stay app-host features**, surfaced by each
   product manifest that links the native stack. The release-build
   `hardware-all` guard and the Windows icon embed move into the product
   crates' `build.rs`/lib gates. CI builds each product as an independent
   job; local development addresses products by `-p` (cargo aliases provided
   in `.cargo/config.toml`), never by re-selecting features.

## Consequences

- A product's dependency closure is its manifest, statically checkable:
  `cargo tree -p taskmanager-tui` proving gpui absence needs no feature
  simulation. The closure gate tests assert per-product manifests.
- Adding a fifth frontend is additive: one crate, one `[[bin]]`, one handler
  struct. No feature arbiter, no root dispatch edits, no shared-layer gates.
- The unified CLI surface is enforced by shared code instead of a build
  failure: every product prints the same help for the modes it carries, and
  honestly reports the capabilities it lacks.
- Toolkit adapters in frontend crates are the only place toolkit types meet
  theme tokens; `taskmanager-theme` regains unconditional neutrality (it
  compiles zero toolkit code on every target).
- The root dispatch shell, `build.rs` feature arbiter, `ui-*` features, and
  `src/frontend.rs` are deleted in the same change (hard cutover; no
  compatibility feature, alias, or shim remains).
- Current references: `docs/ARCH.md` §2 and §6, the four frontend crate
  READMEs, and `crates/taskmanager-cli/README.md`.
