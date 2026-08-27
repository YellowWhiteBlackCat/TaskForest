//! Typed validation for runtime cardinality and retained-memory budgets.

use std::{error, fmt};

use crate::config::{RuntimeConfig, RuntimeProviderBindings};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RuntimeBudgetField {
    RouteLimit,
    ActiveTargetLimit,
    ActiveTargetLimitPerCapability,
    ActiveTargetLimitPerDomain,
    TargetScopeByteLimit,
    PendingDeliveryLimit,
    ControlDeliveryReserve,
    MaxStalledLifetime,
    ObservationRequestQueue,
    ControlRequestQueue,
    ControlEventQueue,
    ObservationEventQueue,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RuntimeConstructionError {
    ZeroBudget(RuntimeBudgetField),
    InconsistentTargetLimits {
        per_capability: usize,
        per_domain: usize,
        global: usize,
    },
    InsufficientDeliveryBudget {
        configured: usize,
        required: usize,
    },
    RouteCapacity {
        routes: usize,
        limit: usize,
    },
}

impl fmt::Display for RuntimeConstructionError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::ZeroBudget(field) => {
                write!(formatter, "runtime budget {field:?} must be nonzero")
            }
            Self::InconsistentTargetLimits {
                per_capability,
                per_domain,
                global,
            } => write!(
                formatter,
                "target limits must satisfy per-capability ({per_capability}) <= per-domain ({per_domain}) <= global ({global})"
            ),
            Self::InsufficientDeliveryBudget {
                configured,
                required,
            } => write!(
                formatter,
                "pending delivery limit {configured} is below route, target, and control-reserve bound {required}"
            ),
            Self::RouteCapacity { routes, limit } => {
                write!(
                    formatter,
                    "runtime route count {routes} exceeds limit {limit}"
                )
            }
        }
    }
}

impl error::Error for RuntimeConstructionError {}

pub(super) fn validate_runtime_config(
    bindings: &RuntimeProviderBindings,
    config: RuntimeConfig,
) -> Result<(), RuntimeConstructionError> {
    let budgets = config.budgets;
    for (value, field) in [
        (budgets.route_limit as u64, RuntimeBudgetField::RouteLimit),
        (
            budgets.active_target_limit as u64,
            RuntimeBudgetField::ActiveTargetLimit,
        ),
        (
            budgets.active_target_limit_per_capability as u64,
            RuntimeBudgetField::ActiveTargetLimitPerCapability,
        ),
        (
            budgets.active_target_limit_per_domain as u64,
            RuntimeBudgetField::ActiveTargetLimitPerDomain,
        ),
        (
            budgets.target_scope_byte_limit as u64,
            RuntimeBudgetField::TargetScopeByteLimit,
        ),
        (
            budgets.pending_delivery_limit as u64,
            RuntimeBudgetField::PendingDeliveryLimit,
        ),
        (
            budgets.control_delivery_reserve as u64,
            RuntimeBudgetField::ControlDeliveryReserve,
        ),
        (
            budgets.max_stalled_lifetime_ms,
            RuntimeBudgetField::MaxStalledLifetime,
        ),
        (
            config.queues.observation_requests as u64,
            RuntimeBudgetField::ObservationRequestQueue,
        ),
        (
            config.queues.control_requests as u64,
            RuntimeBudgetField::ControlRequestQueue,
        ),
        (
            config.queues.control_events as u64,
            RuntimeBudgetField::ControlEventQueue,
        ),
        (
            config.queues.observation_events as u64,
            RuntimeBudgetField::ObservationEventQueue,
        ),
    ] {
        if value == 0 {
            return Err(RuntimeConstructionError::ZeroBudget(field));
        }
    }
    if budgets.active_target_limit_per_capability > budgets.active_target_limit_per_domain
        || budgets.active_target_limit_per_domain > budgets.active_target_limit
    {
        return Err(RuntimeConstructionError::InconsistentTargetLimits {
            per_capability: budgets.active_target_limit_per_capability,
            per_domain: budgets.active_target_limit_per_domain,
            global: budgets.active_target_limit,
        });
    }
    let required_deliveries = budgets
        .route_limit
        .saturating_add(budgets.active_target_limit)
        .saturating_add(budgets.control_delivery_reserve);
    if budgets.pending_delivery_limit < required_deliveries {
        return Err(RuntimeConstructionError::InsufficientDeliveryBudget {
            configured: budgets.pending_delivery_limit,
            required: required_deliveries,
        });
    }
    let routes = bindings.routes().len();
    if routes > budgets.route_limit {
        return Err(RuntimeConstructionError::RouteCapacity {
            routes,
            limit: budgets.route_limit,
        });
    }
    Ok(())
}
