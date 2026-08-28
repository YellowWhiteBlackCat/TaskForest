# ADR-041: Compositor evidence and gamescope boundary

Status: accepted

## Context

The frontend host contract deliberately supports both standalone windows and Wayland
layer-shell surfaces. The evidence route must therefore distinguish pixels rendered by
the current build from compositor semantics owned by the host.

A nested Niri instance can create its Wayland and IPC sockets, while its winit backend
stops answering IPC after a normal client maps. TaskForest can still emit its readiness
markers in that state. That is an environment/backend failure, not evidence that the
application rendered correctly or incorrectly.

Gamescope can host a nested Wayland application at a controlled output size and can
produce a compositor screenshot for a normal single-app surface. It also advertises a
layer-shell global, but that fact alone does not establish complete layer-surface
composition, screenshot coverage, focus behavior, or desktop window-management semantics.

## Decision

1. Keep a real target desktop compositor as the acceptance authority for layer-shell
   configure/ack, anchors, margins, exclusive zones, keyboard interactivity, output
   selection, close/restart and normal-window interaction.
2. If nested Niri IPC becomes unresponsive after client mapping, classify the capture as
   `BLOCKED (compositor/backend)`. The command remains non-successful so it cannot publish
   incomplete evidence, but it must not be reported as a TaskForest product failure or
   replace accepted screenshots.
3. Permit gamescope as an auxiliary single-app pixel backend for standalone rendering and
   responsive-layout review. A future repository route must provide its own source manifest,
   readiness markers, PNG receipt and independent validator before it can publish evidence.
4. Do not change the standalone default or infer layer-shell acceptance from a gamescope
   run. The layer-shell opt-in and the independent app path remain separate.

## Consequences

- A successful gamescope pixel capture can unblock visual inspection of the shared GPUI
  layout while Niri is blocked, without weakening the compositor gate.
- A gamescope capture cannot certify window identity, tiling/focus policy, output rules or
  layer-shell behavior. Those checks remain `SKIP` until a suitable target compositor is
  available.
- Accepted screenshots remain immutable with respect to a blocked or partial run; all
  failed evidence stays in ignored local storage for diagnosis.

## Verification

- The quality gate and public screenshot policy describe the three outcomes: PASS, BLOCKED
  for an unavailable compositor/backend, and SKIP for an unverified capability.
- Standalone and layer-shell routes are exercised separately; the normal window path remains
  the default when the layer-shell opt-in is absent.
