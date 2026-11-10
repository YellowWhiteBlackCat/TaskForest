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
- The static hero card is the only surface marked for real optical glass.
  The scrolling atomic cards use painted glass components. The shared
  capability decision still owns headless, high-contrast, software-adapter,
  and no-adapter fallbacks. Nested optical glass is deliberately **not**
  claimed.

## Layout contract

- The page slot is measured once. Bevy subtracts its own fixed card-lane
  chrome, then spends the shared `admit_card_groups` verdict: complete groups
  only, a clamped card height, and zero groups when the floor does not fit.
- Zero groups degrade to the dense table; a compact viewport therefore never
  paints a half group or a clipped last card.
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
  rows. Keyboard/focus routing remains the shell's single authority.

## Evidence and boundaries

- `tests/headless/pages/process_card_lane.rs` covers chips, whole-group
  admission, focus facts, dense-mode seams, zero-group degradation, measured
  bounds, and the typed painted fallback without an adapter.
- `capture_bevy_scenarios.tsv` adds the light and dark `applications-card-lane`
  scenarios. Their readiness waits for the measured optical hero and whole
  admitted group bounds. Compact windows prove their zero-group degradation in
  headless bounds tests, rather than pretending painted card groups exist.
- Targeted Wayland captures of the light and dark card lane, the tooltip
  overlay and the settings switch focus were run on this host and reviewed
  by eye: the atomic cards paint their name/status and PID/CPU/memory facts,
  and the group header states the omitted rows. The full durable matrix was
  not published (the run stayed targeted).
- Optical GPU readiness remains covered by the existing capability/GPU gates,
  and nested optical glass remains an unproven boundary.
