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
