# Presentation contracts

Navigation uses bounded horizontal tabs or a fixed scrollable vertical rail.
Apps exposes category/application totals and real parent/child rows. Totals
have no representative PID: batch verbs freeze their live subtree;
single-process Properties and affinity remain unavailable on aggregate rows.
Selection follows the rendered semantic order for both pointer and keyboard.

Small/Standard/Large sets GPUI rem to 14/16/18px and shared control metrics;
row density remains independent. The process header and virtualized body share
one horizontal scroll owner. Name stays the leading identity column. Tree
Left/Right collapses, expands or selects an ancestor; leaf/modifier navigation
retains column cursor and sort actions. Typed chrome budgets keep the table's
minimum height, with secondary commands reachable through the actions menu.

Every live Performance device page composes through `perf_views::layout`.
`ChartSpec` tiers own chart floors, state overlays, hover, legends and summary
rows. Live charts and statistics stay in a fixed main viewport; the device rail
alone scrolls at page level. CPU has a bounded nested details viewport. GPU
keeps its headline, optional engine cards and memory group without inventing
unsupported families. Disk/network directions share a peak and static grid but
have separate scene keys and independent hover facts.

Device-sidebar width is clamped to current page slots before pinning its
edges. If the rail cannot coexist with the main viewport, the same devices use
the strip. The pinned scrollbar leaves a dedicated resize gutter. Drag order
comes from the same immutable row props as paint, with no render-time writable
order mirror.

Dashboard windows and readouts use application-correlated system history.
Controls and paging stay fixed, all four complete metric cards remain
reachable, and paint and hover use the selected timeline's actual count.
Durable History shows the shared 1h/24h/7d replay projections; live process
rings never become its authority. Recorded curves keep gaps and signed ranges
with separate per-window scrolling.

Startup fixed chrome keeps actions and bounded source notices outside its
primary table. `StartupPageBudget` admits the timeline as collapsed, bounded
stacked or a side panel. Height pressure preserves failures, retry actions and
exact omitted counts instead of hiding the table or silently dropping timeline
facts.

Properties preserves frozen name/PID while looking up live values by exact
key. Overview exposes available PSS/USS/Swap; Performance carries current and
peak readouts with honest recent samples. Long values and all insight rows
remain reachable in one bounded body. Closing or losing the frozen process
cannot retarget the window through the table selection.

SMBIOS uses shared inventory rows, configured units and wrapped selectable
values. Permission entry stays in Settings. The shared scrollbar uses
viewport/max-offset geometry, keyed drag state and paint-safe invalidation
with one edge inset.

When a tray exists, window close minimizes while root/runtime/singleton remain
alive; tray Quit terminates. Secondary launch activates the existing window.
Capture-only and tray-unavailable sessions may close without leaving a
process.

Layout changes additionally follow the repository elastic-layout playbook and
the measured-bounds gate in [`QUALITY_GATES`](../../../docs/QUALITY_GATES.md).
