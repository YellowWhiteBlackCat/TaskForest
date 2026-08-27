# ADR 010: Storage topology, SMART, and lifecycle boundaries

## Status

Accepted for the shared model and Linux reference adapter. Target-device
receipts remain required before hardware-family coverage is called complete.

## Context

A single `StorageTransport` value cannot accurately describe modern storage.
For example, a USB enclosure may tunnel ATA, SCSI/UAS, or NVMe commands; SAS
devices normally expose the SCSI command set; device-mapper and software RAID
are presentation layers rather than transports. Routing SMART providers from
that collapsed value loses information and encourages device-name or vendor
allowlists.

The previous shared self-test plan also contained an executable name and argv.
That made a Linux `smartctl` implementation detail part of the portable core
API. Self-test polling crossed the provider boundary with only a native locator,
so an adapter did not receive the stable identity and physical-device
generation needed to revalidate a target after hot-plug.

Finally, Linux `removable` describes removable media. It is not a universal
hot-plug capability bit, and a missing bit must not become a believable false.

## Decision

### Orthogonal storage facts

`taskmanager-core::storage` owns four portable axes:

- `StorageProtocol`: NVMe, ATA, SCSI, MMC, SD, UFS, other, or unknown;
- `StorageInterconnect`: PCIe, SATA, SAS, USB, MMC, SD, UFS, IDE, Virtio,
  Fibre Channel, iSCSI, network, PCIe tunnel, FireWire, platform stack, other,
  or unknown;
- `StorageDeviceKind`: physical, virtual, aggregate, or unknown;
- `StorageIdentityStability`: persistent, attachment-scoped, or unknown.

`StorageConnection` combines protocol, outer interconnect, and presentation
kind. A USB SAT device is ATA over USB; UAS is SCSI over USB; SAS is SCSI over
SAS; device-mapper is virtual over the platform stack. Unsupported or missing
evidence remains unknown on only the affected axis.

`StorageTransport` remains as a legacy snapshot/read-model projection. Existing
snake-case values are preserved and UFS is added. Pre-migration snapshots can
derive an effective `StorageConnection`; new providers route on the orthogonal
axes.

`DiskMetrics::media_removable` and `hotplug_capable` are independently optional.
The legacy `removable` boolean remains wire-compatible but does not authorize a
new provider to collapse unknown into false.

### Ownership and provider routing

- core owns only storage facts, health facts, lifecycle identity/generation,
  and typed SMART outcomes;
- application owns independent SMART observation/control ports, user intent, and
  the revision-gated complete-batch projection consumed by every frontend;
- the provider SPI receives a generation-bound `StorageDeviceTarget`;
- the shared runtime owns target-keyed, generation-checked job publication, not
  native command construction;
- each native adapter owns discovery evidence, locator validation, command
  strategy, parsers, external tools, paths, and OS error mapping.

The Linux adapter maps sysfs evidence to `StorageConnection`. Its SMART registry
selects protocol-family providers at runtime. USB bridge strategy can use the
tunneled protocol when known and uses bounded generic probing only when it is
unknown. USB-NVMe probing includes smartmontools' generic SNT translation
handlers (`sntasmedia`, `sntjmicron`, and `sntrealtek`) as runtime mechanisms,
not product/model allowlists. Only typed device-type mismatch advances to the
next mechanism; exhausting bridge mechanisms reports `BridgeLimitation`.
MMC/UFS inventory remains supported even when ATA/SCSI/NVMe SMART is not the
appropriate health protocol.

The Linux `SmartSelfTestPlan` is local to the Linux engine. `smartctl`, `/dev`
paths, command arguments, JSON field names, and errno text are not shared API.

### Lifecycle and control safety

The block inventory is the discovery authority. Mounts, counters, and health
providers are enrichments and cannot confirm absence. Complete discovery may
advance present/absent/reappeared generations; partial or unavailable
discovery may only degrade retained state.

WWID/serial-backed identities are marked persistent. Native-locator fallbacks
are attachment-scoped and must not be presented as reorder-safe. Metadata
failure may temporarily reuse a stronger cached identity for the same
continuous attachment, including its stability classification.

A SMART self-test is a job, not a device lifecycle entry. Runtime ownership is
a map keyed by `(DeviceId, DeviceGeneration)`, so multiple drives can be
tracked concurrently and a replacement generation cannot inherit an older
drive's job. Its intent and observation keep `DeviceId`, `DeviceGeneration`,
and the opaque native locator. Polling providers receive all three together so
a native adapter can reject a locator that no longer resolves to the selected
physical generation. A separate opaque, checked-increment job token prevents
an older poll from overwriting a restarted job; overflow fails before mutation
instead of saturating and reusing a token.

Observation publication is one authoritative `SmartObservationBatch`, never a
single-job optional snapshot. A revision is advanced under the same lock as
every real install, commit, remove, or prune mutation, and is snapshotted
atomically with the job map. The application-owned
`SmartObservationProjection` accepts only a newer revision and atomically
rejects duplicate targets. Consequently a control-first stop can publish an
empty newer batch and a late pre-stop poll cannot resurrect the job in GPUI,
TUI, or another frontend.

## Compatibility

- legacy `transport`, `removable`, and flat SMART intent JSON keys remain
  readable;
- `StorageConnection::legacy_transport` preserves old read-model behavior;
- missing `connection`, identity stability, removable-media availability, and
  hot-plug capability decode as typed unknown/default values;
- the old core `SmartSelfTestPlan` is intentionally removed because it exposed
  an implementation mechanism rather than persisted domain data.

## Consequences and open evidence

Adding a storage family requires protocol/interconnect mapping and runtime
provider registration, never a vendor build. ATA/SATA, SAS/SCSI, NVMe, USB
SAT/UAS/NVMe bridges, MMC/SD, UFS, Virtio, and logical/aggregate storage can all
remain in one OS artifact.

Fixtures prove classification, fallback compatibility, field-level absence,
provider routing (including USB-NVMe SNT exhaustion), lifecycle cache behavior,
fail-closed command targeting, multi-drive coexistence, identity-generation
replacement, timeout retention/expiry, revision overflow, and late-poll
rejection. Linux publishes mutation authority only
after authoritative block discovery, identity metadata, and lifecycle
generation assembly. Every native SMART mechanism attempt re-resolves the
complete target and immediately re-reads sysfs to compare the persistent
physical identity; partial metadata, ambiguous mappings, attachment-scoped
identity, logical/aggregate/virtual presentation, generation/locator drift,
and live replacement all reject the operation.

Production claims still require sanitized target receipts for ATA, SAS, USB
bridges, MMC/SD, UFS, complex dm/md/LVM/multipath topology, permission
denial/recovery, and physical remove/re-add/reorder. The final command-boundary
revalidation is implemented, but a fixture is not a target-hardware receipt.
Drive-side self-test abort, restart-time adoption of already-running tests,
parallel multi-drive provider I/O, and second-OS SMART adapters remain explicit
future capabilities rather than being inferred from local tracking support.
