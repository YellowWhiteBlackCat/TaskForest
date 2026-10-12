# taskmanager-tray-muda

## Role

Shared tray/menu bridge for Windows and macOS adapters using `muda` and
`tray-icon`.

## Boundary

It maps toolkit-neutral tray intent to native menu events. It does not own
telemetry, application reducers, authorization or renderer window state.

## Contract and verification

Typed menu behavior and the verification route are specified in
[`docs/ADAPTER_NOTES.md`](docs/ADAPTER_NOTES.md).

## Module map

```text
src/lib.rs                     Windows/macOS muda tray adapter (Linux uses ksni, ADR-032)
```

## Docs

- [`docs/ADAPTER_NOTES.md`](docs/ADAPTER_NOTES.md) — mapping ownership, the Linux ksni
  divergence and verification.
- [ADR-032](../../adr/032-tray-neutral-contract-and-multi-adapter.md) — tray-neutral
  contract and multi-adapter decision.
