# ADR-046: Bevy as the fourth shared-contract frontend shape

Status: accepted

## Context

TaskForest now documents four frontend peers: GPUI, Iced, Ratatui, and Bevy.
ADR-029 remains correct for the root `taskmanager` binary and its three
feature-gated `ui-*` shapes, but Bevy is built as the separate `taskforest-b`
source-build binary. The shared keybinding, product-intent, and component
capability registries still named only the three root shapes, so Bevy could
pass its own frontend tests without being represented in shared coverage.

## Decision

1. Keep ADR-029's one-binary contract unchanged: the root `taskmanager`
   package continues to select exactly one of GPUI, Iced, or Ratatui through
   its `ui-*` features. Bevy is not added as a fourth root feature.
2. Add Bevy to the toolkit-neutral `FrontendShape` contract and require an
   explicit Bevy declaration for every shared command binding, product intent,
   and component capability.
3. Bevy may declare `Unsupported` or `AcceptedDifference` while its maturity
   is below GPUI, but every such decision carries a non-empty reason. A Bevy
   declaration never claims GPUI reference ownership.
4. Shared registries remain free of Bevy types. Bevy-native input, scene,
   accessibility, and widget state stay in `taskmanager-bevy-ui`.
5. Bevy scoped gates run its declarations, behavior tests, and capture route;
   the release artifact matrix continues to package GPUI only.

## Consequences

- A new shared command, intent, or component cannot silently omit the fourth
  frontend; the Bevy declaration must change in the same contract update.
- Four-peer declaration coverage does not by itself claim behavioral parity.
  Bevy's accepted differences and unsupported surfaces remain visible until
  their own behavior and pixel evidence exists.
- The old three-shape root build remains stable, while the shared semantic
  contract accurately describes the separate Bevy product surface.

## Verification

`FrontendShape::ALL` contains all four peers. `taskmanager-ui-contract` keeps
the registry fold and drift rules, while `taskmanager-bevy-ui` tests assert a
complete binding, functional, and capability declaration. The Bevy scoped
quality gate remains the evidence entry point for its implementation.
