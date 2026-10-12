# SPI discipline

One SPI module per capability domain: `src/system.rs`, `src/process.rs`,
`src/storage.rs`, `src/sensor.rs`, `src/service.rs`, `src/power.rs`,
`src/environment.rs` and `src/integration.rs`. Each describes facts,
failures and provenance for its domain without choosing an OS, performing
I/O, owning worker threads or rendering a result.

Platform adapters implement these traits; semantics never change per
platform.

## Registration rules

Each registered capability has one provider, one route and one bounded lane.
Keep request/outcome types exhaustive and verify catalog completeness,
registration bijection and platform adapter conformance.

```bash
cargo nextest run -p taskmanager-platform-provider -j 4
```
