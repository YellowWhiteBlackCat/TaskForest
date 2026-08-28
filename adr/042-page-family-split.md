# ADR-042: The page-family split — one shared data-page shell

Status: accepted. This is the current composition decision for every GPUI
top-level page outside the Performance chart surface; it retires the
per-page outer wrappers.

## Context

The eight top-level pages drifted into two composition styles with three
realizations of the same skeleton. The Performance page owns its chart
composition root (`tm-perf-main-viewport`, ADR-039); every other page
hand-built its own outer column: padded frame, status-bar placement, and
scroll contracts differed per page (Apps, Services, Users, Startup,
AppHistory, Containers, System), and the status bar had already started
drifting — individual pages grew slightly different outer flex wrappers. A
skeleton adjustment (padding, footer rule, scroll contract) had to be
replayed N times, once per page.

## Decision

1. Every top-level page declares a `PageFamily`: exactly one `Chart` surface
   (Performance) and every other page `Data`. The mapping lives in
   `gpui_app::root::navigation` and is exhaustive over `TopPage::ALL`.
2. The data family composes through the ONE shared outer shell:
   `taskmanager_ui::layout::PageScaffold` (flex column: padded
   `PageFrame` body + optional shell footer). The status bar is the
   shell-owned footer; pages no longer place it themselves.
3. List-style inventory pages (Services, Users, Startup) additionally share
   the ONE inner header+body split, `ListPageScaffold` in
   `gpui_app::list_view`, mounted inside the shared outer shell.
4. The chart surface keeps its own composition root (ADR-039); the two
   families never share a shell.
5. A render-path guard (`page_family_contract_tests`, mounted in
   `navigation.rs`) proves the split: every data page paints
   `tm-page-scaffold`; the chart surface paints `tm-perf-main-viewport`
   while never mounting the data shell. A page that grows its own outer
   wrapper fails here before the families can drift apart.
6. The telemetry-readiness marker `tm-telemetry-ready-body` lives on the
   shared `page_viewport` wrapper, never stamped onto the page body: the
   body owns its family selector, and re-stamping would erase it before the
   guard could observe it. (The first revision stamped it onto the page
   body and silently swallowed `tm-page-scaffold`; the guard caught it.)

## Consequences

- A data-page skeleton adjustment propagates to all seven pages from
  `PageScaffold` in one place; a list header/body change propagates from
  `ListPageScaffold`.
- A new data page is additive: declare its family, compose through the
  scaffold; the render-path guard enforces both.
- ADR-027 stays the single source of page identity (page table); this ADR
  owns only the shared data shell and its test identities:
  `tm-page-scaffold`, `tm-page-scaffold-footer`, `tm-list-page-scaffold`.
- The readiness marker's meaning narrows to "the shared viewport carries a
  ready page"; existing positive assertions keep passing because the
  wrapper paints with the body.