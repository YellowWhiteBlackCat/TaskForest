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
  into switches, radio choices, theme/language/refresh/capacity controls, the
  permission center, and keyboard focus rings. `tests/headless/pages/settings.rs`
  and `settings_widgets_parity.rs` prove activation, read-back, rem scaling,
  high contrast, unavailable permission data, and the focused switch.
- **Process details**: `pages/processes/details.rs` consumes
  `taskmanager_application::process_details_vm`; the effect observer queues
  refresh/network/escalation commands through the shell. The detail tests
  prove empty, partial, unavailable, unreadable, and permission-dependent
  states without fabricating zero or success.
- **Keyboard / secondary press / linkage**: `input::dispatch` owns one ordered
  key pass; the card and dense presentations address the same shell row,
  selection reveal, semantic address, official `Button`, and secondary-press
  seam. The focus ring follows the toolkit's `InputFocus`, and the card lane's
  overflow action filters through the shell and switches to dense mode.
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
  are headlessly proved; no targeted capture yet shows the rail beside the
  card lane.
- The `tooltip-overlay` capture is a real tooltip above the Applications
  optical hero; the window uses a software renderer in the capture harness, so
  it proves placement, content and bounds, not hardware WESL refraction.
- Nested optical glass is bounded by
  [`NESTED_OPTICAL_CHAIN.md`](NESTED_OPTICAL_CHAIN.md): depth `0` and `1` are
  admitted by the measured chain, depth `2` or deeper fails closed. The GPU
  receipt measures chain pixels; no shipped-window screenshot proves the
  visual appearance of two optical layers with human eyes.
- System-depth modal, multi-window/layer-shell, and GPUI-specific device hero
  work remain the frontend charter's public boundaries; this crate does not
  own a second vocabulary for them.
