# taskmanager-windows-api

## Role

Minimal audited Windows API boundary for performance, locale, Known Folder,
exact-process (terminate/priority/affinity/per-thread suspend-resume/job
limits), WTS sessions with per-logon incarnation revalidation and current-session
lock, SCM, topology/cache, NIC metadata, IP Helper
connection tables, ToolHelp32 thread details, DXGI/D3DKMT/PDH GPU surfaces
(adapter and per-process memory, per-engine and per-instance utilization,
per-core CPU frequency), ACPI thermal zones, Windows Event Log
(`EvtQuery`/`EvtSubscribe`), power scheme/overlay queries, SMBIOS, the
handle-table open-files lane (sacrificial-thread name resolution), the
kernel-snapshot memory-compression size, SetupAPI compute-accelerator (NPU)
inventory, EnumDisplayDevices + registry EDID reads, and the LXss
registry WSL distribution inventory (ADR-031; gap ledger
in ADR-018), plus the ADR-035 UAC `runas` call group
(`ShellExecuteExW("runas")` + `SEE_MASK_NOCLOSEPROCESS`, bounded
`WaitForSingleObject`, `GetExitCodeProcess`, `CloseHandle`, and the
interactive-session check that gates it).

## Boundary

All `unsafe`, handles, buffers, encoding, OS length validation, allocation caps
and ownership stay here. Public APIs return typed values/errors only; provider
crates do not receive raw Windows handles or pointers. Every function has a
non-Windows arm returning the typed `Unsupported` error so adapters and contract
tests build on every host.

## Contract and verification

Prefer safe crates first. Every query is bounded, checks reported lengths and
maps unsupported/permission/not-found to typed outcomes. Run Windows target
contract, boundary firewall and native verification; never add PowerShell/CMD
telemetry as a shortcut.

WTS inventory uses `WTSSessionInfoEx` Level 1 `LogonTime` as a raw per-logon
comparison token. Missing or unreadable values leave the row displayable but
without a control identity; this information class is unsupported on Windows
Vista and Windows Server 2008. The timestamp is not a uniqueness guarantee.
Each logoff re-queries that token immediately
before `WTSLogoffSession`; WTS accepts only a numeric session ID, so this
check/action pair is not atomic and does not promise to remove the narrow reuse
race. `LockWorkStation` has no target parameter and is only called when the
requested identity matches the current process's WTS session; success means the
asynchronous lock request started, not that the session is already locked.

## Module map

```text
src/pdh/                         performance counters (counters.rs, cpu.rs, gpu.rs)
src/process/                     exact-process control (+ insights/)
src/process_tree.rs              ToolHelp32 process tree snapshot
src/process_network.rs           IP Helper per-process network tables
src/gpu.rs  disk.rs  network.rs  power.rs  thermal.rs  display.rs
src/memory_info.rs               kernel-snapshot memory/compression size
src/event_log.rs                 Windows Event Log (EvtQuery/EvtSubscribe)
src/smbios.rs                    SMBIOS table reading
src/task_scheduler.rs            startup-item inventory
src/job_control.rs               job-object process limits
src/sessions.rs                  WTS logon identity, exact-target logoff, current-session lock
src/topology.rs                  CPU topology and cache
src/npu.rs                       SetupAPI compute-accelerator (NPU) inventory
src/open_files.rs                handle-table open-files lane
src/icons.rs                     process icon extraction
src/known_folders.rs  time_zones.rs  single_instance.rs  msg_pump.rs
src/runas.rs                     UAC ShellExecuteExW(\"runas\") (ADR-035)
src/wsl.rs                       LXss registry WSL distribution inventory
```

Every unsafe block carries a SAFETY proof; public APIs return typed values only (ADR-031).

## Docs

- [`docs/AUDIT_RULES.md`](docs/AUDIT_RULES.md) — unsafe containment, query discipline and
  the verification commands.
- [ADR-031](../../adr/031-windows-native-safe-boundary.md) and
  [ADR-018](../../adr/018-windows-telemetry-safety.md) — Windows boundary and gap ledger.
