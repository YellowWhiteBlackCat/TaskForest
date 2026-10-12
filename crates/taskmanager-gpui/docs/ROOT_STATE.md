# Root state ownership

- `root/projection_materialization.rs` owns the revision-keyed read model of
  the shell's privately held system store. Named platform-batch systems replace
  each inventory's rows and source status together; renderers never fold raw
  events.
- `root/projection_caches.rs` owns immutable process, inventory, history and
  Properties memos. Builders run outside `RefCell` borrows, and consumers
  receive owned `Rc` snapshots instead of mutable guards.
- `root/window_surface.rs` composes local surfaces with application-owned
  `DirectTrackState::interaction`. Properties and dangerous confirmations
  replace each other; only a matching frozen Confirm transition emits an
  effect.
- `root/process_insights_ui.rs` correlates one frozen target and application
  revision. The accepted `ProjectedProcessInsights` retains seven independent
  Pending/Current/Unavailable facets. Partial replies, late enrichment and
  missing reasons stay distinct. Refresh preserves the original frozen target
  and refuses duplicate work while any facet collects. PID reuse cannot publish
  into it.
- `process_insights/view/facets.rs` renders the selected shared facet. Long
  lists keep complete data and true totals; normal page controls reach entries
  beyond the bounded row materialization window. No whole-snapshot error masks
  another facet's available data or distinct missing reason.
- `root/render/surfaces.rs` maps the application surface authority to one
  active renderer. `render/transients.rs` then composes feedback, pause and
  tooltips.
- `root/responsive.rs` derives one frame/content budget. Pages receive explicit
  slots after shell chrome is deducted; width and vertical capacity stay
  separate.
- `root/presentation_preferences.rs` owns persisted appearance, units, device
  and sidebar policy as one immutable snapshot. Per-axis fingerprints
  invalidate only affected projections; page, focus and runtime handles remain
  local state.
- `root/startup/config_sync.rs` drains external configuration before
  submitting base-aware drafts without file I/O. Revisions replace persisted
  fields together and never reset navigation, runtime handles or active history
  requests.
- `root/history_runtime.rs` owns Disabled/Connecting/Unavailable/Active reader
  phases. Enable submits a correlated app-host request; disable releases
  replay. Stale completions cannot undo disable or replace a newer connection.
- `perf_views/history_replay.rs` adapts the application replay lifecycle and
  each accepted row allocation into stable GPUI graph handles. Filesystem work
  and writer teardown belong to app-host's bounded worker.
- `root/keyboard.rs` normalizes each key once and routes through the
  shared/local surface policy; both input paths read one cached projection,
  as the repository invariant requires.
- `root/dialog_scroll_state.rs` owns separate long-form body handles.
  Persistent actions stay outside the scrolling coordinate tree.

Service Details reads shared dependency and query-keyed log lifecycles. Export
completion enters shell feedback; it owns no worker, file writer or mirrored
service target. Users session controls arm the shared
`SessionControlConfirmation` from one complete provider identity; Confirm
rechecks the current projection, and every close path clears the request without
an effect. Process control, affinity, resource limits, SMART jobs and GPU engine
requests likewise use application-owned sessions with frozen identity and
storage generation checks. Late and duplicate completions cannot create
feedback.

The Run dialog's `TextInputState` is the command-text authority. Launch, reveal
and URL open use the direct track's typed shell-action session. Snapshot,
diagnostic and PNG requests use named app-host clients. The PNG host tries
Blade readback before its selected native adapter; unsupported backends remain
typed failures.
