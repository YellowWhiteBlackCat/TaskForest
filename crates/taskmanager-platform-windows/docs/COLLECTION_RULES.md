# Collection rules

## GPU and PDH

GPU telemetry joins DXGI/PDH only by exact adapter LUID, and unavailable
samples never borrow a sibling or become zero. DXGI and NVML enumeration
ceilings publish typed partial status instead of silently truncating a
healthy inventory.

NVML and DXGI assemble canonical GPU observations independently, then merge
by exact PCI/LUID identity with existing per-field precedence.
Engine-utilization rows (`telemetry.gpu.engines`) are served from PDH
per-engine data without elevation — the Linux lane's privileged PMU helper
has no Windows counterpart need. Per-process GPU utilization and memory
aggregate the PDH `pid_<pid>` engine and `GPU Process Memory` instances, with
NVML kept only as an NVIDIA supplement; WDDM's designed NVML unavailability
is never rendered as a believable zero. Throttle masks have explicit
availability (including confirmed empty and future-bit `Other`), and
providers never write schema-v1 GPU mirrors.

## CPU, memory, and services

CPU usage/frequency/thermal slots and memory scalar/optional families are
assembled as typed groups before one snapshot construction. Partial per-core
frequency coverage remains `Partial`, unsupported optional memory facts
remain unavailable, and provider code never writes compatibility mirrors. The
CPU energy-preference slot carries the effective power-overlay label (mapped
from the documented overlay GUIDs; unknown GUIDs stay absent).

SCM inventory dependencies are assembled once as typed
`ServiceRelationGraph` edges; legacy relation strings are projected only by
core serialization.

Service log snapshots and incremental streams come from the boundary's
winevt (`EvtQuery`/`EvtSubscribe`) surface against user-readable channels;
access denials and unreadable channels stay typed, and a silent service is an
honest empty snapshot. Boot evidence reads the Diagnostics-Performance boot
event through the same surface, and the session Lock action is the documented
`LockWorkStation` call on the calling session.

Process resources publish sysinfo memory as a typed current observation and
job-object/limit families as typed unsupported facts; they never write
schema-v1 resource mirrors directly.
