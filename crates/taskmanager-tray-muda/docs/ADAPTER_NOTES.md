# Adapter notes

The crate maps toolkit-neutral tray intent to native menu events using `muda`
and `tray-icon` on Windows and macOS. Menu identity, show/hide/quit actions
and unavailable native tray behavior are typed.

Linux is intentionally outside this crate: it uses the `ksni` adapter under
the neutral tray contract (ADR-032), so this crate carries no Linux code.

The crate does not own telemetry, application reducers, authorization or
renderer window state; platform-specific event loops stay at the composition
edge.

## Verification

Keep target cfg compilation green and platform-specific event loops at the
composition edge:

```bash
cargo nextest run -p taskmanager-tray-muda -j 4 --all-targets
```

See [ADR-032](../../../adr/032-tray-neutral-contract-and-multi-adapter.md) for the
tray-neutral contract and multi-adapter decision.
