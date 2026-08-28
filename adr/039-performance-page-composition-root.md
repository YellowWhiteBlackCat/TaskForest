# ADR-039: One Performance page composition root and chart tier system

Status: accepted. This is the current composition decision for every GPUI
Performance device page; it retires the three parallel page shells.

## Context

The Performance page grew three independent composition roots: the shared
`main_with_stats` helper (disk/network/GPU/battery/fan), a bespoke scrolling
Memory column, and a bespoke CPU column. Height contracts (180/140/0/190),
viewport policies (fixed clip vs scrolling vs none), `GraphOpts` aesthetic
injection, summary rows, and the responsive budget applied differently per
page, and the mini-cell label pattern was hand-copied in three places with
drifting fonts and offsets.

## Decision

1. `perf_views::layout` is the ONLY page composition root. Every device page
   (CPU/Memory/Disk/Network/GPU/Battery/Fan) assembles through `perf_page`
   with typed slots: title, `header_extra`, `HeadlineSurface` (declared
   `ChartSpec`s, or one `Custom` replacement such as the GPU engine
   inventory), `below`, the statistics column, and the frame
   `PerformancePageBudget`. `performance_split`, `stats_panel`, and the card
   assembly are module-private; there is no compile-time path to a parallel
   shell.
2. A chart is declared, not assembled. `ChartSpec` + `ChartTier`
   (`Headline`/`Secondary`) derive the entire card contract in ONE place:
   height floor (180/140), growth, first-frame state overlay, hover surface,
   dual-series legend, the Batch-8 aesthetic injection, and the
   latest/avg/peak summary row. The value-format table (hover, badge,
   summary) is derived from the typed `GraphUnit`; scale policy stays a
   per-family input (`with_max` over the shared `finite_series_peak*`
   helpers). Mini density cells (CPU per-core, GPU engines) render through
   the one `elements::mini_graph_cell`.
3. The main column is ONE fixed viewport (never a scrolling body): headline
   charts absorb slack through `flex_1`, secondary content compresses to its
   tier floor, and content that still cannot fit is clipped. The statistics
   rail follows the budget's `details` presentation — pinned, stacked below
   the viewport, or hidden — using the budget's `stats_width`.
4. The GPU primary engine card is a Headline chart (hover, state overlay,
   summary), not a bare card; its identity caption carries the live
   per-engine readout.
5. Render-path assertions guard the root: every selectable device page must
   paint the shared title row and `tm-perf-main-viewport`, headline cards
   hold their tier floor (`tm-perf-chart-card:*`), and no page may mount a
   page-local scrolling main column.

## Minimum-space doctrine (three layers of floors)

Flexible layout never means unbounded shrinking. Space is bounded at every
layer, and each layer owns exactly one floor as a single source of truth;
the contract between layers is that a lower layer can never receive space
below the layer above's floor.

1. **Window layer (hard bound).** The compositor may not shrink the surface
   below `responsive::MIN_WIDTH × MIN_HEIGHT` (720×480), set once as the
   window's `window_min_size` from those constants — never a second literal.
   Windowed capture parsing clamps to the same constants.
2. **Budget layer (ordered degradation).** Width: the frame budget allocates
   typed slot floors (`PERFORMANCE_MAIN_MIN_WIDTH` 360, stats 236–280,
   device sidebar 220–460) and degrades Sidebar→Strip, Pinned→Stacked→Hidden
   when a floor cannot be met. Height: the typed
   `PerformanceVerticalRunway` ladder (`Charts` → `Core` → `Floor`) names
   exactly which fixed obligations may still render — the chart inventory
   drops at `Core`, and the header band plus per-chart summary rows drop at
   `Floor`, explicitly rather than by silent clipping. `chart_inventory`
   folds the two axes through `chart_inventory_from_axes`: two typed reasons
   in, one product bit out. The below band is the overflow-tolerant tail: it
   renders from `Core` up and yields first; what it cannot fit under the
   ladder is clipped by the fixed viewport — the ordered last resort of the
   deliberate decision 3 policy, never a silent default.
3. **Tier layer (card floors).** `ChartTier::Headline` 180px and
   `Secondary` 140px are enforced on the card, and mini cells keep their grid
   row heights.

Feasibility guarantee: the minimum viable page (shell chrome + title row +
headline floor ≈ 320px) fits inside the window-layer floor's worst content
height (≈ 400px), so the ladder is always satisfiable within the product
contract — no page can present an incoherent composition; the guards assert
the headline floor at the 720×480 minimum and prove the ladder's drop order
(matrix → header band/summaries) on fresh windows per rung.

## Consequences

- New device pages or chart families are additive declarations; they cannot
  fork the viewport, height, aesthetic, or summary contract.
- Memory and CPU lose their bespoke columns and scroll bodies; Memory/CPU
  content composes into the window like every other device page.
- The clipping policy for over-tall `below` content is deliberate (see
  decision 3); introducing scrolling for that case requires replacing this
  ADR, not adding a second path.
- Test identities: headline `tm-perf-chart:{id}` / `tm-perf-chart-card:{id}`,
  secondary `tm-perf-secondary-graph:{id}`, summary `tm-perf-chart-summary:{id}`.
