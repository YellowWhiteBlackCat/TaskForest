# UI components and presentation

Generic badges, surface painting, rich text and virtual table/body construction
come from `zhuge-ui-kit-iced`. Pure fixed-extent geometry and text-run splitting
come from `zhuge-ui-core`. This frontend owns theme/column/business adapters,
native selection and renderer caches; all shared types use their defining modules.

- `src/ui.rs` owns root dispatch/chrome; `src/ui/performance.rs` owns the
  Performance selector/detail composition; `src/ui/perf_rail/captions.rs` owns
  pure rail text projection; the remaining `src/ui/` modules own page
  projections and charts.
- `src/ui/components.rs` owns shared Iced surfaces, the root page scaffold,
  key/value rows and the state-panel grammar;
  `src/ui/components/primitives.rs` adds the token-styled
  badge/divider/progress/tooltip primitives (colors only from the palette
  snapshot, unavailable progress never renders as a measured zero);
  `src/ui/components/inputs.rs` adds switch/slider/select/search/segmented
  controls that route keyboard access through the focus shell.
- `src/focus.rs` owns keyboard-reachable activation shells and draws the shared
  `palette().ring` focus ring gated by the renderer-local input modality
  (`src/input_modality.rs`: only keyboard focus paints the ring, the same
  strict policy the GPUI root tracker applies); `src/focus/reveal.rs` measures
  the actual focused control and reveals it inside its nearest scroll owner.
- `src/ui/virtual_list.rs` owns bounded table-window geometry, the
  sticky-header composition shell and the typed column vocabulary — the
  Applications table derives widths/alignment/hideability from the shared
  `PROCESS_COLUMNS` contract, and Services/Users/Startup keep local typed
  specs.
- `src/ui/responsive.rs` is the frame-local layout budget chain (`LayoutProfile`
  × `VerticalSpace` → `PageLayoutBudget` → page presentation enums, GPUI
  semantics as pure data). Chrome and toolbar breakpoints are typed facts on
  it, not scattered literals.
- `src/app/appearance.rs` resolves the System color mode from the OS
  (`iced::system::theme()` boot query + `theme_changes()` subscription); an
  explicit Light/Dark/EyeForest choice is never overridden. The motion
  preference is a persisted config token (`normal`/`reduced`/`none`) that seeds
  the process motion policy: reduced clamps transitions to 80ms, none drops
  modals straight to their final state without the frame pump.
- Type scale is single-track: `tokens::FONT_*` px constants on the Small
  baseline, scaled by the application `scale_factor` (`renderer_scale`). Role
  methods that bake the UiSize into the value double-scale and are not used
  here.
- The Settings surface is grouped (General/Appearance/Fonts/System/Notifications
  /Units) and renders real controls — segmented, slider, select, switch — over
  the same `SettingsChange` channel and persisted tokens; the shortcut legend
  derives from the Iced binding declaration. Text rendering shows an honest
  unavailable state (no dead selector).
- Diagnostic review consumes the shared frozen plan and publication lifecycle.
  The title and Export/Retry/Close actions stay fixed around one bounded scroll
  body; only explicit confirmation admits a writer, and replacing the surface
  invalidates late completions.
- About (`ui/about.rs`) uses shared build metadata and typed repository
  opening. Native boot requests desktop appearance through the shared effect
  seam. System information (`ui/system_information.rs`) has an independent
  primary slot, frozen shared groups and a bounded scroll owner with fixed
  Copy/Close actions.
- Hardware review uses the shared accepted memory-inventory rows, including
  incomplete populated slots. System actions remain fixed above one native
  bounded scroll body.
- Iced-local surfaces: the first-run dialog (`ui/first_run.rs`, platform
  setup-script application controller, quiet observation and explicit Settings
  entry, bounded metadata viewport with fixed actions; dismiss is side-effect
  free), the Containers full page (`ui/containers.rs`, six honest branch states
  via `page_branch`), and the System-page dashboard (`ui/system_dashboard.rs`)
  consumes the shared application timeline. Fixed window controls and metric
  paging admit complete cards inside the allocated body; summary CPU/memory
  values use the same accepted readouts as their curves.
- Device charts share one window→geometry mapping (`WindowSlots`,
  right-anchored slots; hover crosshairs snap through the same mapping) with
  DATA/OVERLAY dual caches; the dual-series device chart
  (`ui/device_chart/multi.rs`) separates read/write and rx/tx with same-hue
  tinted variants, fed by the store's split live rings (session-local chart
  height until the multi factory grows a Fill variant).
- Applications column widths are keyboard-reachable, persisted overrides over
  the contract defaults: a 6px header drag hotspot or the column menu's
  per-column stepper publishes `ResizeProcessColumn` (fill/identity columns are
  never resizable), widths clamp to 40..=600px, and the header, body and scroll
  extent resolve from the same override-or-default seam so sticky alignment
  never drifts. Widths persist through the shared `process_col_widths` config
  tokens (the same vocabulary GPUI writes); startup rehydrates through the same
  clamp domain and drops unknown tokens.
