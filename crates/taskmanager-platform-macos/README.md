# taskmanager-platform-macos

## Role

macOS application-port adapter composition using safe system APIs, mature
crates and bounded compatibility commands.

## Boundary

OS paths and command parsing stay here. Unsupported GPU, container, network or
control features remain typed; Linux assumptions never enter shared models.
Abilities without receipts stay typed `Unsupported` (ADR-019).

## Contract and verification

Register the same platform SPI as other adapters and preserve partial/provider
failure semantics. Typed collection rules for launchd, CPU/memory, GPU and
runtime registration live in
[`docs/COLLECTION_RULES.md`](docs/COLLECTION_RULES.md).

Linux CI proves compile/contract coverage; live claims require a macOS run of
`../../scripts/quality/native-platform-fact-safety.sh` and its `.tmp/` receipt.

## Module map

```text
src/provider.rs                        capability provider implementations
├── process/  process.rs               process control and telemetry
├── process_facts.rs                   per-process fact assembly
├── storage/  storage.rs               disk inventory and SMART
├── system/  system.rs                 system telemetry and hardware
├── sensor.rs  service.rs              thermal sensors and launchd services
├── environment.rs  integration.rs     environment and integration providers
├── power.rs  gpu.rs                   power and GPU providers
├── directory_usage.rs                 directory size scanning
src/bindings.rs                        safe system-API bindings with typed degradation
src/instance.rs  config.rs  tray.rs  window_capture.rs
```

## Docs

- [`docs/COLLECTION_RULES.md`](docs/COLLECTION_RULES.md) — typed collection and
  registration rules, including the process-identity `Unsupported` boundary.
- [ADR-019](../../adr/019-macos-telemetry-safety.md) — macOS telemetry safety.
