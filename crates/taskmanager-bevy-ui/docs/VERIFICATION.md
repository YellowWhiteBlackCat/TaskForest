# Verification

Commands (see also the README Contract section):

```bash
cargo check --locked -p taskmanager-bevy-ui --tests
bash scripts/accept-bevy-interactions.sh
cargo clippy --locked -p taskmanager-bevy-ui --tests
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
3. **Real Window Wayland Capture (`scripts/capture-bevy.sh`)**:
   Executes the real `taskforest-b` binary inside a nested private Wayland
   compositor (KWin + Niri). Provides authoritative pixel, DPI, font
   rasterization, and compositor presentation evidence under fail-closed gates.

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

Check the shared workspace resolution keeps `bevy_app`/`bevy_ecs` at one 0.20
version across this crate and `taskmanager-platform-runtime`
(`cargo tree -p taskmanager-bevy-ui -d`).
