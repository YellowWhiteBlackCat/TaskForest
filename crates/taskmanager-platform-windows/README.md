# taskmanager-platform-windows

## Role

Windows application-port adapter composition using safe crates and the minimal
typed native boundary in `taskmanager-windows-api`.

## Boundary

Provider code never uses PowerShell, CMD or a command interpreter for telemetry.
Unsupported fields remain typed; native handles and buffers stay inside the
audited boundary. Foreign-process escalation crosses the ADR-035 UAC transport:
`provider::process::uac` pre-creates the one-shot randomly named reply file,
builds the fixed helper command line with the escalation crate's pure builder,
and drives the audited `runas` call group; every raw result is classified by
`taskmanager-escalation::uac`'s typed transport facts. The crossing is
compile-verified (`x86_64-pc-windows-msvc`) and included in the Windows MSI
next to the GPUI executable; a development or damaged install still surfaces
the honest typed `HelperUnavailable`. Native success/denial has to be
receipted on a Windows desktop before it becomes a release claim. A normal
inherited-token child is never treated as elevation.

## Contract and verification

Use exact process identity, bounded enumeration, checked OS lengths and typed
SCM/session/locale/topology outcomes. Every target-scoped process read validates
the creation token before and after its native query; reveal validates before
desktop side effects. GPU telemetry joins DXGI/PDH only by exact adapter LUID,
and unavailable samples never borrow a sibling or become zero. DXGI and NVML
enumeration ceilings publish typed partial status instead of silently truncating
a healthy inventory. Process resources publish sysinfo memory as a typed current
observation and job-object/limit families as typed unsupported facts; they never
write schema-v1 resource mirrors directly. Linux proves cross-target contracts; a
Windows native run of `../../scripts/quality/native-platform-fact-safety.sh` is required for
real numbers and permissions; pixels additionally require the Windows capture route.
SCM inventory dependencies are assembled once as typed `ServiceRelationGraph`
edges; legacy relation strings are projected only by core serialization.
Service log snapshots and incremental streams come from the boundary's winevt
(`EvtQuery`/`EvtSubscribe`) surface against user-readable channels; access
denials and unreadable channels stay typed, and a silent service is an honest
empty snapshot. Boot evidence reads the Diagnostics-Performance boot event
through the same surface, and the session Lock action is the documented
`LockWorkStation` call on the calling session.
CPU usage/frequency/thermal slots and memory scalar/optional families are
assembled as typed groups before one snapshot construction. Partial per-core
frequency coverage remains `Partial`, unsupported optional memory facts remain
unavailable, and provider code never writes compatibility mirrors. The CPU
energy-preference slot carries the effective power-overlay label (mapped from
the documented overlay GUIDs; unknown GUIDs stay absent).
NVML and DXGI assemble canonical GPU observations independently, then merge by
exact PCI/LUID identity with existing per-field precedence. Engine-utilization
rows (`telemetry.gpu.engines`) are served from PDH per-engine data without
elevation — the Linux lane's privileged PMU helper has no Windows counterpart
need. Per-process GPU utilization and memory aggregate the PDH
`pid_<pid>` engine and `GPU Process Memory` instances, with NVML kept only as
an NVIDIA supplement; WDDM's designed NVML unavailability is never rendered as
a believable zero. Throttle masks have
explicit availability (including confirmed empty and future-bit `Other`), and
providers never write schema-v1 GPU mirrors.
Process control maps Suspend/Resume onto the documented per-thread path
(ToolHelp32 snapshot + `OpenThread(THREAD_SUSPEND_RESUME)` +
`SuspendThread`/`ResumeThread`); the undocumented `NtSuspendProcess` is
deliberately not used and the two mechanisms are never mixed. Thread rows
carry descriptions and CPU time via `GetThreadDescription`/`GetThreadTimes`
with identity-bound rate baselines. Resource limits ride boundary-owned
nested jobs (memory / process count / whole-number CPU percent); they only
tighten, never survive app exit (session-scoped — presented as such), and a
fractional CPU quota is rejected rather than rounded.
The open-files insight walks the system handle table through the boundary
(same-user processes; other users stay typed `PermissionDenied`) with the
sacrificial-thread name resolution the named-pipe deadlock forces. Memory
telemetry reports the compression store size from the kernel process
snapshot (absent store = honest absence, never zero). Hardware inventory
includes monitors via EnumDisplayDevices + registry-cached EDID parsed by
the shared portable parser, and NPU inventory via the SetupAPI compute
accelerator class with utilization deliberately typed-unavailable until a
counter-set receipt exists.
The container rollup is the WSL view: one row per registered distribution
(LXss inventory), with running distributions sampled through the fixed
`wsl.exe` program channel (`--list --running`, `--exec ls/cat` on `/proc` —
never a shell). Stopped distributions are never sampled (executing against
them cold-boots the utility VM) and keep typed-unavailable metrics; per-row
values are thread-leader aggregates with the semantics registered in
[`docs/TELEMETRY_MANIFEST.md`](../../docs/TELEMETRY_MANIFEST.md).
Startup inventory includes Task Scheduler logon/boot tasks (read-only COM)
and folder-item control rides the same `StartupApproved\StartupFolder`
byte Task Manager flips. The environment insight reads another same-user
process's variables and working directory from its PEB through the
boundary, and the adapter supplies local-time rules by synthesizing a TZif
payload for the core parser from the native per-year zone rules.
System and process registrations cross into runtime as named binding
transactions, preserving capability identity without positional wiring.

## Module map

```text
src/provider/
├── process/                   list, insights, control, uac (ADR-035 transport)
├── system/                    cpu_freq, cpu_info, disk, gpu, hardware_inventory,
│                              msr_readout, network, auxiliary
├── service/ (+ log_runtime)  storage/  environment/ (+ sessions, boot_evidence)
├── power/  sensor/  integration/
src/bindings.rs                taskmanager-windows-api consumption wrappers
src/command.rs  config.rs  instance.rs  local_time.rs
```

No PowerShell/CMD anywhere; native calls stay inside the ADR-031 boundary.
