# ADR-040: Optional setup is discovered without a startup modal

Status: accepted

## Context

TaskForest can discover an optional, package-owned Linux setup asset that
enables additional telemetry values. The capability is useful but is not
required for the monitor to start, render its normal pages, or keep the
standalone application usable.

The previous flow submitted the typed `Observe` request during startup and
opened the full First Run dialog as soon as the provider reported the asset.
That made a successful capability probe a recurring modal interruption. It
also made a machine with the optional package installed behave as though the
user had requested a setup action.

## Decision

1. Startup may continue to submit the typed setup `Observe` request so the
   frontend knows whether the optional capability is available. Observation
   is a background capability check, not a presentation command.
2. A successful observation never opens a modal surface. When setup is
   available, Settings exposes one explicit, localized entry point to review
   it; when it is unavailable, the entry is omitted.
3. The existing First Run surface remains the confirmation and progress
   surface for user-initiated View, Run, Revert, and Restart actions. Their
   typed request, authorization, failure, restart, and feedback semantics do
   not change.
4. A background observation failure stays silent and typed. It must not turn a
   transient provider problem into a startup error dialog.
5. Do not add a dismissal bit to configuration for this behavior. Since the
   startup modal is removed entirely, there is no repeated prompt to suppress;
   capability availability can also change when packages or helpers change.

## Consequences

- Normal startup is non-blocking and preserves the user's current page.
- The optional capability remains discoverable and accessible without hiding
  the existing setup workflow behind an undocumented shortcut.
- Standalone App and layer-shell presentation roles share the same typed
  discovery/action semantics; only the surface route differs.
- The fixed setup asset, native helper boundary, and application capability
  contract remain unchanged. No system file is removed or rewritten by the
  frontend.

## Verification

- The GPUI reducer test proves an available background observation leaves the
  First Run surface closed.
- The Settings visual behavior test proves the explicit entry is rendered and
  opens the First Run surface only after activation.
- GPUI and app-host checks, plus the controlled nested-Niri capture matrix,
  remain required before release claims are made.
