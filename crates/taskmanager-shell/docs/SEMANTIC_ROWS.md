# Shared semantic process rows

`append_process_semantic_rows` adapts the exact ordered `ProcessTreeRow` slice
used by a frontend's process renderer into `taskmanager-ui-contract` inputs. It
preserves row order, category/application/process identity, selection, expansion
state, and available actions. The caller supplies localization, selection lookup,
and a visible-index resolver over the same process snapshot that produced the
row slice.

Semantic node identity and expansion state use separate values. Valid
`ProcessRowId` values produce stable semantic IDs. Rows without a live process
identity remain visible with IDs scoped to the projection generation and row
position; those rows expose no selection, focus, expand, collapse, or control
actions. Category expansion uses its own category key.

Process rows carry their core-owned CPU and memory `ScalarObservation` values.
Category and application rows carry all nine core-owned `AggregateMetric`
values from `ProcessRowAggregate`. Availability, failure reasons, current and
last-known values, observation times, and aggregate member coverage stay typed
through the handoff. The adapter does not reduce aggregates to a current scalar;
localized text and visual formatting remain frontend responsibilities.

The adapter lives in shell and depends on the toolkit-neutral UI contract. The
UI contract does not depend on shell. Frontends can connect their semantic
builders to this shared input during their UI integration work.
