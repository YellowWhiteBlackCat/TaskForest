# Budgets and event delivery

## Runtime budgets

`RuntimeBudgets` is validated by the only production constructor,
`ChannelRuntime::try_new`, before channels or ECS entities are allocated. The
default route ceiling is 64: one power-of-two step above the current 45 typed
routes and equal to the worker-lane ceiling. Per-capability targets remain 64;
the explicit global/domain ceilings are 256/128, so at most four full target
partitions exist globally and two in one domain. Since every scope is already
capped at 4 KiB by the contract, retained target identity is capped at 1 MiB.
Zero, crossed, undersized-delivery, and route-overflow budgets fail as typed
construction errors.

## Delivery permits and the terminal mailbox

Every admitted lifecycle also reserves one class-attributed delivery permit. A
terminal must first claim that exact `(CapabilityId, RequestId)` permit before
it can retire lifecycle state; wrong, missing, and duplicate claims change
nothing. Completion retires the ECS owner but keeps that permit until the
application drains its terminal event. Primary event queues remain bounded by
`QueueCapacities`; if one is full, the terminal moves to a mutex-backed mailbox
bounded by the 352 default permits (64 capability owners + 256 target owners +
32 control-reserved slots). Observation admission cannot consume that control
reserve. No pump thread or provider blocks on frontend progress.

Queue depth/high-water and budget pressure are visible in the scheduling
snapshot. Terminal enqueue and capability-health commit additionally share one
publication barrier with catalog snapshots: once a consumer can observe a
terminal event, a subsequent capability snapshot cannot expose the pre-terminal
health state.

## Fairness

Control and observation stay fair; within each class, primary and mailbox
fronts merge by `EventSequence`, preserving FIFO even when draining a primary
slot permits a newer publication to refill it. Sequence allocation and
insertion into primary storage or the retained mailbox share one short commit
lock, so concurrent publishers cannot expose sequence N+1 while sequence N is
still between allocation and enqueue. Intermediate progress uses non-blocking
coalescing, while terminal outcomes are never intentionally dropped.

Event draining uses a typed two-partition turn. Control owns the initial turn;
after a delivery the other partition owns the next turn, so control cannot
starve observations. Empty partitions fall through. Within a partition,
primary and retained-terminal queues merge by `EventSequence`; cross-partition
fairness is not causality. `EventFinality::{Progress, Terminal}` selects
coalescing or the bounded terminal mailbox.
