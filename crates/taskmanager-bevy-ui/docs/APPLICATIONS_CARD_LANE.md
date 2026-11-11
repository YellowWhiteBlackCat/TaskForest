# Applications card lane

The Applications page has two presentations of ONE shell-owned process row
projection:

- **Focus lane (default):** a current-selection/system-state dashboard, six
  status chips, and a bounded atomic-card flow grouped by the provider's own
  process state. The visual reference is the public FlClash v0.9.1 redesign
  description — dashboard around current focus, proxy groups as cards, group
  tabs as chips, optional two-tier Liquid Glass, iOS-style surfaces. No asset,
  code, or layout metric is copied from FlClash.
- **Dense mode:** the measured process table. Sorting, column resize, cell
  selection, row selection, wheel/virtual window, context menu, and the
  Properties/Affinity/Batch confirmation seams remain unchanged.

## Ownership

- `ShellApp` owns every row fact: query, status filter, sort, visible row
  window, selection, semantic identity, and typed availability.
- `taskmanager-ui-contract::presentation::card_flow` owns whole-group
  admission and the card-height clamp. Bevy supplies only its own measured
  control ladder, group header height, group gap, and column ceiling.
- `taskmanager-theme` owns design tokens; the Bevy card hero consumes
  `GlassTier::Card` through the palette and the shared shadow contract.
- The focus dashboard and the atomic cards are ALL marked for real optical
  glass. `backdrop_mirror` mirrors each marked surface's PAINTED tier into
  the offscreen capture root, so the shader samples the product's real
  surfaces, their edges and their shared layered shadows instead of a flat
  canvas. The shared capability decision still owns headless, high-contrast,
  software-adapter, and no-adapter fallbacks.
- The mirror alone is a painted lower composite; the RECURSIVE optical chain
  lives beside it in
  [`NESTED_OPTICAL_CHAIN.md`](NESTED_OPTICAL_CHAIN.md): the chain target holds
  the first layer's optical pixels, not a painted proxy.
- The production renderer reports `OPTICAL_GLASS_MAX_NESTED_LAYERS = 2` and the
  hardware receipt records `nesting_layers_proven=true`: an overlay measured
  at depth `1` samples the layer beneath it optically. A deeper composition
  still fails closed to the typed Frosted fallback.

## Layout contract

- The bounded `ProcessCardFlow` viewport is measured once. The mode row,
  dashboard, title and toolbar have already spent their height before that
  node is laid out, so Bevy subtracts only the flow's own group gap and
  spends the shared `admit_card_groups` verdict: complete groups only, a
  clamped card height, and zero groups when the floor does not fit.
- Zero groups degrade to the dense table; a compact viewport therefore never
  paints a half group or a clipped last card.
- A bucket's whole size is stated in its header. The page chrome's canonical
  status pills are the SIX shared filters; the lane paints no second chip row
  because the buckets and the pills classify by the same provider state.
- When the admitted row cannot hold a bucket, its header carries a localized
  overflow action. Activating it reduces through the shell's canonical status
  filter and switches the page to its dense table, so every omitted row stays
  reachable.
- A card's width floor is the width its PID/CPU/memory fact line needs; wide
  slots therefore admit a card grid instead of a row of banners. Each fact is
  labeled from the shared catalog and clipped on a bounded single line.
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
  bottom edges are headlessly asserted at 1180×780 and the compact 720×480
  degradation is asserted through the zero-group verdict.
- Atom cards keep the same `ProcessRowLink`, semantic address, AccessKit node,
  official `Button`, pointer activation, and secondary-press seam as dense
  rows. Keyboard/focus routing remains the shell's single authority. The
  optical card's hover, pressed and selected ink rides a state overlay above
  the glass, because an optical surface owns no painted fill to replace.

## Evidence and boundaries

- `tests/headless/pages/process_card_lane.rs` covers shared status pills,
  whole-group admission, focus facts, dense-mode seams, zero-group
  degradation, measured bounds, overflow activation, and the typed painted
  fallback without an adapter.
- `tests/headless/pages/card_lane_width_admission.rs` asserts the right-edge
  verdict before the renderer spends it: whole group width admission, a
  narrow slot's one-column degradation, the overflow action's reserved slot,
  and the typed zero-column degradation for a slot that holds no whole card.
- `tests/headless/optical_backdrop_mirror.rs` proves that a marked surface is
  mirrored at its measured rectangle, that a move or palette change bumps the
  composite revision, that an unchanged surface does not, and that stale
  mirrors are swept.
- `capture_bevy_scenarios.tsv` adds the light and dark `applications-card-lane`
  scenarios and the wide/compact `applications-card-lane-details-*`
  readiness scenarios. Their readiness waits for the measured optical hero,
  whole admitted group bounds, and — for the details scenarios — the shared
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
- Optical GPU readiness remains covered by the existing capability/GPU gates.
  The mirrored lower composite is proved headlessly and the nested optical
  chain is GPU-measured. The GPU receipt remains the only pixel proof of two
  simultaneously optical layers.
- The GPU receipt records `max_nested_layers=2` and
  `nesting_layers_proven=true` on this host's Intel Arc B390 (Vulkan)
  adapter. No two-layer optical screenshot is published yet — the receipt is
  the evidence, not an image.
