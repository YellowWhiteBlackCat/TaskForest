# Process rows in semantic snapshots

The accessibility model keeps process measurements typed alongside the row's
localized text. `ProcessRowObservations` carries core-owned CPU and memory
`ScalarObservation` values. `ProcessAggregateSemantics` carries all nine
core-owned `AggregateMetric` values for category and application rows, including
availability, failure reason, retained values, observation time, and member
coverage.

`ProcessRowBehavior` distinguishes selectable rows, display-only rows without a
trusted live identity, and expandable process tree items. A `SemanticNodeId` is
not an expansion key: tree rows store the renderer's expansion key separately,
and snapshot validation only accepts expansion keys on tree-item nodes.
Display-only process rows stay visible while exposing no focus, selection, or
expand/collapse action.

The contract imports measurement types from `taskmanager-core` and has no shell
dependency. `taskmanager-shell::append_process_semantic_rows` maps its canonical
ordered process tree rows into these inputs; frontend builders consume the
resulting semantic snapshot without recreating process identity or aggregate
availability policy.
