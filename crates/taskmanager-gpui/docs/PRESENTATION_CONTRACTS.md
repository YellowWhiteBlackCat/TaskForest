# Presentation contracts

Navigation uses bounded horizontal tabs or a fixed scrollable vertical rail.
Apps exposes category/application totals and real parent/child rows. Totals
have no representative PID: batch verbs freeze their live subtree;
single-process Properties and affinity remain unavailable on aggregate rows.
Selection follows the rendered semantic order for both pointer and keyboard.
Apps control bands spend one shared inset `GlassSurfaceRole::PageHeader`: the
Card-tier fill, hairline and radius remain, while the standalone action strip
and wide unified control band no longer stack a floating elevation shadow.

Small/Standard/Large sets GPUI rem to 14/16/18px and shared control metrics;
row density remains independent. The process header and virtualized body share
one horizontal scroll owner. Name stays the leading identity column. Tree
Left/Right collapses, expands or selects an ancestor; leaf/modifier navigation
retains column cursor and sort actions. Typed chrome budgets keep the table's
minimum height, with secondary commands reachable through the actions menu.

Every live Performance device page composes through `perf_views::layout`.
`ChartSpec` tiers own chart floors, state overlays, hover, legends and summary
rows. Live charts and statistics stay in a fixed main viewport; the device rail
alone scrolls at page level. Chart frames, first-frame state capsules and
summary pills spend one inset `GlassSurfaceRole::InlinePlot`: the shared
Card-tier material stays unchanged while the parent card carries elevation.
CPU has a bounded nested details viewport. GPU
keeps its headline, optional engine cards and memory group without inventing
unsupported families. Disk/network directions share a peak and static grid but
have separate scene keys and independent hover facts.

Detaching one chart from this viewport is a typed, fail-closed contract, not a
second renderer. `detached_chart_window::DetachedChartWindowState` freezes a
`DetachedChartKey` and applies `Detach`/`Reattach` commands; the embedded mode
is the default. `ChartWindowCapability::current()` is `Unavailable` in this
build because no verified second-window adapter exists, so a detach never
reaches a window request and no UI surface may advertise a floating chart.
`LayerShellSurface` names the Wayland request only; existing
`window_presentation` layer-shell support belongs to the main desktop-widget
host and is not evidence of a detached chart protocol. A granted session (only
representable in tests today) reattaches exclusively through its frozen key,
and duplicate or foreign requests leave the session unchanged or fail with a
typed error. Behavior is pinned in
`tests/headless/detached_chart_window_tests.rs`; no host geometry claims are
made because no visible window is created.

Device-sidebar width is clamped to current page slots before pinning its
edges. If the rail cannot coexist with the main viewport, the same devices use
the strip. The pinned scrollbar leaves a dedicated resize gutter, transparent
at rest and discoverable by hover or keyboard focus. Pointer dragging, arrow
keys and accessibility increment/decrement actions share the same width bounds.
Drag order comes from the same immutable row props as paint, with no
render-time writable order mirror.

Dashboard windows and readouts use application-correlated system history.
Controls and paging stay fixed, all four complete metric cards remain
reachable, and paint and hover use the selected timeline's actual count.
Primary Dashboard metric and history cards spend one floating
`GlassSurfaceRole::PageCard`; compact-widget metric cards reuse that owner.
The shared Card-tier fill, hairline and radius remain unchanged; this role
records that these cards belong to the primary page and carry its elevation.
Geometry and slot budgets are unchanged.
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

The GPU insight facet's summary and engine cards are two mandatory whole
groups. At a 650px post-rail content slot they spend two equal 321px columns
with the shared `SPACE_8` gap; below that budget they stack as complete groups
inside the same bounded scroll owner. Card widths follow the admitted slot, so
long device IDs and engine lines cannot push content past the right edge.

SMBIOS uses shared inventory rows, configured units and wrapped selectable
values. Permission entry stays in Settings. The shared scrollbar uses
viewport/max-offset geometry, keyed drag state and paint-safe invalidation
with one edge inset.

System Health keeps storage and sensor sections as complete surfaces under one
page-owned bounded scroll rail. Section cards spend one floating
`GlassSurfaceRole::PageCard`; filesystem rows, sensor groups and sensor rows
spend the same owner as insets, so nested surfaces never stack a second shadow.
The typed single-column layout reduces section, filesystem-row, sensor-group
and sensor-row insets as well as the gap between stacked sections, while the
multi-column grid keeps the regular gap; compact windows retain more readable
facts without shrinking labels or splitting groups.

Root navigation controls spend the theme's derived `hover_bg()` token, not a
private accent-alpha blend. Tabs, the settings gear and the orientation
affordance share the same hover ink the sidebar, rows and other frontends use.

When a tray exists, window close minimizes while root/runtime/singleton remain
alive; tray Quit terminates. Secondary launch activates the existing window.
Capture-only and tray-unavailable sessions may close without leaving a
process.

Layout changes additionally follow the repository elastic-layout playbook and
the measured-bounds gate in [`QUALITY_GATES`](../../../docs/QUALITY_GATES.md).
