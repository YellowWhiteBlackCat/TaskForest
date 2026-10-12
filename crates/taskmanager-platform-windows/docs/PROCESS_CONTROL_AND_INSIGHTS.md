# Process control and insights

## Suspend/resume and limits

Process control maps Suspend/Resume onto the documented per-thread path
(ToolHelp32 snapshot + `OpenThread(THREAD_SUSPEND_RESUME)` +
`SuspendThread`/`ResumeThread`); the undocumented `NtSuspendProcess` is
deliberately not used and the two mechanisms are never mixed. Thread rows
carry descriptions and CPU time via `GetThreadDescription`/`GetThreadTimes`
with identity-bound rate baselines. Resource limits ride boundary-owned
nested jobs (memory / process count / whole-number CPU percent); they only
tighten, never survive app exit (session-scoped — presented as such), and a
fractional CPU quota is rejected rather than rounded.

Use exact process identity, bounded enumeration, checked OS lengths and typed
SCM/session/locale/topology outcomes. Every target-scoped process read
validates the creation token before and after its native query; reveal
validates before desktop side effects.

## Insights

The open-files insight walks the system handle table through the boundary
(same-user processes; other users stay typed `PermissionDenied`) with the
sacrificial-thread name resolution the named-pipe deadlock forces. Memory
telemetry reports the compression store size from the kernel process
snapshot (absent store = honest absence, never zero). Hardware inventory
includes monitors via EnumDisplayDevices + registry-cached EDID parsed by the
shared portable parser, and NPU inventory via the SetupAPI compute
accelerator class with utilization deliberately typed-unavailable until a
counter-set receipt exists. The environment insight reads another same-user
process's variables and working directory from its PEB through the boundary.

## WSL, startup, and local time

The container rollup is the WSL view: one row per registered distribution
(LXss inventory), with running distributions sampled through the fixed
`wsl.exe` program channel (`--list --running`, `--exec ls/cat` on `/proc` —
never a shell). Stopped distributions are never sampled (executing against
them cold-boots the utility VM) and keep typed-unavailable metrics; per-row
values are thread-leader aggregates with the semantics registered in
[`TELEMETRY_MANIFEST`](../../../docs/TELEMETRY_MANIFEST.md).

Startup inventory includes Task Scheduler logon/boot tasks (read-only COM)
and folder-item control rides the same `StartupApproved\StartupFolder` byte
Task Manager flips. The adapter supplies local-time rules by synthesizing a
TZif payload for the core parser from the native per-year zone rules.
