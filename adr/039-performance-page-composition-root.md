# ADR-039: One Performance page composition root and chart tier system

Status: accepted. This is the current composition decision for every GPUI
Performance device page; it retires the three parallel page shells.

## Context

The Performance page grew three independent composition roots: the shared
`main_with_stats` helper (disk/network/GPU/battery/fan), a bespoke Memory
column, and a bespoke CPU column. Height contracts (180/140/72), viewport
policies, `GraphOpts` aesthetic injection, summary rows, and the responsive
budget applied differently per page, and the mini-cell label pattern was
hand-copied in three places with drifting fonts and offsets.

## Decision

1. `perf_views::layout` is the ONLY page composition root. Every device page
   (CPU/Memory/Disk/Network/GPU/Battery/Fan) assembles through `perf_page`
   with typed slots: title, `header_extra`, `HeadlineSurface` (declared
   `ChartSpec`s), `below`, the statistics column, and the frame
   `PerformancePageBudget`. `performance_split`, `stats_panel`, and the card
   assembly are module-private; there is no compile-time path to a parallel
   shell.
2. A chart is declared, not assembled. `ChartSpec` + `ChartTier`
   (`Headline`/`Secondary`/`Compact`) derive the entire card contract in ONE
   place: height floor (180/140/72), growth, first-frame state overlay, hover surface,
   dual-series legend, the Batch-8 aesthetic injection, and the
   latest/avg/peak summary row. The value-format table (hover, badge,
   summary) is derived from the typed `GraphUnit`; scale policy stays a
   per-family input (`with_max` over the shared `finite_series_peak*`
   helpers). Mini density cells (CPU per-core, GPU engines) render through
   the one `elements::mini_graph_cell`.
3. The Performance workspace is ONE fixed, non-scrolling composition. The
   main viewport and the statistics rail never mount a scroll owner; only the
   left device selector may scroll. Headline charts absorb slack through
   `flex_1`, and every optional lower band is admitted only when its complete
   minimum footprint fits. A band that cannot fit is omitted or summarized
   before painting, so no bottom row is clipped or hidden behind an implicit
   scrollbar. The statistics rail follows the budget's `details`
   presentation — pinned, stacked below the viewport, or hidden — using the
   budget's `stats_width`.
4. GPU always keeps aggregate utilization as the large Headline chart. When
   the full inventory is admitted, per-engine utilization renders as a fine
   mini-card group below it, followed by one Compact GPU-memory utilization
   chart at the bottom. The engine group can never replace or resize the
   aggregate headline into a different semantic role.
5. Render-path assertions guard the root: every selectable device page must
   paint the shared title row and `tm-perf-main-viewport`, headline cards
   hold their tier floor (`tm-perf-chart-card:*`), and no page may mount a
   page-local scrolling main column.

All future responsive or dense-detail changes follow the repository's
[elastic layout playbook](../docs/ELASTIC_LAYOUT_PLAYBOOK.md): derive complete
slot footprints first, allocate primary space from the remainder, admit lower
groups atomically, and attach both headless bounds and current-build pixel
evidence before calling the layout complete.

## Minimum-space doctrine (three layers of floors)

Flexible layout never means unbounded shrinking. Space is bounded at every
layer, and each layer owns exactly one floor as a single source of truth;
the contract between layers is that a lower layer can never receive space
below the layer above's floor.

1. **Window layer (hard bound).** The compositor may not shrink the surface
   below `responsive::MIN_WIDTH × MIN_HEIGHT` (720×480), set once as the
   window's `window_min_size` from those constants — never a second literal.
   Windowed capture parsing clamps to the same constants.

   The page's complete vertical grammar, in order, with each slot's ladder
   contract:

   | Slot | Content | Drops at |
   |---|---|---|
   | title row | identity + context | never |
   | vital line | one-line distilled fact (disk capacity, VRAM totals, link state) | never |
   | header band | CPU readouts, memory composition | Floor |
   | headline surface | the tier-180 aggregate chart(s) | never |
   | data band | engine mini-cards, partition/directory panels, secondary charts, compact GPU-memory chart | Core |
   | stats rail | typed statistic column | width budget (Pinned/Stacked/Hidden) |

   Charts are the headline of a Performance page, but they are not its whole
   meaning: every page whose primary fact is not carried by a chart declares
   a `vital_line` so the Floor composition (title + vital + headline) still
   answers the page's question — a disk page never collapses to a chart-only
   surface.
2. **Budget layer (ordered degradation).** Width: the frame budget allocates
   typed slot floors (`PERFORMANCE_MAIN_MIN_WIDTH` 360, stats 236–280,
   device sidebar 220–460) and degrades Sidebar→Strip, Pinned→Stacked→Hidden
   when a floor cannot be met. Height: the typed
   `PerformanceVerticalRunway` ladder (`Charts` → `Core` → `Floor`) names
   exactly which fixed obligations may still render — the chart inventory
   drops at `Core`, and the header band plus per-chart summary rows drop at
   `Floor`, explicitly rather than by silent clipping. `chart_inventory`
   folds the two axes through `chart_inventory_from_axes`: two typed reasons
   in, one product bit out. The below band renders only from `Charts` up and
   yields first; page-specific caps and fit checks omit excess content before
   it reaches the fixed viewport. `Core` is a strict headline-only
   composition, so there is no overflow-tolerant scrolling or clipping tail.
3. **Tier layer (card floors).** `ChartTier::Headline` 180px,
   `Secondary` 140px, and `Compact` 72px are enforced on the card; mini cells
   keep their bounded grid row heights.

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
- The no-scroll policy is deliberate (see decision 3): lower content must be
  bounded, summarized, or dropped before it reaches the fixed viewport. Adding
  a Performance-page scroll owner requires replacing this ADR, not adding a
  second path.
- Test identities: headline `tm-perf-chart:{id}` / `tm-perf-chart-card:{id}`,
  secondary `tm-perf-secondary-graph:{id}`, summary `tm-perf-chart-summary:{id}`.
