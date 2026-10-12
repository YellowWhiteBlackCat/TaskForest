# Device row identities

`taskmanager-shell::app::device_rows` owns renderer-neutral device row identities,
anchors, and family revisions. It imports facts from their core owner modules and
uses the application device lifecycle projection as the identity authority.

`DeviceRowId` qualifies identity by family: storage disk and partition, network,
GPU, power supply, sensor channel, or NPU. Physical rows include the core-owned
`DeviceId` and `DeviceGeneration`. A storage partition also includes its parent
disk and partition ID; the parent ID and generation must match the enclosing
disk fact. A sensor channel includes its physical device and local channel ID.
Labels and positions never substitute for missing identity.

An identity is selectable only when its non-empty owner ID, family partition,
and generation match the accepted lifecycle projection. Duplicate rows,
cross-partition ownership conflicts, missing child IDs, and invalid generations
fail closed. A visible metric without an admitted identity may still be rendered
as a fact, but it cannot produce a stable row anchor.

NPU inventory uses the application lifecycle registry. The first successful
inventory assigns the initial generation. Only a successful complete inventory
can confirm an omitted NPU absent; a later return receives a higher generation.
Provider failure, empty or duplicate device IDs, and other incomplete inventories
do not confirm absence. Expired lifecycle rows keep an exact-ID generation
tombstone within the core registry's bounded retired-identity capacity. Under
enough identity churn that also retires an ID's generation tombstone, a later
observation starts a new bounded authority; family data revisions still reject
anchors from the prior projection.

`DeviceRowAnchor` binds an identity and index to that family's data and order
revisions. `reconcile_device_row_identity` finds a still-present identity after
reordering, while `resolve_device_row_anchor` rejects stale geometry. Data
revisions are family-local; order revisions change when the ordered identity
sequence changes. An unrelated family update does not invalidate these values.

This module defines the shared owner seam only. Renderer selection and row
presentation wiring is tracked in the frontend phase.
