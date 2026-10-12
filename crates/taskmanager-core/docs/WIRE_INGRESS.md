# Wire ingress and per-domain DTO rules

Private wire DTOs (`src/core/*/wire.rs`) are the only place a published
external payload is parsed; hydration immediately produces current typed
observations, and serialization projects legacy keys only from currently
Available truth. Failed facts never serialize as zero, empty or false success.

## Services

`ServiceDeps` and `ServiceItem` each own one private typed
`ServiceRelationGraph`; consumers receive only read-only relation accessors.
Their private serde ingress/serializer handles four published dependency-string
fields; the domain only stores the typed relation graph. Input is
canonicalized immediately, and unknown typed relation kinds round-trip
unchanged.

## Battery

`BatteryInfo` owns a private `BatteryScalarObservations` group exposed through
typed read accessors and one apply operation. Its four schema-v1 scalar options
live only in `BatteryInfoWire`: trusted legacy rows hydrate Unknown
observations, while only currently Available typed values project back to
those keys.

## CPU and memory

`CpuMetrics` owns one private `CpuScalarObservations` group; its typed
per-core groups are the only per-core authority. `MemoryMetrics` likewise owns
private scalar and optional observation groups that are replaced and retained
atomically. Their private wire DTOs hydrate legacy values only into Unknown
truth behind CPU identity/topology or a positive memory denominator, and emit
legacy success keys only for currently Available observations. Confirmed-empty
CPU groups remain distinct from failed groups, and memory percentage accessors
return `Option` instead of collapsing an unknown denominator to zero.

## Disk and network

Disk/partition and network schema-v1 numbers, `transport`/`removable`,
`is_wireless`, SSID and signal live only in private wire DTOs. Domain rows
expose read-only typed observation, connection, attachment-capability and
adapter-class accessors. Disk rows also carry per-kind SMART self-test
capabilities; older payloads hydrate them as `Unknown`, never from SMART health
availability. Providers use named typed apply operations.

## GPU

`GpuMetrics` owns private `GpuScalarObservations` plus an independent typed
throttle observation; engine failure and field provenance remain separate
facts. Its private wire DTO imports schema-v1 values only for an identified
device and the historical field sentinel, and emits legacy keys only from
currently Available truth. Failed GPU reads therefore cannot serialize as
idle, cold, empty-memory or unthrottled success.

## Process resources and identity

`ProcessResourceSnapshot` stores only private state, sources and canonical
`ProcessResourceObservations`. Its private wire DTO imports non-empty
schema-v1 mirrors only when typed truth is `Unknown` and a success time
exists; typed conflicts win. Serialization projects legacy success fields only
from typed `Current`, while confirmed empty, partial, stale and unavailable
remain distinguishable in the typed observation.

`ProcessItem` owns private metadata and application-identity observations.
Its private wire DTO imports legacy owner/path only for a trustworthy nonzero
process identity when typed truth is `Unknown`; current accessors are the only
consumer surface, and serialization projects legacy owner/path from current
typed truth.

## Saved views

External saved-view input is read-only at the private ingress: recognized old
tokens are canonicalized into `ProcessViewPresetConfig`. The canonical write
DTO has no obsolete mode field, so saving a parsed configuration cannot
republish the old vocabulary.
