# Retention budgets

CPU, memory, storage and network host aggregates retain at most 36,001
points, covering an hour at the minimum 100 ms cadence. Receipts, per-core,
device and secondary rings stay at 600 points. `HistoryRetention` declares
both physical budgets; live graphs clone only their requested tail, and
preferences never replace authority or grow physical storage.

The three correlated per-core rings share core's hard outer-cardinality
limit, so one oversized provider vector cannot multiply retained histories.

Dynamic battery/sensor histories retain old identities across partial
discovery, append explicit gaps when a retained identity is not sampled, and
share a 256-identity domain ceiling; new identities above that ceiling are
rejected with an observable ingestion report until authoritative discovery
retires old entries.

Persistence callbacks execute inside the owning domain commit so persisted
order cannot diverge from live rings and receipts. Custom sinks must not
synchronously read that same telemetry domain; the production store only
appends to its own bounded pending state and satisfies this non-reentrant
contract.
