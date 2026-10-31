# Module notes

Detail behind the README module map:

- `src/app.rs` — the frontend-owned route model (nine pages), the nav rail,
  page mounting, and `ShellTrack`: the SystemParam every page reads the
  folded projection through (the page-agent data entry).
- `src/input.rs` — the real-input seam: Bevy keyboard events forwarded
  through the shell's own routers, the frontend-owned route chords (the
  shell page always follows the visible page), the Dialog-scope Enter
  mapping, the per-inventory action-menu open chords, the `PendingEffects`
  effect bridge to the drain, and the one-shot quit forward.
- `src/window_surface.rs` owns the single About/system-information/setup/
  diagnostic surface. State notifications coalesce before layout; header and
  complete action groups stay fixed around one bounded, scrollable body.
- `src/first_run_modal.rs` projects the application setup controller: silent
  startup discovery, explicit Settings entry, typed native actions and
  metadata copy.
- `src/about_modal.rs` reviews shared application build metadata, opens the
  repository and switches to independent system information or diagnostics.
- `src/system_information_modal.rs` freezes shared groups from the cached
  projection, scrolls complete values and copies exactly the displayed review.
- `src/text_selection.rs` queues text output for Bevy's native clipboard;
  success feedback requires a completed system write. Linux uses the strict
  Wayland `bevy_clipboard` patch; Windows/macOS retain upstream native output.
- `src/confirmation.rs` — the shell's armed destructive-action gate rendered
  as one modal under the app shell root, with typed confirm/dismiss paths
  and republished gate transitions. A tree batch draws the FROZEN collateral
  (the affected root and every descendant below it, indented) from the armed
  intent — never from the live process table, so a later refresh cannot
  change the list.
- `src/focus_visible.rs` binds actual widget Tab navigation to toolkit focus,
  paints one keyboard ring and reveals focused controls through the owned
  scroll area. Settings zero dimming uses the persisted config field and
  leaves missing values distinct.
- `src/semantic.rs` — the accessibility seam: the ui-contract
  `SemanticSnapshot` (revision-keyed) plus accessibility row nodes.
- `src/drain.rs` — the per-frame `PreUpdate` event-port drain; folds platform
  batches into the shell and triggers `ShellProjectionFolded` (the pages'
  data-refresh event).
- `src/palette.rs` — the theme-token → bevy adapter; the only place tokens
  become bevy colors/type metrics.
- `src/pages/processes/properties_modal/` — the properties modal's surfaces:
  `backdrop.rs` (the modal's frost contract over a translucent viewport),
  `tabs.rs` (the sliding tab indicator, a typed frame-stepped motion),
  `memory.rs` (the PSS/USS/Swap segment comparison bar), `gpu.rs` (the VRAM
  and per-engine micro bars), `quota.rs` (the observed resource-quota gauge
  and its alert; an undeclared quota reads "not declared", never a percentage).
- `src/pages/performance/scene/hover.rs` — the curve card's hover readout:
  the hovered value plus the INSTANT that value was measured at (the shared
  trend window's timestamped samples through the shared formatter), never a
  sample offset. A sample without a measurement instant reads the shared
  dash.
- `src/pages/performance/scene/npu_power.rs` — the NPU card's power section:
  the accelerator's power curve beside the compute curve, drawn only while
  the shared contract reports the capability. The samples come from the
  shell's bounded power history; an absent capability draws no curve at all.
- `src/pages/performance/scene/sidebar/cores.rs` — the per-core grid: the
  P/E/LP-E class groups, the NUMA/socket envelopes, the cells with their
  independent label/value slots, the column policy (four columns wide, two in
  the 720×480 fold) and the typed reveal motion.
- `src/pages/performance/device_curves/storage.rs` + `storage/sync.rs` — the
  storage surface: the per-partition capacity ring, the SMART status badge, the
  wear gauge, the mount alarm row and the missing-tool guidance card, with a
  typed expand/collapse motion and end-of-frame retirement.
- `src/pages/performance/device_curves/storage/alarm.rs` — the filesystem
  alarm row: the raised alarm wash (the theme's warning ink composed over the
  card's own fill in the theme's sRGB space) and the alarm's own mark — a
  read-only mount wears the shared padlock, an erroring mount the alarm
  triangle.
- `src/widgets/scrollbar.rs` — the visible scroll rail: a translucent track
  plus a capsule thumb, bound to the scroll viewport it serves.
- `src/widgets/switch.rs` — the sliding switch: a real knob that eases
  between the track's ends, not a checkbox.
- `src/widgets/focus_ring.rs` — the focus ring's layer model: an inner
  highlight plus an outer halo, with the shared hairline widths.
- `src/widgets/easing.rs` — the shared `(0.16, 1, 0.3, 1)` ease-out curve and
  the progress helper every micro-motion reads.
- `src/window/edge.rs` — the one-pixel edge highlight on an undecorated
  window.
- `src/pages/settings/rows.rs` — the settings rows that mount the switch and
  scale with the UI-size axis.
- `src/widgets/table.rs` + `src/widgets/table/` — the process table: the
  column-drag resize session, the shaped-paragraph ellipsis, the
  tabular-figures font features, the zero-value contrast calibration and the
  visible cell selection with its row copy.
- `src/pages/system/` — the System page surfaces: `overview.rs` (the measured
  card grid, its 720×480 two-by-two fold and the memory-inventory capture
  anchor), `topology.rs` (the SMBIOS slot and channel map), `privacy.rs` (the
  serial masking toggle) plus the existing dashboard/thermal/health modules.
- `src/widgets/chart.rs` + `src/widgets/chart/` — the chart family: a chart is
  ONE continuous, coverage-antialiased path per measured run, drawn by a
  per-frame canvas painter that measures its own slot and projects the window
  against that measurement. `geometry.rs` owns the projection and the
  reference rules, `canvas.rs` the raster layer order (wash → grid → curve →
  guide), `painter.rs` the per-frame paint and the motion state, `hover.rs`
  the crosshair and the value pill. The former per-segment rotated-rectangle
  primitive is gone with no fallback: a segment seam is a defect, not a style.
- `src/widgets.rs` + `src/widgets/` — the owned component layer: table/chart
  projection cores, bsn! render adapters, controls, and the action menu
  supporting both pointer click and keyboard navigation.
- `src/input_contract.rs` — Bevy-key normalization through the shared command
  router and stable semantic addresses for rendered rows.
- `src/runtime.rs` — the process-wide platform client via the app-host
  `OnceLock` cache pattern.
- `src/pages/alerts/` — the alert-center surfaces: `severity.rs` maps the
  core's alert severity onto the theme's severity levels and reads the ink
  from the theme's own severity palette (the frontend never rebuilds a
  severity table); `banner.rs` is the firing alert's spring reveal; `toast.rs`
  is the clipboard-success confirmation; `pause.rs` is the global PAUSED
  watermark mounted over the content slot (the nav rail stays live).
- `src/pages/services/` — `status_dot.rs` (the breathing status dot; only the
  running state breathes, and the ink is the palette's success/danger/dim
  triple), `log_text.rs` (the RFC3339 stamp plus the level-keyword split),
  `log_follow.rs` (the follow lock, whose single authority is the shell's
  `feed.follow`), `deps_tree.rs` (the dependency relation tree), `unit_filter.rs`
  (the sliding unit-type filter), `action_glyph.rs` (the lifecycle marks).
- `src/pages/sessions/` — `kind.rs` (maps the core-owned, typed family fact to
  the family's own semantic plate: a display-server session the monitor, a
  console session the terminal; the frontend never reclassifies `tty`/`seat`/
  `remote`) and
  `bubble.rs` (the logout confirmation bubble: a projection of the shared
  confirmation gate, never a second gate).
- `src/pages/containers/` — the containers page: `quota.rs` (the DECLARED
  container quota: the typed state of the declarations, the observed share of
  the declared soft memory ceiling, the declared CPU budget, and the pulsing
  warning halo an over-limit row wears). The page compares a container only
  against the ceilings the container declared: a container that declares
  nothing is never compared against a limit of the page's own making, and an
  unreadable declaration reads "unknown" instead of a zero.
- `src/pages/startup/` — `impact.rs` (the three-tier impact badge), `timeline.rs`
  (the boot critical-chain gantt over the shared timeline rows), `phases.rs`
  (the three-phase proportion rows; the phases come from the shared
  contract's measured split, and an unmeasured phase renders as an
  unavailable row with no bar at all, never a zero-width one), `forensics.rs` (the pinned
  failed-unit forensics panel with typed disclosure state), `inks.rs` (the
  page's semantic ink painter — the sanctioned route when the window's text
  role observer owns the default ink).

Diagnostic commands open, confirm, retry and close the shared sanitized
review lifecycle. The native window injects its diagnostic client; capture
never creates a writer. The modal consumes all keyboard input, exposes scroll
keys, and reserves title/actions around one bounded body viewport.
Applications derive table capacity from its measured slot; complete process
trees share the modal budget and native expand/collapse controls.
