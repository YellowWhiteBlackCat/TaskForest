//! Physical retention policy: long host overviews without multiplying device rings.

/// Four host aggregate rings retain one hour at the application's 100 ms minimum cadence.
pub const HOST_AGGREGATE_HISTORY_CAPACITY: usize = 36_001;
/// All device, per-core, receipt and secondary metric rings remain bounded independently.
pub const DETAIL_HISTORY_CAPACITY: usize = 600;

/// Explicit physical budgets for authoritative telemetry rings.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct HistoryRetention {
    detail: usize,
    host_aggregates: usize,
}

impl HistoryRetention {
    pub const PRODUCT: Self = Self {
        detail: DETAIL_HISTORY_CAPACITY,
        host_aggregates: HOST_AGGREGATE_HISTORY_CAPACITY,
    };

    /// A bounded uniform store for isolated compositions and behavior tests.
    #[must_use]
    pub const fn uniform(capacity: usize) -> Self {
        Self::bounded(capacity, capacity)
    }

    #[must_use]
    pub const fn bounded(detail: usize, host_aggregates: usize) -> Self {
        Self {
            detail: clamp(detail, DETAIL_HISTORY_CAPACITY),
            host_aggregates: clamp(host_aggregates, HOST_AGGREGATE_HISTORY_CAPACITY),
        }
    }

    pub(crate) const fn detail(self) -> usize {
        self.detail
    }
    pub(crate) const fn host_aggregates(self) -> usize {
        self.host_aggregates
    }
}

const fn clamp(value: usize, maximum: usize) -> usize {
    if value == 0 {
        1
    } else if value > maximum {
        maximum
    } else {
        value
    }
}
