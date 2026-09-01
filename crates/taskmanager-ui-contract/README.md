# taskmanager-ui-contract

## Role

Toolkit-neutral UI contracts: pages, descriptors, commands, focus targets,
semantic snapshots, icon identities, user-facing message keys, the
cross-frontend keybinding coverage matrix, the CORE-04 product-intent matrix,
and the component/surface capability parity registry (CORE-08:
`taskmanager-ui` is the reference shape — GPUI-05; every deliberate frontend
difference is a reasoned, gate-checked registry entry, never prose parity
claims).

## Boundary

The crate has no renderer, OS API, provider, window object or business I/O. It
expresses intent and semantics that GPUI, Iced, TUI, and Bevy adapt independently.

## Contract and verification

Keep command IDs, page order, focus reachability, semantic roles, localized
message keys and product intents exhaustive and stable. Verify enum coverage,
cross-frontend command parity, and explicit functional surface decisions.
Component capability registration is likewise exhaustive: `ColumnDragResize`
is a component-level contract whose GPUI reference is
`taskmanager-ui/src/data/table/resize.rs`; page identity and persistence stay
outside this crate.

## Module map

```text
src/functional.rs                CORE-04 registry: product intent × frontend surface
│                                decision (shared/local/accepted-difference/unsupported)
src/command.rs  keybindings.rs  navigation.rs  focus.rs  message.rs
                                 shared interaction vocabulary
src/columns.rs  icon.rs  capabilities.rs   column, icon and capability contracts
src/accessibility/               neutral accessibility model and snapshots
```
