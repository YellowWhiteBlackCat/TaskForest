# Collection semantics

## Local time

Local-time discovery is a native composition fact: `TZ=""` produces explicit
fixed UTC, safe relative names resolve below the zoneinfo root, and absolute
paths remain opt-in `TZ` inputs. Non-regular and oversize sources are
rejected before parsing. Missing, denied, rejected and malformed rules remain
typed; none falls back to UTC. A regular network file can still delay
startup.

## Services and power

Systemd dependency parsing validates provider-native targets and writes only
typed `ServiceRelationGraph` edges; legacy JSON text projections remain a
core wire-boundary concern and are never a second Linux parser output.
Service inventory parsers construct `ServiceItem` through the typed inventory
boundary; they never assign historical relation strings.

Power-supply collection follows the same rule: sysfs scalars are assembled as
one `BatteryScalarObservations` group and applied once, while lifecycle
retention and schema-v1 projections remain typed-core responsibilities.

## CPU, memory, and GPU

CPU and memory collection also assemble only typed observation groups before
a single domain constructor. Refresh failure retains CPU groups and both
memory groups atomically as stale; Linux never writes schema-v1 scalar,
per-core, composition, module, commit, compression, or rate mirrors.

GPU DRM/AMD/Intel/NVML providers likewise contribute only typed scalar and
throttle observations. The optional graphics-API provider runs fixed-argv,
bounded `glxinfo -B` / `vulkaninfo --summary` probes once and binds their
version tokens only when exactly one DRM GPU is visible. The registry
preserves per-field precedence/provenance, retains failed values only as
stale within one device generation, and applies the merged groups once;
legacy GPU keys remain a core wire concern.

For the static CPU Base field, cpufreq policy `base_frequency` is
authoritative and the highest visible policy is selected for heterogeneous
CPUs; CPUID 0x16 is only a static fallback. Live `scaling_cur_freq` samples
remain telemetry.

## Process inventory

`engine/process/enumerate.rs` is the only Linux process enumerator: one `/proc`
directory pass yields both the availability probe and the sorted pid list, and
one `stat` read per pid yields identity (name, state, parent) and scalars from
the same payload, so a row can never carry one process's name and another's
counters. Enumeration never delegates to `sysinfo`'s process table. That table
holds one open `/proc/<pid>/stat` descriptor per observed process for the
lifetime of the handle and raises the process's soft `RLIMIT_NOFILE` to the hard
limit when it initializes that budget, which would make an unprivileged 7×24
monitor's descriptor table track the host's process population and would let a
dependency take a process-global action the product never observes. Every read
here is open-use-close, so retained descriptors are a fixed cost, and argv is
re-read each tick rather than cached against the identity.

A pid that cannot be read contributes a typed failure against the `stat`
source and publishes no row; an empty name plus zeroed scalars would be a
fabricated identity. Rows are published in ascending pid order. The bound is
gated twice: `tests/soak.rs` Arm D asserts an absolute per-tick ceiling on
cross-process `/proc` handles plus an unchanged `Max open files` soft limit,
and `enumeration_holds_no_descriptor_per_observed_process` asserts descriptor
count equality across a full sweep.

### Sampled cadence

Two per-process sources are too expensive to read on every tick, so both are
decimated and both bridge the gap with a typed `Stale` value rather than an
absence: `/proc/<pid>/fd` (a directory walk, `FD_COUNT_REFRESH_EVERY_N_TICKS`)
and `/proc/<pid>/smaps_rollup` (`SMAPS_REFRESH_EVERY_N_TICKS`). The rollup makes
the kernel walk every page table of the target; measured over 400 live
processes it cost 1.23 ms each against 36 µs for `status`, 31 µs for `stat` and
16 µs for `cmdline`, so reading it every tick spent roughly 83% of the whole
process refresh on the USS and huge-page detail counters alone. RSS components,
hybrid PSS and VmSwap come from `status` and stay fresh every tick.

A deferred tick is not a measurement failure: the aggregate source stays
`Available` while a prior value is retained and `Empty` when none exists, so the
source cannot flap 1-Available : 4-Empty across the cadence. Core's
`memory_uss_bytes` / `memory_anon_huge_pages_bytes` and every figure derived
from them (shared, proportional shared, USS ratio, huge-page ratio, breakdown,
USS sort key) read that retained value, because it is the number the row
displays; `current_memory_bytes`, `current_memory_pss_bytes` and
`current_swap_bytes` stay fresh-only. A pid seen for the first time is sampled
immediately, so no row ever shows a fabricated zero.
