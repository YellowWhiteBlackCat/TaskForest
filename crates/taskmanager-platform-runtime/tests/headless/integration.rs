use std::thread;
use std::time::Duration;

use taskmanager_application::{
    CommandLaunchRequest, PlatformEvent, SetupScriptRequest, ShellEvent,
};
use taskmanager_core::core::identity::ProviderId;
use taskmanager_core::core::setup::{SetupScriptAction, SetupScriptEvent};
use taskmanager_platform_contract::{CapabilityId, RequestEnvelope, RequestId};

use super::*;
use crate::{ProviderBinding, RuntimeConfig, RuntimeProviderBindings};

fn fixed_clock() -> u64 {
    19
}

fn integration_bindings_with_desktop_notification() -> RuntimeProviderBindings {
    let mut bindings = integration_bindings();
    bindings.integration.desktop_notification =
        ProviderBinding::present(ProviderId::borrowed("fixture.alerts.notify"));
    bindings
}

fn integration_bindings() -> RuntimeProviderBindings {
    let provider = ProviderId::borrowed("fixture.integration");
    let mut bindings = RuntimeProviderBindings::default();
    bindings.integration.command_launch = ProviderBinding::present(provider.clone());
    bindings.integration.resource_reveal = ProviderBinding::present(provider.clone());
    bindings.integration.url_open = ProviderBinding::present(provider.clone());
    bindings.integration.desktop_appearance = ProviderBinding::present(provider);
    bindings
}

fn integration_bindings_with_setup_script() -> RuntimeProviderBindings {
    let mut bindings = integration_bindings();
    bindings.integration.setup_script =
        ProviderBinding::present(ProviderId::borrowed("fixture.first-run.setup"));
    bindings
}

#[test]
fn pending_integration_group_promotes_atomically_and_reports_one_missing_lane() {
    let complete = crate::ChannelRuntime::new(
        integration_bindings_with_desktop_notification(),
        RuntimeConfig::new(fixed_clock),
    );
    assert_eq!(complete.lanes.integration.missing_capabilities().count(), 0);
    assert!(complete.lanes.integration.try_complete().is_some());

    let mut incomplete_bindings = integration_bindings_with_desktop_notification();
    incomplete_bindings.integration.resource_reveal = ProviderBinding::absent();
    let incomplete =
        crate::ChannelRuntime::new(incomplete_bindings, RuntimeConfig::new(fixed_clock));
    assert_eq!(
        incomplete
            .lanes
            .integration
            .missing_capabilities()
            .collect::<Vec<_>>(),
        [CapabilityId::RESOURCE_REVEAL]
    );
    assert!(incomplete.lanes.integration.try_complete().is_none());
}

#[test]
fn shared_integration_runtime_maps_command_completion_without_native_event_logic() {
    let runtime =
        crate::ChannelRuntime::new(integration_bindings(), RuntimeConfig::new(fixed_clock));
    let crate::ChannelRuntime {
        handle,
        publisher,
        lanes,
    } = runtime;
    let workers = crate::WorkerRuntime::default();
    spawn_integration_lanes(
        &workers,
        lanes
            .integration
            .try_complete()
            .expect("complete integration lanes"),
        IntegrationExecutors::new(
            |command| {
                assert_eq!(command, "fixture");
                Ok(73)
            },
            |_target, _cached_executable| Err(ProviderFailure::Unsupported),
            |_url| Err(ProviderFailure::Unsupported),
            || Err(ProviderFailure::Unsupported),
        ),
        publisher,
    )
    .expect("integration workers start");
    handle
        .command_launch()
        .expect("command launch port")
        .try_submit(RequestEnvelope {
            id: RequestId::new(1).expect("fixture request id"),
            capability: CapabilityId::COMMAND_LAUNCH,
            submitted_at_ms: 1,
            payload: CommandLaunchRequest {
                command: "fixture".to_owned(),
            },
        })
        .expect("command request accepted");

    for _ in 0..100 {
        if let Some(event) = handle.events().try_recv().expect("connected event port") {
            assert!(matches!(
                event.outcome,
                Ok(PlatformEvent::Shell(ShellEvent::CommandLaunched {
                    pid: 73
                }))
            ));
            return;
        }
        thread::sleep(Duration::from_millis(2));
    }
    panic!("integration runtime event did not arrive");
}

#[test]
fn optional_setup_script_lane_is_typed_and_does_not_change_complete_core_requirements() {
    let runtime = crate::ChannelRuntime::new(
        integration_bindings_with_setup_script(),
        RuntimeConfig::new(fixed_clock),
    );
    let crate::ChannelRuntime {
        handle,
        publisher,
        lanes,
    } = runtime;
    let workers = crate::WorkerRuntime::default();
    spawn_integration_lanes(
        &workers,
        lanes
            .integration
            .try_complete()
            .expect("the four standard integration lanes remain complete"),
        IntegrationExecutors::new(
            |_command| Ok(1),
            |_target, _cached_executable| Ok(()),
            |_url| Ok(()),
            || Err(ProviderFailure::Unsupported),
        )
        .with_setup_script(|action| Ok(SetupScriptEvent::ActionCompleted { action })),
        publisher,
    )
    .expect("integration workers start");
    handle
        .setup_script()
        .expect("present setup provider must create a typed port")
        .try_submit(RequestEnvelope {
            id: RequestId::new(2).expect("fixture request id"),
            capability: CapabilityId::FIRST_RUN_SETUP,
            submitted_at_ms: 1,
            payload: SetupScriptRequest {
                action: SetupScriptAction::Run,
            },
        })
        .expect("setup action accepted by its own bounded lane");

    for _ in 0..100 {
        if let Some(event) = handle.events().try_recv().expect("connected event port") {
            assert!(matches!(
                event.outcome,
                Ok(PlatformEvent::SetupScript(
                    SetupScriptEvent::ActionCompleted {
                        action: SetupScriptAction::Run
                    }
                ))
            ));
            return;
        }
        thread::sleep(Duration::from_millis(2));
    }
    panic!("setup script runtime event did not arrive");
}
