# GPUI parity boundaries

This charter records the Bevy capabilities checked against the GPUI page
contract and the remaining boundaries. It is a current-state document, not a
score or a work plan; behavior proofs stay in the mounted headless trees.

## Implemented and proved

- **Tooltip / popover**: `tooltip.rs` owns the hover/focus state machine,
  viewport placement, Escape/pointer dismissal and the real Card-tier glass
  card. `menu_modal.rs` owns an anchored popover whose outside press reaches
  the underlying surface. `tests/headless/tooltip.rs`,
  `tests/headless/menu_popover_tests.rs`, and
  `tests/headless/pages/settings_widgets_parity.rs` cover those seams.
- **Settings controls**: the settings page projects the shared shell entries
  into switches, discrete radio choices, a continuous refresh-cadence slider,
  the permission center, and keyboard focus rings. The slider matches GPUI's
  0.5–5.0 s / 0.1 s reference row, writes the shared shell and config
  authorities, binds the same shared slider skin as alert thresholds, and
  keeps drag updates in place until the final value.
  `tests/headless/pages/settings.rs` and `settings_widgets_parity.rs` prove
  activation, read-back, rem scaling, high contrast, unavailable permission
  data, and the focused switch.
- **Slider keyboard control**: the shared slider skin is a tab stop that
  wears the product's one focus ring, so the keyboard reaches it at all. A
  focused slider owns GPUI's key set — the arrows step, Home/End jump to the
  range ends — and those keys never reach the shell's routers, so the page's
  selection movement cannot run underneath a slider the user is adjusting.
  The vertical axis rides the official widget's own relative-step entry, so
  the step size and the range clamp stay the widget's authority. Every
  change reaches the canonical authority exactly like a drag, the widget's
  own value follows it (so a second step composes on the accepted value),
  and the keyboard focus follows the control across the page rebuild its own
  change triggers. `tests/headless/pages/settings_slider_keyboard.rs` proves
  the tab stop, the key ownership, the cadence write-back, and the value
  write-back.
- **Keyboard focus continuity**: EVERY interactive control the component
  layer mounts (buttons, checkboxes, switches, radio/select choices, sliders,
  text inputs, table rows, tabs and scrollbars) wears a stable, page-scoped
  `FocusIdentity` resolved from the scene's own semantic markers. A rebuild
  replaces the entity, not the control: `follow_focus_across_rebuild`
  rebinds the one `InputFocus` to the replacement, so a control that changes
  the page (toggling the theme mode) keeps the focus it just received.
  `tests/headless/focus_continuity.rs` proves one behaviour test per family
  — press, rebuild, press the SAME key, assert the second action lands —
  each paired with a sabotage arm that disables the restore; the real
  Settings switch rebuild is proved in `tests/headless/focus_visible.rs`.
- **Startup status filter**: the Startup page offers GPUI's `All`, `Enabled`,
  and `Disabled` choices through the shared shell filter vocabulary. Filtering
  preserves provider identity for selection, keyboard movement, and the
  fallback action menu; `startup-status-enabled` standard and compact captures
  prove the mounted filter and filtered rows.
- **Process details**: `pages/processes/details.rs` consumes
  `taskmanager_application::process_details_vm`; the effect observer queues
  refresh/network/escalation commands through the shell. The detail tests
  prove empty, partial, unavailable, unreadable, and permission-dependent
  states without fabricating zero or success.
- **System information groups**: `system_information_modal.rs` now renders
  the shell-owned `SystemInformationGroup` facts as titled groups with
  bounded label/value rows, matching GPUI's grouped review instead of one
  unstructured copy blob. The headless tree proves mounted titles/values,
  clipped slots, the non-shrinking value floor and the typed unavailable
  state. Copy All still exports the complete provider-owned text.
- **Keyboard / secondary press / linkage**: `input::dispatch` owns one ordered
  key pass; the card and dense presentations address the same shell row,
  selection reveal, semantic address, official `Button`, and secondary-press
  seam. The focus ring follows the toolkit's `InputFocus`, and the card lane's
  overflow action filters through the shell and switches to dense mode.
- **Focus card / group deduplication**: when the shared height verdict admits
  complete status groups, the selected row's dashboard represents the first
  card of its status bucket; the bounded group row admits only the remaining
  members and the overflow count includes the dashboard. The headless plan,
  mounted overflow, and standard/compact bounds tests prove that one shell row
  is never painted twice and that both responsive rungs stay bounded.
- **Card actions / nested optical glass**: the action popover can anchor to an
  atom card and freezes that card's visible-row identity. The standard
  `applications-card-lane-optical-menu` hardware capture admits its frame only
  when the menu's material is Attached at measured depth `1`, a depth-`0`
  glass surface is Attached, the chain camera is active, and lane bounds fit.
  The compact companion capture proves menu/card bounds; Painted fallback
  pixels are not recorded as optical proof.
- **Degraded glass surfaces**: the shared theme owns the fallback material
  (`Painted`, `Frosted`, `Opaque`) and this frontend paints it. A high-contrast
  card is a crisp opaque panel with its tier edge, no sheen and no backdrop
  sampling; a headless card keeps its tier material and sheen; a Frosted card
  still samples and blurs the backdrop with no refraction. The binding carries
  the material, `src/palette.rs` resolves the theme's surface, and
  `tests/headless/optical_fallback.rs` asserts the fill, edge, shadow,
  sheen gradient, grain and uniform lanes for each kind.
- **Keyboard help**: bare `F1` opens the shell-owned help mode; `?` / Escape /
  the footer Close action leave it. `?` types into an active search field,
  and a learned page chord closes help while still moving the route.
  `help_overlay.rs` binds the shared `page_help`, `command_help`, binding
  declaration and local opener table to the product's centered bounded-scroll
  dialog; the standard and compact `keyboard-help` captures prove the mounted,
  painted surface. Hardware-WESL capture readiness additionally requires an
  attached production material on this dialog, so a `Painted` fallback cannot
  be recorded as optical evidence.
- **Empty / error / permission states**: the card lane has a typed empty state;
  settings permission controls consume the shell's typed
  `CapabilitySnapshot` and reject a stale escalation offer; detail summaries
  distinguish empty, unreadable, unsupported, missing, and
  escalation-required observations.

## Explicit boundaries

- The page-level details rail is an OPTIONAL whole group admitted at the
  shared `COMPACT_CONTENT_WIDTH` band: a measured page slot at or above it
  paints the selected-process details beside either the card lane or the dense
  table, and a narrower slot hides the rail as one group instead of clipping
  its label/value columns. Its projection, effect and typed-unavailable seams
  are headlessly proved. Targeted `applications-card-lane-details-wide` and
  `applications-card-lane-details-compact` real-window scenarios were reviewed
  by eye: the wide rail has bounded label/value rows and the compact rail is
  hidden with no clipped detail content. The compact frame exposed a blank
  lower canvas where the zero-group verdict should degrade to the dense
  table; a post-layout fold trigger now rebuilds the active presentation, and
  the fresh reviewed compact screenshot shows whole dense-table rows filling
  that lower slot. The wide rail's bottom still uses a bounded scroll area;
  its rail lane now reserves one control-height bottom safety band. A capture
  made between row-aligned scroll positions can still show a partial row at
  the viewport edge; the screenshot does not prove a whole-row snap.
- The `tooltip-overlay` capture is a real tooltip above the Applications
  optical hero; the window uses a software renderer in the capture harness, so
  it proves placement, content and bounds, not hardware WESL refraction.
- Nested optical glass is bounded by
  [`NESTED_OPTICAL_CHAIN.md`](NESTED_OPTICAL_CHAIN.md): depth `0` and `1` are
  admitted by the measured chain, depth `2` or deeper fails closed. The GPU
  receipt measures chain pixels, nested-only output and the theme-derived
  Fresnel rim; the targeted standard hardware capture binds the shipped menu
  to a real depth-1 material.
- System-depth modal, multi-window/layer-shell, and GPUI-specific device hero
  work remain the frontend charter's public boundaries; this crate does not
  own a second vocabulary for them.
