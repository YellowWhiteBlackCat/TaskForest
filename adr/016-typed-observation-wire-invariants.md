# ADR-016: Typed observation wire payloads fail closed

Status: accepted

## Context

The shared telemetry model keeps value or semantic state, freshness/failure,
and last-success time together. Constructors such as `available`, `absent`,
`partial`, and `unavailable` create coherent combinations, but the public
snapshot fields are retained for wire compatibility. A derived `Deserialize`
implementation previously allowed contradictory JSON to bypass those
constructors, including:

- `Available` or `Partial` without a value/state or success time;
- `Stale` without evidence of a prior successful observation;
- `Unavailable` carrying a believable value, semantic state, or success time;
- an available group containing failed slots, or an unavailable group carrying
  current or stale item history.

Accessors hid some contradictions, but accepting them left different consumers
free to interpret the same payload differently. Silently converting malformed
input to a generic unavailable state would also discard the original failure
and make corruption indistinguishable from a real provider result.

`Unknown` is different. It is the explicit compatibility state for snapshots
written before typed observation truth existed. Existing DTO-specific
compatibility policies may inspect adjacent legacy projections, but typed
current accessors never treat `Unknown` itself as current.

## Decision

Typed observation wire payloads are validated during deserialization and
contradictory payloads are rejected. They are not silently normalized.
Serialization shape, public fields, and normal constructors remain unchanged.

The shared `ObservationWireError` vocabulary reports stable invariant classes.
Private wire structs deserialize first and convert through `TryFrom`, so nested
observations are validated before their owning group or DTO.

The invariants are:

| Contract | Current (`Available` / `Partial`) | `Stale` | `Unavailable` |
| --- | --- | --- | --- |
| `ScalarObservation<T>` | value and success time required | value and success time required | value and success time forbidden |
| `OptionalObservation<T>` | `Present`, `Absent`, or `NotApplicable` plus success time required | a non-`Unknown` retained state and success time required | only `Unknown`, with no success time |
| `ScalarObservationGroup<T>` | success time required; a fully `Available` group contains only `Available` slots, while `Partial` may contain failed slots | success time required; only stale or unavailable slots are permitted | success time and trustworthy current/stale item history forbidden; per-slot unavailable facts remain allowed |
| `ProcessMetadataObservation<T>` | value and success time required; confirmed `Absent` requires no value and a success time | success time required | value and success time forbidden |

Process metadata predates the reusable optional-state axis. Its stale form may
represent either a retained value or a previously confirmed absence, so a
success timestamp is the available proof of history when the retained value is
`None`.

`Unknown` remains deserializable for schema compatibility. It never becomes
typed current truth, and each owning DTO remains responsible for deciding
whether its separate legacy fields have enough identity and success evidence
to be consulted. Real measured zero, `false`, confirmed empty groups,
confirmed absence, and not-applicable states remain valid because their current
state and success time are explicit.

## Consequences

Malformed imported snapshots and diagnostic payloads fail at their boundary
instead of creating believable telemetry. Nested CPU groups, process metadata,
network optional facts, and every scalar-using hardware DTO share the same
policy without provider- or OS-specific code.

Callers that manually construct public fields can still create an invalid
in-memory value for compatibility, but serializing and then deserializing that
value is intentionally rejected. New code should use constructors. A future
breaking schema may make fields private; this decision does not require that
large migration.

Tests cover every rejected invariant class plus valid schema-compatibility
`Unknown`, measured zero, confirmed empty/absent/not-applicable, partial groups,
per-slot unavailable facts, and stale confirmed absence.
