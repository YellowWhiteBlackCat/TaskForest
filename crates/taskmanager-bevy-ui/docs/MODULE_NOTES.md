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
  and republished gate transitions.
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
- `src/widgets.rs` + `src/widgets/` — the owned component layer: table/chart
  projection cores, bsn! render adapters, controls, and the action menu
  supporting both pointer click and keyboard navigation.
- `src/input_contract.rs` — Bevy-key normalization through the shared command
  router and stable semantic addresses for rendered rows.
- `src/runtime.rs` — the process-wide platform client via the app-host
  `OnceLock` cache pattern.

Diagnostic commands open, confirm, retry and close the shared sanitized
review lifecycle. The native window injects its diagnostic client; capture
never creates a writer. The modal consumes all keyboard input, exposes scroll
keys, and reserves title/actions around one bounded body viewport.
Applications derive table capacity from its measured slot; complete process
trees share the modal budget and native expand/collapse controls.
