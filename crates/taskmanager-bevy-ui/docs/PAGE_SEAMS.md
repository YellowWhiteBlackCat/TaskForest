# Page seams

`src/pages.rs` + `src/pages/` hold the nine mounted page modules, one file
each (`content(&PageContext) -> impl Scene`), plus the `process_tree.rs`
owned inspection and `history.rs` connector adapter: the measured process
table, performance summary/curves/device blocks, the three read-only
inventory tables (services/startup/sessions), settings, and the alert center.

- `pages/processes/properties_modal.rs` delivers full identity-bound Overview,
  Performance, Command and Insights tabs. Its model resolves canonical live
  keys independently of table filtering; measured charts preserve missing
  samples, and complete facet lists scroll in one bounded body between fixed
  selectors and actions.
- `src/pages/alerts/editor.rs` mounts canonical rule creation, field edits,
  enabled state, removal and clipboard import/export. Rules and events scroll
  beneath fixed page actions; edits always reduce through the shared shell.
  The threshold value rides a real `bevy_ui_widgets` slider whose typed value
  change reduces through the canonical rule edit, and a successful export
  raises one clipboard-success toast.
- The alert center paints severity through the theme's own severity palette
  (the frontend resolves the owner's palette, it never assembles a severity
  table), and the firing rule's banner opens through a spring reveal. A paused
  telemetry stream freezes the content slot behind a PAUSED watermark while
  the nav rail stays live.
- The services page paints a breathing status dot per unit (only the running
  state breathes), splits the log level keywords out of the message body, and
  releases the follow lock when the reader leaves the newest entry — the
  shell's `feed.follow` stays the single authority. The unit-type filter bar
  slides sideways instead of wrapping, and the dependency panel renders the
  relation tree with typed disclosure state.
- The containers page states each container's DECLARED quota in its own
  column: the observed share of the declared soft memory ceiling, the
  declared CPU budget, or the typed absence of a declaration ("not declared"
  / "unknown"). A row that crossed a declared ceiling wears a pulsing amber
  halo whose opacity follows the shared projection revision; the page never
  compares a container against a ceiling the container did not declare, and
  the seven column slots are percentage shares of the row's content box, so
  the header and every data row keep ONE basis at any window width.
- The startup page pins the failed-unit forensics panel above the inventory
  (each unit an accordion over the provider's own load/active/sub state) and
  lays the boot critical chain out as a gantt over the shared timeline rows.
  The three-phase proportion rows mark an unmeasured phase as unavailable.
- System Health selects shared storage and sensor groups inside the measured
  System body. SMART controls freeze disk generation before activation and arm
  the shared confirmation; replaced targets cannot redirect a stale control.
- Performance device editing exposes concrete show/hide and order controls,
  persists through the shared config coordinator and blocks page keys while
  open.
- Performance preserves device selection across mounting. Disk
  throughput/activity and battery charge/power curves use identity-scoped
  history and measured widths; complete device groups scroll beneath fixed
  selectors and leave a bottom inset.
- `src/pages/history/control.rs` routes both review surfaces through one
  history controller and native session. Performance review has pinned actions
  around a bounded body; application review admits complete metric cards in
  its own bounded body. Settings enables/disables that session through
  canonical config.
- `src/widgets/history_controls.rs` owns window, refresh and return-to-live
  controls; `pages/performance/replay.rs` scales curves to measured chart
  bounds.

All nine routed pages are mounted on the widget substrate (the shared page
set is complete, System included), and the structural seams are live: real
input (keyboard through the shell's own routers, pointer picking and wheel
scroll through typed page seams), destructive verbs (EndTask/ProcessBatch/
ServiceControl through the shared gate — a typed action menu per inventory,
one confirmation modal, typed effects), the polyline chart family (hero
curves, sidebar mini-graphs, and the memory composition bar through one
gap-aware adapter), and the semantic accessibility channel (ui-contract
`SemanticSnapshot` plus `bevy_a11y` nodes published through the
`accesskit_unix` bridge on Linux). Remaining open surface is declared, never
hidden: backend capability absence is shown as typed unavailable state;
per-row process trends and multi-window composition remain separate surfaces.
