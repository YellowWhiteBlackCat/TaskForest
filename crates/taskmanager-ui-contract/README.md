# taskmanager-ui-contract

## Role

Toolkit-neutral UI contracts: pages, descriptors, commands, focus targets,
semantic snapshots, icon identities and user-facing message keys.

## Boundary

The crate has no renderer, OS API, provider, window object or business I/O. It
expresses intent and semantics that GPUI, Iced and TUI adapt independently.

## Contract and verification

Keep command IDs, page order, focus reachability, semantic roles and localized
message keys exhaustive and stable. Verify enum coverage and cross-frontend
command parity.
