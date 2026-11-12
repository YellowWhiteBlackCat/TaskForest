# Applications card lane

The Applications page has two presentations of ONE shell-owned process row
projection:

- **Focus lane (default):** a current-selection/system-state dashboard, six
  status chips, and a bounded atomic-card flow grouped by the provider's own
  process state. The visual reference is the [public FlClash v0.9.1 changelog](https://github.com/chen08209/FlClash/blob/main/changelog.json)
  description — dashboard around current focus, proxy groups as cards, group
  tabs as chips, optional two-tier Liquid Glass, iOS-style surfaces. No asset,
  code, or layout metric is copied from FlClash.
- **Dense mode:** the measured process table. Sorting, column resize, cell
  selection, row selection, wheel/virtual window, context menu, and the
  Properties/Affinity/Batch confirmation seams remain unchanged.

## Ownership

- `ShellApp` owns every row fact: query, status filter, sort, visible row
  window, selection, semantic identity, and typed availability.
- `taskmanager-ui-contract::presentation::card_flow` owns the rung order,
  whole-group admission and card-height clamp. `presentation::card_width`
  owns the measured card/content caps, whole-column admission, bounded lower
  content width and residual lane space. Bevy supplies only its own measured
  control ladder, optional dashboard height, group header height, group gap,
  card floors and column ceiling.
- `taskmanager-theme` owns design tokens; the Bevy card hero consumes
  `GlassTier::Card` through the palette and the shared shadow contract.
- The focus dashboard and the atomic cards are ALL marked for real optical
  glass. `backdrop_mirror` mirrors each marked surface's PAINTED tier into
  the offscreen capture root, so the shader samples the product's real
  surfaces, their edges and their shared layered shadows instead of a flat
  canvas. The shared capability decision still owns headless, high-contrast,
  software-adapter, and no-adapter fallbacks.
- Atomic cards consume the full shared `GlassPanelMaterial` — fill, specular
  edge, REM panel radius and `Layered` shadow — rather than reprojecting a
  subset through control geometry. Hover/pressed/selected ink remains on the
  state overlay above the glass, with the same rounded boundary.
- The mirror alone is a painted lower composite; the RECURSIVE optical chain
  lives beside it in
  [`NESTED_OPTICAL_CHAIN.md`](NESTED_OPTICAL_CHAIN.md): the chain target holds
  the first layer's optical pixels, not a painted proxy.
- The production renderer reports `OPTICAL_GLASS_MAX_NESTED_LAYERS = 2` and the
  hardware receipt records `nesting_layers_proven=true`: an overlay measured
  at depth `1` samples the layer beneath it optically. A deeper composition
  still fails closed to the typed Frosted fallback.

## Layout contract

- The bounded adaptive lane below the fixed mode row is measured once. The
  shared `admit_card_flow` fold spends its remaining height in order:
  complete status groups with the dashboard, one complete selected-process
  focus card with the optional dashboard removed, then the dense table.
- With complete groups admitted, the dashboard is the current-focus card of
  the selected row's status bucket: that row is not painted a second time in
  the bucket's atomic-card row, and its header's overflow count treats the
  dashboard as one represented member. This keeps the FlClash reference's
  "dashboard around the current exit" above "proxy groups as cards" as one
  hierarchy of distinct facts instead of a duplicated card.
- The focus-card rung also spends `card_width` for one complete column. It
  preserves the selected row's full name/status and PID/CPU/memory lines,
  semantic address, accessibility node and activation seams. If either
  measured floor fails, the lane yields to the table; it never paints a
  partial card, row or group header.
- A bucket's whole size is stated in its header. The page chrome's canonical
  status pills are the SIX shared filters; the lane paints no second chip row
  because the buckets and the pills classify by the same provider state.
- When the admitted row cannot hold a bucket, its header carries a localized
  overflow action. Activating it reduces through the shell's canonical status
  filter and switches the page to its dense table, so every omitted row stays
  reachable.
- A card's width floor is the width its PID/CPU/memory fact line needs; its
  measured ceiling stops it becoming an empty banner. The shared cap fold
  admits a whole card grid and returns one bounded content width plus the
  residual lane space. Wide slots keep that residual OUTSIDE the card group,
  and the group header's overflow action stays inside the same bounded
  group. Each fact is labeled from the shared catalog and clipped on a
  bounded single line.
- The lane scrolls by whole buckets. A typed `CardFlowWindow` owns the first
  visible bucket, and the shell's selection reveal moves that window to the
  selected row's bucket instead of leaving the cursor behind.
- A bucket is the provider's own status, not a run of adjacent rows: a page
  sorted by CPU interleaves the statuses, and every status still collects
  into ONE group. A group header states the bucket's whole size and names
  the omitted rows with the shared `more_rows_label`, so a truncated row
  never reads as the whole bucket.
- The card's content is fixed (name/status line, fact line, gap, padding),
  so its floor and ceiling are the same height: leftover slot space stays
  OUTSIDE the card instead of inflating it into an empty panel.
- The lower card flow is a bounded `Overflow::clip_y()` viewport. Its right and
  bottom edges are headlessly asserted at 1180×780, a wide 1600×900 measured
  composition asserts the shared cap and its residual, and the 720×480
  composition admits one complete selected focus card between the card and
  group floors.
- Atom cards keep the same `ProcessRowLink`, semantic address, AccessKit node,
  official `Button`, pointer activation, and secondary-press seam as dense
  rows. Keyboard/focus routing remains the shell's single authority. The
  optical card's hover, pressed and selected ink rides a state overlay above
  the glass, because an optical surface owns no painted fill to replace.
- Atom-card hover hints expose the provider-owned command line when it is
  longer than the displayed process name, matching the GPUI process-row
  tooltip. The tooltip uses the existing shared hover state and glass surface.
- The `applications-card-lane-optical-menu` capture opens the production
  process Actions popover against a mounted atom card and freezes that same
  card's visible-row identity. Its measured rectangle overlaps one depth-0
  optical surface, so the menu samples the real chain target at depth 1.
  Hardware capture readiness requires the menu's production material to be
  `Attached`, an exact `(0, 1)` optical pair, complete lane bounds, and an
  active chain camera; a Painted fallback cannot certify the optical frame.

## Evidence and boundaries

- `tests/headless/pages/process_card_lane.rs` covers shared status pills,
  whole-group and focus-card admission, focus facts, dense-mode seams,
  compact selected-card bounds, measured bounds, overflow activation, the
  GPUI-parity command hint, and the typed painted fallback without an adapter.
- `tests/headless/pages/process_card_glass.rs` proves that an atomic card and
  its interaction overlay consume the same shared Card-tier fill, edge,
  shadow, panel radius, optical marker and backdrop mirror.
- `tests/headless/pages/process_menu.rs` proves the card popover freezes the
  identity of the visible row it is anchored to.
- `tests/headless/pages/card_lane_width_admission.rs` asserts the capped
  right-edge verdict before the renderer spends it: whole group width
  admission, the measured card/content cap and residual, a narrow slot's
  one-column degradation, the overflow action's bounded slot, and the typed
  zero-column degradation for a slot that holds no whole card.
- `tests/headless/optical_backdrop_mirror.rs` proves that a marked surface is
  mirrored at its measured rectangle, that a move or palette change bumps the
  composite revision, that an unchanged surface does not, and that stale
  mirrors are swept.
- `capture_bevy_scenarios.tsv` adds the light and dark `applications-card-lane`
  scenarios and the wide/compact `applications-card-lane-details-*`
  readiness scenarios, plus standard and compact optical-menu scenarios.
  Optical hardware readiness waits for the attached nested menu and active
  chain; all card-lane frames also wait for whole-group or focus-card bounds.
  The details scenarios use the shared
  `COMPACT_CONTENT_WIDTH` whole-group verdict against the rail's measured
  `Display`. Compact windows prove their zero-group degradation in headless
  bounds tests, rather than pretending painted card groups exist.
- Targeted Wayland captures of the light and dark card lane, the tooltip
  overlay and the settings switch focus were run on this host and reviewed
  by eye: the three-column card grid paints name/status and PID/CPU/memory
  facts on bounded single lines, group headers state whole bucket sizes and
  their overflow actions, the selected card keeps its state tint, and the
  tooltip and focused switch remain unobstructed. No overlap or clipped
  value was observed. The full durable matrix was not published (the run
  stayed targeted).
- The GPU proof renders a separate nested-only target and compares it with
  the production chain target; the standard gate records both the pixel proof
  and the adapter receipt. The targeted hardware `applications-card-lane-optical-menu`
  window capture is additionally gated by attached depth-1 material readiness.
  The compact screenshot is layout evidence only when hardware WESL capture is
  disabled and does not claim optical output.
