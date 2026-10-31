# Capability enrichments

Display, compositor, package and GPU enrichments are capability-optional. The
DRM/EDID path supplies static hardware inventory without a graphical session;
Wayland current mode/HDR/VRR state is a separate runtime capability and is
not merged into `HardwareInfo`. Package versions use bounded local metadata
or a fixed-argument native query for the detected distro; a missing database
or tool yields `None`. PCI marketing names are best-effort identity
enrichment, never a vendor-SKU inference.

The current-window PNG adapter is also capability-optional. On KDE Wayland it
uses the installed Spectacle active-window provider through fixed argv,
validates the returned PNG header and dimensions, and reports a typed
provider absence or fault when Spectacle is unavailable. The cross-target
backend roster and future-slot vocabulary are owned by
[`taskforest-contract`](../../taskforest-contract/docs/SURFACE_AND_DISCOVERY.md).

This Linux crate does not define the macOS or Windows implementation of these
facts. Those adapters must fill the same neutral contracts from their native
APIs, or return typed absence; Linux receipts must not be treated as
cross-platform evidence.

## Registration and escalation

Wireless link backfill consumes one named counter/source context. The on-demand
Intel PMU provider reuses `PolkitGate` as its only readiness
authority. Composition publishes the exact initial capability status; each
request rechecks the same `pkexec + policy + executable helper` triple and
fails before prompt launch when installation is incomplete. Provider failures
cross as `Err(ProviderFailure)`, never as an `Ok` snapshot containing failure.
