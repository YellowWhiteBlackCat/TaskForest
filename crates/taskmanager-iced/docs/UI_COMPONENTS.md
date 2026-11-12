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
  `src/ui/components/controls.rs` plus `controls/` own the GPUI-parity
  activation/layout family: the one `content_button` native-button adapter,
  button, icon button, pill, checkbox, section header, toolbar, label,
  single-line input, multi-line editor and the component-layer spinner entry.
  Every native button spends the adapter's typed style/state/focus/activation
  seam; Run Task mounts the multi-line editor over the existing
  `CommandLaunchRequest::from_draft` owner, which alone canonicalizes line
  endings and trims the draft; `ui/components/toast.rs` paints the shared
  shell feedback notice as a non-modal Card-glass toast whose visibility
  follows the shell's `FeedbackLifecycle`.
  `src/ui/components/primitives.rs` adds the token-styled
  badge/divider/progress/tooltip primitives (colors only from the palette
  snapshot, unavailable progress never renders as a measured zero); tooltip
  wrappers inspect descendant focus state and the keyboard-visible modality
  so focusable hints open without requiring pointer hover, and inventory-row
  detail plates park at the viewport edge farthest from the trigger so dense
  table rows stay readable (GPUI `row_tooltip_anchor` parity);
  `src/ui/components/inputs.rs` adds switch/slider/select/search/segmented
  controls that route keyboard access through the focus shell. Services search
  combines a page-local name/description query with the GPUI-parity
  All/Active/Inactive/Failed status segmented filter; both narrow visible
  indices while canonical rows retain provider identity. Startup search and
  the shell-owned All/Enabled/Disabled segmented filter compose through one
  typed projection key; the shared Startup projection remains complete for
  other consumers. Users keeps every
  session visible and applies GPUI's shared-process-query highlight to matching
  name text without filtering the session inventory.
- `controls/strip.rs` owns the bounded horizontal action strip: all action
  groups keep their measured width, and the caller declares the tallest
  `ControlRole` whose shared extent sizes the viewport. Control padding stays
  inside each control, so button-only navigation does not add input padding
  to the page's vertical budget. Navigation and the Applications action bar
  use the same owner; the process table keeps its own two-axis viewport.
- `src/theme.rs::control_glass` is the ONE control-surface seam: the shared
  Card glass gradient, its specular edge, and the single-quad projection of
  the theme's two-layer glass shadow (the same projection the floating
  panels consume). The switch, slider, select, search field and the
  segmented track, plus the new activation/text family, all read it, so no
  control owns a second palette, radius ladder or shadow ladder.
  The Applications empty table body spends the shared inset
  `GlassSurfaceRole::PageHeader`: it keeps the Card-tier fill and hairline
  without stacking the floating elevation shadow that the GPUI control-band
  owner removed.
  `theme/control_measurement.rs` selects the live `UiSize`/`RowDensity` axis
  and delegates to `presentation_binding::control_measurement`; every control
  spends `ControlRole::{Button, IconButton, Input, Toggle}` through
  `resolve_size`, never a second size table. Hover/pressed/focus/disabled
  only reweight paint (the shared `ControlState::changes_measurement`
  contract). The focus ring reads the shared ring token, so a pointer-driven
  focus paints no ring on the switch, segmented track, buttons or checkbox.
  All visible copy is an i18n key resolved by `taskmanager-application`;
  `icon_button` uses `IconId` and exposes its localized label as a shared
  tooltip because Iced 0.14 has no accessible-name operation.
  `src/ui/spinner.rs` remains the sole spinner canvas/arc owner; the component
  wrapper only re-surfaces it to the warm-up body.
- The last raw toolkit controls are converged: native buttons live only in
  `controls/material.rs::content_button`; the Users session actions run
  through the shared glass button in both availability states (the
  unavailable state keeps the shared `ControlRole::Button` extent, drops the
  focus stop and publishes no request), the Network signal-quality bar
  spends the shared `components::progress` primitive (clamped ratio, unknown
  grammar for unavailable data), and the Applications priority preset uses
  the shared `components::select` — the same `ControlRole::Input` glass,
  geometry and focus stop the Settings selects use, instead of a bare
  `pick_list`.
- `src/app/user_menu_availability.rs` is the Users row-menu availability
  authority: the frozen menu target must still have one unique provider
  identity in the canonical session projection. When it is stale, the menu
  exposes only `UserRowMenuClose`, arrow traversal keeps only that focus
  stop, and an open request lands on Close; Disconnect/Lock are disabled,
  unfocusable and publish no session-control request. A current target keeps
  Disconnect and Lock in the focus cycle and opens on Disconnect. The
  availability verdict also keys the rendered menu so a projection refresh
  remounts the action buttons in the correct state.
- `src/ui/components/dashboard_budget.rs` spends the shared card-flow
  admission verdict (`taskmanager_ui_contract::presentation::card_flow`):
  the whole-group admission and the card floor..ceiling clamp are the shared
  arithmetic, while Iced keeps only its own compact-summary decision and the
  chart/chrome split inside one admitted card.
- `src/focus.rs` owns keyboard-reachable activation shells and draws the shared
  `palette().ring` focus ring gated by the shared input modality
  (`taskmanager_ui_contract::InputModality`: only keyboard focus paints the
  ring — one strict policy for GPUI, Iced and Bevy; `src/input_modality.rs`
  owns only the root capture observer); `src/focus/reveal.rs` measures the
  actual focused control and reveals it inside its nearest scroll owner.
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
  derives from the Iced binding declaration. The graph-point slider spends
  GPUI's one-point step and restart note, while the sliding-graph switch binds
  the persisted `Config::sliding_graphs` fact to the Performance chart motion.
  Text rendering shows an honest unavailable state (no dead selector).
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
- Device charts share one window→geometry mapping (the shared owner's
  `taskmanager_shell::presentation::ChartSlots`,
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
