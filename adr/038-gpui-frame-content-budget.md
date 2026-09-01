# ADR-038: GPUI frame budget and semantic page slots

## Status

Accepted

## Context

GPUI pages previously classified responsive behavior from the outer window size.
That value included navigation, optional CSD chrome, alerts, and page padding, so
the same page could receive a different effective width than the policy had
classified. Performance also allowed a persisted device sidebar and a fixed
statistics rail to consume the main graph's readable width.

TaskForest must keep the standalone desktop application and the layer-shell
widget as separate surfaces. A responsive policy change in the desktop shell
must not become a widget-specific page fork or mutate persisted preferences.

## Decision

The GPUI root computes one immutable `FrameBudget` per render. It deducts the
actual shell regions known at that boundary: navigation orientation and outer
insets, horizontal navigation height, CSD titlebar when present, and the active
alert band. `FrameBudget` produces a `ContentBudget` whose width and height are
the inputs for page policy. `PageLayoutBudget` remains the compact typed page
projection for existing page adapters.

Performance maps the content budget once into semantic slots: device navigation,
main viewport, and statistics rail. The persisted sidebar width is a preference,
not an authority over current geometry. When three readable columns do not fit,
the device navigation becomes a strip; the statistics rail becomes stacked or,
only at the smallest capacity, hidden. The primary viewport keeps its minimum
readable width.

The normal desktop RootView remains the standalone surface. The layer-shell
DesktopWidget branch continues to render its compact widget projection without
the desktop titlebar, navigation, page frame, or Performance slot policy.

## Consequences

- All GPUI pages share one resize-time capacity projection.
- Page modules do not re-read window pixels or subtract shell regions locally.
- Long locales and short vertical rails keep their existing local scroll
  contracts; Performance is the explicit exception: only its left device
  selector scrolls, while the main viewport and statistics rail remain fixed
  and lower content is bounded before painting.
- Slot allocation is pure and headlessly testable across width, height,
  orientation, CSD, alert, and persisted-sidebar inputs.
- The page-specific `PerformancePageBudget` still owns semantic composition;
  individual metric cards do not acquire responsive booleans.

## Verification

The GPUI responsive tests cover shell deduction, vertical navigation insets,
sidebar clamping, pinned/stacked transitions, hidden-sidebar device access, and
the existing standalone page geometry suite. Widget and standalone surface
selection remain covered by their existing neutral presentation tests.
