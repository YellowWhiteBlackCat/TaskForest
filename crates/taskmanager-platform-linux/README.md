# taskmanager-platform-linux

## Role

Linux application-port adapters and bounded collectors for `/proc`, `/sys`,
systemd/OpenRC, SMART, hwmon, DRM, cgroup and Linux controls.

## Boundary

The crate translates Linux facts into platform contracts; it never exposes
Linux paths, raw handles or provider choice to core/application/frontends.
Privilege flows through escalation helpers and stable identity checks.

Display, compositor, package, GPU and capture enrichments are
capability-optional and never fabricate support; their rules and the
Linux-not-cross-platform-evidence rule live in
[`docs/CAPABILITY_ENRICHMENTS.md`](docs/CAPABILITY_ENRICHMENTS.md).

## Key modules

- `src/backend/` groups application-port implementations by capability family.
- `src/engine/` owns bounded `/proc`, `/sys`, command, hardware and control engines.
- `src/provider/` registers the Linux implementations and translates them into the platform SPI.

## Contract and verification

Missing files, permission, malformed input, timeout, PID/device replacement and
recovery are typed. Keep I/O bounded and off UI threads; verify parser fixtures,
provider behavior, live smoke and target hardware receipts separately.
Typed observation assembly rules for time, services, power, CPU, memory and
GPU live in [`docs/COLLECTION_SEMANTICS.md`](docs/COLLECTION_SEMANTICS.md).

## Module map

```text
src/engine.rs                          collection engine root
├── collector/                         domain collection orchestration
├── hardware/                          hardware inventory and topology
├── nvml.rs                            NVIDIA NVML GPU telemetry
├── power.rs                           power/RAPL collection
├── process/                           per-process telemetry collection
├── sensors/                           thermal/fan sensor collection
├── services/                          systemd service collection
├── smart/                             SMART disk health collection
├── startup/                           startup-item collection
├── runtime_evidence/                  runtime evidence capture
├── desktop_appearance.rs              desktop theme detection
├── session.rs  storage_health.rs  storage_target.rs
src/provider.rs                        capability provider implementations
├── process.rs  process_target.rs      process control and targeting
├── environment.rs  integration.rs     environment and integration providers
├── power.rs  rapl_power.rs            power and RAPL providers
├── sensor.rs  service.rs  storage.rs  system.rs
├── gpu_engine_rows.rs  npu_inventory.rs  msr_readout.rs  smbios_memory.rs
├── directory_usage.rs  source_status.rs
src/backend.rs                         environment, integration, power, process, sensor,
│                                      service, storage, system control backends
src/platform_handle/                   native platform handle with polkit bindings
src/config.rs  instance.rs  local_time.rs  tray.rs  window_capture.rs
```

SMART self-test control is one fixed `taskforest-smart-self-test-helper` polkit
operation; it revalidates persistent identity and exact self-test capability
before starting the device command. Other control paths go through
taskmanager-escalation (polkit) or udisks2 (eject, MC !493).

## Docs

- [`docs/COLLECTION_SEMANTICS.md`](docs/COLLECTION_SEMANTICS.md) — local time, services,
  power, CPU/memory and GPU typed observation assembly.
- [`docs/CAPABILITY_ENRICHMENTS.md`](docs/CAPABILITY_ENRICHMENTS.md) — optional enrichment
  rules, capture adapter and polkit registration discipline.
- [`CROSSPLATFORM_STRATEGY`](../../docs/CROSSPLATFORM_STRATEGY.md) and
  [`TELEMETRY_MANIFEST`](../../docs/TELEMETRY_MANIFEST.md) — platform collection strategy.
