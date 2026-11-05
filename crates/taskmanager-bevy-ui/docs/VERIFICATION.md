# Verification

Commands (see also the README Contract section):

```bash
cargo check --locked -p taskmanager-bevy-ui --tests
bash scripts/accept-bevy-interactions.sh
cargo clippy --locked -p taskmanager-bevy-ui --tests
bash scripts/quality/bevy-optical-gpu-gate.sh   # discovers and runs the ignored GPU proof; writes a fail-closed receipt
python3 scripts/quality/bevy_optical_capability_guard.py
python3 scripts/quality/bevy_bsn_guard.py --mode enforce   # 100% Scene law
python3 scripts/quality/bevy_rem_guard.py --mode enforce   # rem sizing axis
python3 scripts/quality/bevy_world_access_guard.py         # resource access scope
bash scripts/capture-bevy.sh                               # real pixels, live Wayland
```

The capture validator is fail-closed on app_id, PID/window identity, PNG,
markers, source provenance and current worktree.

## Verification Hierarchy

Testing follows a strict three-tier verification contract:

1. **Pure Headless ECS Seams (`tests/headless/`)**:
   Runs on `MinimalPlugins` without window or GPU. Verifies projection folding
   (`drain.rs`), command routing and input normalization (`input.rs`), destructive
   gate state machines (`confirmation.rs`), popovers and outside-press forwarding
   (`menu_popover_tests.rs`), palette token resolution, and semantic accessibility
   trees (`semantic.rs` AccessKit projection). Fast, deterministic, and isolated.
2. **Headless Layout & Sizing Seams**:
   Verifies Taffy layout, responsive breakpoints, scroll areas, and edge
   protection without an OS window (e.g. `scroll_and_motion_parity.rs`). Uses
   `UiPlugin` + `TextPlugin` + `WindowPlugin { primary_window: None, .. }` +
   headless primary window/camera entity. Asserts measured node geometry
   (`ComputedNode::size` and `UiGlobalTransform`) while pinning time step
   via `TimeUpdateStrategy::ManualDuration(Duration::from_millis(16))` to prevent
   wall-clock race conditions.
   The window composition also proves camera ownership here: the interactive
   shell resolves to `PresentationCamera`, the isolated backdrop tree resolves
   to `BackdropCaptureCamera`, and offscreen target dimensions/DPI follow the
   primary window without changing the page tree. Invalidation tests verify
   the offscreen camera stays dormant without a live optical surface, renders
   exactly one frame per lower-composite change (fold, resize, palette, route)
   when a surface exists, and stops rendering once mirrored. Glass binding tests verify
   uniform projection, invalid placement, material attachment and typed
   fallback. Attachment does not prove GPU readiness. The capture camera
   remains inactive while the capability is unsupported.
3. **Real Window Wayland Capture (`scripts/capture-bevy.sh`)**:
   Executes the real `taskforest-b` binary inside a nested private Wayland
   compositor (KWin + Niri). Provides authoritative pixel, DPI, font
   rasterization, and compositor presentation evidence under fail-closed gates.
   The `process-context-menu(-compact)` rows open the real Applications action
   menu, and their readiness gate refuses to emit a frame unless the whole
   glass card is inside the window — a popover that flips past an edge and
   clips its first verbs cannot be recorded as a passing capture.

## Testing Disciplines & Anti-False-Pass Rules

- **Never Fabricate Final State**: Tests must inject events at official entry
  points (`KeyboardInput` via `MessageWriter`, or `PointerPress` for picking).
  Never insert "already confirmed", "task ended", or "connected" components
  directly into the World to make a test pass.
- **Respect Lifecycle & Deferred Boundaries**:
  - `Commands::spawn`, entity despawn, and `commands().trigger(...)` resolve at
    ApplyDeferred boundaries. Always step `app.update()` (often two updates for
    dependent reactions) before asserting queries or observer effects.
  - Never call `world.flush()` as a shortcut to force immediate state; flushing
    alters production scheduling semantics.
  - Use `run_system_once` only for narrow, one-shot input injection (such as
    writing into `MessageWriter<KeyboardInput>`). Never chain `run_system_once`
    to test continuous systems with `Local`, change detection, or timer state.
- **Explicit Time Control**: Any test exercising animations, easing curves,
  motion progress, or timer-dependent UI (e.g. alerts or toast fading) must
  inject `TimeUpdateStrategy::ManualDuration`. Never rely on wall-clock time
  (`std::time::Instant`).
- **Scene Mount Readiness**: Scene entities do not exist during declaration.
  Assert mount completion using Bevy 0.20's bottom-up `Ready` event, never by
  polling for incremental component additions.
- **Hit-Testing & Picking Pass-Through**: When testing button clicks or popover
  outside-clicks, ensure child nodes (icons, text) carry `Pickable::IGNORE` so
  pointer events reach the intended interactive surface.

The opt-in GPU test requires a working adapter and is run through
`bevy-optical-gpu-gate.sh`, never through an ad-hoc ignored-test command. It
renders the production WESL and material against a controlled lower scene and
checks blur, edge refraction, dispersion, specular, grain, opacity, rounded
clipping, sharp foreground and a changed background. It captures only final
output targets and prints `optical-gpu receipt:` with the adapter identity,
operation set and proven limits. The gate writes
`target/optical-gpu-evidence/receipt.json`; `PROVEN` requires hardware, while
`NO_ADAPTER` and `SOFTWARE_ADAPTER` are explicitly recorded as `PAINTED`.
`TM_OPTICAL_GPU_GATE=1` rejects both degraded states. The capability itself
is derived from the live `RenderAdapterInfo` in
`src/glass/optical/capability.rs`, not from a palette constant. High-contrast,
headless, software-adapter and no-adapter compositions fail closed to typed
fallbacks. The current proof covers one top-level glass layer; nested ordering
is deliberately unproven and remains a typed fallback.

Check the shared workspace resolution keeps `bevy_app`/`bevy_ecs` at one 0.20
version across this crate and `taskmanager-platform-runtime`
(`cargo tree -p taskmanager-bevy-ui -d`).
