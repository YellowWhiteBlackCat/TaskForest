use taskmanager_core::core::identity::ProviderId;
use taskmanager_platform_contract::CapabilityId;

use crate::{ProviderBinding, RuntimeBudgets};

use super::*;

impl ChannelRuntime {
    pub(crate) fn new(bindings: RuntimeProviderBindings, config: RuntimeConfig) -> Self {
        Self::try_new(bindings, config).expect("fixture runtime configuration is valid")
    }
}

#[test]
fn absent_binding_creates_no_port_lane_or_catalog_descriptor() {
    fn fixed_clock() -> u64 {
        7
    }

    let runtime = ChannelRuntime::new(
        RuntimeProviderBindings::default(),
        RuntimeConfig::new(fixed_clock),
    );

    assert!(runtime.handle.host_telemetry().is_none());
    assert!(runtime.lanes.system.observations.host_rx.is_none());
    assert!(
        runtime
            .handle
            .capabilities()
            .snapshot()
            .get(&CapabilityId::TELEMETRY_CPU)
            .is_none()
    );
}

#[test]
fn construction_rejects_zero_and_crossed_budgets_before_allocating_channels() {
    fn fixed_clock() -> u64 {
        7
    }

    let zero = RuntimeBudgets {
        route_limit: 0,
        max_stalled_lifetime_ms: RuntimeBudgets::DEFAULT.max_stalled_lifetime_ms,
        ..RuntimeBudgets::DEFAULT
    };
    assert_eq!(
        ChannelRuntime::try_new(
            RuntimeProviderBindings::default(),
            RuntimeConfig::new(fixed_clock).with_budgets(zero),
        )
        .err(),
        Some(RuntimeConstructionError::ZeroBudget(
            RuntimeBudgetField::RouteLimit
        ))
    );

    let zero_control_reserve = RuntimeBudgets {
        control_delivery_reserve: 0,
        max_stalled_lifetime_ms: RuntimeBudgets::DEFAULT.max_stalled_lifetime_ms,
        ..RuntimeBudgets::DEFAULT
    };
    assert_eq!(
        ChannelRuntime::try_new(
            RuntimeProviderBindings::default(),
            RuntimeConfig::new(fixed_clock).with_budgets(zero_control_reserve),
        )
        .err(),
        Some(RuntimeConstructionError::ZeroBudget(
            RuntimeBudgetField::ControlDeliveryReserve
        ))
    );

    let crossed = RuntimeBudgets {
        active_target_limit: 2,
        active_target_limit_per_domain: 3,
        active_target_limit_per_capability: 1,
        max_stalled_lifetime_ms: RuntimeBudgets::DEFAULT.max_stalled_lifetime_ms,
        ..RuntimeBudgets::DEFAULT
    };
    assert!(matches!(
        ChannelRuntime::try_new(
            RuntimeProviderBindings::default(),
            RuntimeConfig::new(fixed_clock).with_budgets(crossed),
        ),
        Err(RuntimeConstructionError::InconsistentTargetLimits { .. })
    ));

    let undersized_delivery = RuntimeBudgets {
        pending_delivery_limit: RuntimeBudgets::DEFAULT
            .route_limit
            .saturating_add(RuntimeBudgets::DEFAULT.active_target_limit)
            .saturating_add(RuntimeBudgets::DEFAULT.control_delivery_reserve)
            .saturating_sub(1),
        max_stalled_lifetime_ms: RuntimeBudgets::DEFAULT.max_stalled_lifetime_ms,
        ..RuntimeBudgets::DEFAULT
    };
    assert!(matches!(
        ChannelRuntime::try_new(
            RuntimeProviderBindings::default(),
            RuntimeConfig::new(fixed_clock).with_budgets(undersized_delivery),
        ),
        Err(RuntimeConstructionError::InsufficientDeliveryBudget { .. })
    ));
}

#[test]
fn construction_rejects_route_growth_above_the_explicit_limit() {
    fn fixed_clock() -> u64 {
        7
    }

    let provider = ProviderId::borrowed("fixture.routes");
    let mut bindings = RuntimeProviderBindings::default();
    bindings.system.host = ProviderBinding::present(provider.clone());
    bindings.system.cpu = ProviderBinding::present(provider);
    let budgets = RuntimeBudgets {
        route_limit: 1,
        active_target_limit: 1,
        active_target_limit_per_capability: 1,
        active_target_limit_per_domain: 1,
        target_scope_byte_limit: taskmanager_platform_contract::MAX_REQUEST_SCOPE_BYTES,
        pending_delivery_limit: 3,
        control_delivery_reserve: 1,
        max_stalled_lifetime_ms: RuntimeBudgets::DEFAULT.max_stalled_lifetime_ms,
    };

    assert_eq!(
        ChannelRuntime::try_new(
            bindings,
            RuntimeConfig::new(fixed_clock).with_budgets(budgets),
        )
        .err(),
        Some(RuntimeConstructionError::RouteCapacity {
            routes: 2,
            limit: 1,
        })
    );
}
