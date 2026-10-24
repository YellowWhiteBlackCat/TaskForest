# taskmanager-platform-windows

## Role

Windows application-port adapter composition using safe crates and the minimal
typed native boundary in `taskmanager-windows-api`.

## Boundary

Provider code never uses PowerShell, CMD or a command interpreter for telemetry.
Unsupported fields remain typed; native handles and buffers stay inside the
audited boundary. Foreign-process escalation crosses the ADR-035 UAC
transport; its one-shot reply-file protocol, compile verification and
receipt-before-release-claim rule are specified in
[`docs/UAC_TRANSPORT.md`](docs/UAC_TRANSPORT.md).

## Contract and verification

Use exact process identity, bounded enumeration, checked OS lengths and typed
SCM/session/locale/topology outcomes. GPU joins are exact-LUID only; CPU,
memory and service assemblies build typed groups before one snapshot
construction and never write schema-v1 mirrors. Per-domain collection rules
and the process control/insight inventory (suspend/resume, resource limits,
open files, WSL, startup, environment) are specified in
[`docs/COLLECTION_RULES.md`](docs/COLLECTION_RULES.md) and
[`docs/PROCESS_CONTROL_AND_INSIGHTS.md`](docs/PROCESS_CONTROL_AND_INSIGHTS.md).

Linux proves cross-target contracts; a Windows native run of
`../../scripts/quality/native-platform-fact-safety.sh` is required for
real numbers and permissions; pixels additionally require the Windows capture
route.

## Module map

```text
src/provider/
├── process/                   list, insights, control, uac (ADR-035 transport)
├── system/                    cpu_freq, cpu_info, cpu_throttle, disk, gpu,
│                              hardware_inventory, msr_readout, network, auxiliary
├── service/ (+ log_runtime)  storage/  environment/ (+ sessions, boot_evidence)
├── power/  sensor/  integration/
src/bindings.rs                taskmanager-windows-api consumption wrappers
src/command.rs  config.rs  instance.rs  local_time.rs  tray.rs  window_capture.rs
```

No PowerShell/CMD anywhere; native calls stay inside the ADR-031 boundary.

## Docs

- [`docs/COLLECTION_RULES.md`](docs/COLLECTION_RULES.md) — GPU/PDH, CPU/memory, services
  and winevt collection rules.
- [`docs/PROCESS_CONTROL_AND_INSIGHTS.md`](docs/PROCESS_CONTROL_AND_INSIGHTS.md) — process
  control, resource limits, insights, WSL, startup and local-time synthesis.
- [`docs/UAC_TRANSPORT.md`](docs/UAC_TRANSPORT.md) — the ADR-035 escalation transport.
- [ADR-031](../../adr/031-windows-native-safe-boundary.md) and
  [ADR-035](../../adr/035-windows-uac-foreign-process-control-transport.md) — boundary and
  transport decisions.
