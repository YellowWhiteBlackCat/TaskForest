# taskmanager-platform-linux

## Role

Linux application-port adapters and bounded collectors for `/proc`, `/sys`,
systemd/OpenRC, SMART, hwmon, DRM, cgroup and Linux controls.

## Boundary

The crate translates Linux facts into platform contracts; it never exposes
Linux paths, raw handles or provider choice to core/application/frontends.
Privilege flows through escalation helpers and stable identity checks.

Display, compositor, package and GPU enrichments are capability-optional. The
DRM/EDID path supplies static hardware inventory without a graphical session;
Wayland current mode/HDR/VRR state is a separate runtime capability and is not
merged into `HardwareInfo`. Package versions use bounded local metadata or a
fixed-argument native query for the detected distro; a missing database or tool
yields `None`. PCI marketing names are best-effort identity enrichment, never a
vendor-SKU inference.

For the static CPU Base field, cpufreq policy `base_frequency` is authoritative
and the highest visible policy is selected for heterogeneous CPUs; CPUID 0x16
is only a static fallback. Live `scaling_cur_freq` samples remain telemetry.

This Linux crate does not define the macOS or Windows implementation of these
facts. Those adapters must fill the same neutral contracts from their native
APIs, or return typed absence; Linux receipts must not be treated as
cross-platform evidence.

## Key modules

- `src/backend/` groups application-port implementations by capability family.
- `src/engine/` owns bounded `/proc`, `/sys`, command, hardware and control engines.
- `src/provider/` registers the Linux implementations and translates them into the platform SPI.

Provider composition converts each registration to a named runtime binding
transaction before assembly; required capabilities cannot be reordered by a
positional constructor. Wireless link backfill likewise consumes one named
counter/source context.

The on-demand Intel PMU provider reuses `PolkitGate` as its only readiness
authority. Composition publishes the exact initial capability status; each
request rechecks the same `pkexec + policy + executable helper` triple and
fails before prompt launch when installation is incomplete. Provider failures
cross as `Err(ProviderFailure)`, never as an `Ok` snapshot containing failure.

## Contract and verification

Missing files, permission, malformed input, timeout, PID/device replacement and
recovery are typed. Keep I/O bounded and off UI threads; verify parser fixtures,
provider behavior, live smoke and target hardware receipts separately.
Local-time discovery is a native composition fact: `TZ=""` produces explicit
fixed UTC, safe relative names resolve below the zoneinfo root, and absolute
paths remain opt-in `TZ` inputs. Non-regular and oversize sources are rejected
before parsing. Missing, denied, rejected and malformed rules remain typed;
none falls back to UTC. A regular network file can still delay startup.
Systemd dependency parsing validates provider-native targets and writes only
typed `ServiceRelationGraph` edges; legacy JSON text projections remain a core
wire-boundary concern and are never a second Linux parser output. Service
inventory parsers construct `ServiceItem` through the typed inventory boundary;
they never assign historical relation strings.
Power-supply collection follows the same rule: sysfs scalars are assembled as
one `BatteryScalarObservations` group and applied once, while lifecycle
retention and schema-v1 projections remain typed-core responsibilities.
CPU and memory collection also assemble only typed observation groups before a
single domain constructor. Refresh failure retains CPU groups and both memory
groups atomically as stale; Linux never writes schema-v1 scalar, per-core,
composition, module, commit, compression, or rate mirrors.
GPU DRM/AMD/Intel/NVML providers likewise contribute only typed scalar and
throttle observations. The optional graphics-API provider runs fixed-argv,
bounded `glxinfo -B` / `vulkaninfo --summary` probes once and binds their
version tokens only when exactly one DRM GPU is visible. The registry preserves
per-field precedence/provenance, retains failed values only as stale within one
device generation, and applies the merged groups once; legacy GPU keys remain a
core wire concern.
