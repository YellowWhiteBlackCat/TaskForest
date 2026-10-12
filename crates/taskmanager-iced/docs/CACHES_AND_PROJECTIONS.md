# Caches, projections, and runtime seams

- `src/app/viewport_state.rs` owns the window size and the six independent
  Applications, App-history, Services, Startup, Users and Performance-rail
  scroll lifetimes. A validated resize invalidates every observed viewport
  atomically; views receive only offsets, bounds and widget ids.
- `src/app/projection_caches.rs` is the private owner of the eight renderer-only
  memos. Process performance keys by pid/ring revision; graph series by history
  revision/capacity/device identity; durable app-history rows by accepted
  replay request; Services, Startup and Users by independent inventory
  revisions; the Performance rail by system/history revisions, visible window,
  device selector order and unit choices. Views receive immutable `Rc`
  snapshots rather than `RefCell` guards. Scroll and viewport state remain
  outside because their interaction lifetime is independent.
- Canonical system facts live only in `ShellApp`'s private
  `SystemProjectionStore`; Iced reads `projection()` and submits batches or
  named semantic reducers. Demo/capture/tests use the shell's typed fixture
  facts instead of assigning projection fields. Persistent Performance and
  application reviews use the same accepted query, window and refresh
  lifecycle. Recorded review fixes its controls around one bounded body and
  preserves signed magnitude coordinates and axis-cache identity. Capture
  writes private input files and waits for real worker rows plus their
  presented frame; no renderer seeds replay data.
- `src/app/history_replay.rs` adapts the application request lifecycle and the
  exhaustive `Disabled | Connecting | Unavailable | Active` reader state.
  Runtime enable is a non-blocking, latest-request-wins connector transition;
  disable immediately closes replay. The top History page consumes the shared
  durable 1h/24h/7d application projection, while Performance filters
  application series from its device replay. The tick drains clients and never
  reads files.
- `src/tray.rs` owns the Iced-local tray spec/action mapping;
  `src/app/runtime.rs` owns the non-blocking single-instance activation pump.
  Production uses the `TaskForestI` identity, keeps one primary
  window/process, and minimizes to the tray when the native tray is available;
  restore explicitly unminimizes, requests focus and asks for attention. Typed
  tray failure degrades to a window-only process. Demo/capture bypasses native
  lifetime resources.
- Keyboard, native-window and tray exits submit distinct shell `QuitReason`
  values; Iced only reads `should_quit()`. Footer activity, settings failures,
  export and clipboard results all use the shell's typed feedback authority;
  Iced keeps no parallel footer feedback strings.
- Service Details projects the same application dependency/log lifecycles as
  GPUI. Queue admission begins before runtime submission, filter/cursor changes
  create a new query generation, and only its returned request can fold. Log
  timestamps come from the tick/event cache; export status uses shell feedback.
- Snapshot and service-log export use app-host-injected named clients. Message
  handlers only submit immutable typed requests; the tick drains correlated
  completions into shell feedback, with no renderer filesystem/thread owner.
- Service-details dependency chips iterate the immutable typed `ServiceDeps`
  projection, while inventory rows keep `ServiceItem` relations read-only;
  legacy JSON strings never enter Iced state as a parallel authority.
- GPU rail/detail projections fix aggregate utilization as the primary history
  and keep canonical scalar/VRAM facts simultaneous. Standard geometry adds
  every available engine graph; compact geometry is an elastic, non-scrolling
  aggregate surface. Engine collection remains a typed capability action rather
  than display state. System projects each successful NPU inventory as
  identity, driver, aggregate/engine utilization and dedicated/shared memory
  cards; unavailable observations remain explicit and no trend is invented
  without history authority.
- `../../scripts/capture-iced.sh` and its validator own pixel evidence, not
  product state.

The Applications projection presents the same category-first tree contract as
the other products: Applications, Background and Uncategorized are first-level
buckets. Applications inserts a selectable PID-less application aggregate
before each recursive process tree; the other buckets expand directly to real
process rows. Legacy grouping tokens are normalized when configuration loads.
Small/Standard/Large UI size uses the application scale hook on top of
compositor DPI; density remains a separate table-whitespace preference.
