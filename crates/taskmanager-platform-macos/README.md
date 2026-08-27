# taskmanager-platform-macos

## Role

macOS application-port adapter composition using safe system APIs, mature
crates and bounded compatibility commands.

## Boundary

OS paths and command parsing stay here. Unsupported GPU, container, network or
control features remain typed; Linux assumptions never enter shared models.

## Contract and verification

Register the same platform SPI as other adapters and preserve partial/provider
failure semantics. Shared command lifecycle stays in `taskmanager-platform-portable`;
this crate only chooses fixed argv and parses macOS output. The current safe process source exposes only a
second-resolution compatibility start time, so target-scoped resources,
destructive control, and process resource reveal fail closed as `Unsupported`
until a precise same-handle identity boundary exists. Linux CI proves
compile/contract coverage; live claims require a macOS run of
`../../scripts/quality/native-platform-fact-safety.sh` and its `.tmp/` receipt.
Launchd inventory constructs typed `ServiceItem` rows and leaves the canonical
relation graph empty when the bounded provider has no relationship facts; it
never writes compatibility relation strings.
CPU and memory providers build canonical scalar/per-core and optional groups,
then construct each domain snapshot once. Compatibility JSON projection stays
inside core; unsupported memory enrichments remain typed unavailable.
GPU inventory constructs typed dedicated/aggregate capacity observations once;
unsupported live scalars and throttle capability remain explicit typed absence.
The adapter never writes schema-v1 GPU mirrors.
System and process registrations cross into runtime as named binding
transactions; macOS composition owns provider selection but not positional
runtime wiring.
