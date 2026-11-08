use super::*;

fn key(device: &str, chart: &str) -> DetachedChartKey {
    DetachedChartKey::new(device, chart).expect("valid chart identity")
}

fn detach(mode: ChartWindowDetachMode) -> DetachedChartWindowCommand {
    DetachedChartWindowCommand::Detach {
        key: key("cpu", "main-graph"),
        mode,
    }
}

fn reattach() -> DetachedChartWindowCommand {
    DetachedChartWindowCommand::Reattach {
        key: key("cpu", "main-graph"),
    }
}

#[test]
fn chart_identity_rejects_empty_device_or_chart() {
    assert_eq!(
        DetachedChartKey::new("", "main-graph"),
        Err(DetachedChartKeyError::EmptyIdentity)
    );
    assert_eq!(
        DetachedChartKey::new("   ", "main-graph"),
        Err(DetachedChartKeyError::EmptyIdentity)
    );
    assert_eq!(
        DetachedChartKey::new("cpu", "\t"),
        Err(DetachedChartKeyError::EmptyIdentity)
    );

    let frozen = key("nic:0", "rx-tx");
    assert_eq!(frozen.device(), "nic:0");
    assert_eq!(frozen.chart(), "rx-tx");
}

#[test]
fn default_state_is_embedded_and_capability_is_honestly_unavailable() {
    let state = DetachedChartWindowState::new();
    assert!(!state.is_detached());
    assert_eq!(state.detached(), None);
    assert_eq!(state.capability(), ChartWindowCapability::current());
    let ChartWindowCapability::Unavailable { reason } = state.capability();
    assert!(reason.contains("no detached chart window adapter"));
}

#[test]
fn detach_modes_classify_only_outside_page_requests() {
    assert!(!ChartWindowDetachMode::Embedded.is_detached());
    assert!(ChartWindowDetachMode::SeparateWindow.is_detached());
    assert!(ChartWindowDetachMode::LayerShellSurface.is_detached());
    assert_eq!(
        ChartWindowDetachMode::default(),
        ChartWindowDetachMode::Embedded
    );
}

#[test]
fn every_detach_request_fails_closed_without_mutating_state() {
    let mut state = DetachedChartWindowState::new();
    for mode in [
        ChartWindowDetachMode::SeparateWindow,
        ChartWindowDetachMode::LayerShellSurface,
    ] {
        for _ in 0..2 {
            assert_eq!(
                state.apply(detach(mode)),
                Err(DetachedChartWindowError::CapabilityUnavailable)
            );
            assert!(!state.is_detached(), "refused request must not detach");
            assert_eq!(state.detached(), None);
        }
    }
}

#[test]
fn embedded_request_is_a_reattach_and_refuses_when_no_session_exists() {
    let mut state = DetachedChartWindowState::new();
    assert_eq!(
        state.apply(detach(ChartWindowDetachMode::Embedded)),
        Err(DetachedChartWindowError::NotDetached)
    );
    assert!(!state.is_detached());
}

#[test]
fn reattach_without_matching_session_is_a_typed_refusal() {
    let mut state = DetachedChartWindowState::new();
    assert_eq!(
        state.apply(reattach()),
        Err(DetachedChartWindowError::NotDetached)
    );
    let other = DetachedChartWindowCommand::Reattach {
        key: key("gpu:0", "main-graph"),
    };
    assert_eq!(
        state.apply(other),
        Err(DetachedChartWindowError::NotDetached)
    );
}

#[test]
fn granted_session_reattaches_only_for_its_frozen_chart() {
    let mut state = DetachedChartWindowState {
        detached: Some((
            key("cpu", "main-graph"),
            ChartWindowDetachMode::SeparateWindow,
        )),
    };
    assert!(state.is_detached());
    assert_eq!(
        state.detached(),
        Some((
            &key("cpu", "main-graph"),
            ChartWindowDetachMode::SeparateWindow
        ))
    );
    assert_eq!(
        state.apply(DetachedChartWindowCommand::Detach {
            key: key("cpu", "main-graph"),
            mode: ChartWindowDetachMode::LayerShellSurface,
        }),
        Ok(ChartWindowDetachMode::Embedded)
    );
    assert!(!state.is_detached());

    let other = DetachedChartWindowCommand::Reattach {
        key: key("cpu", "per-core"),
    };
    assert_eq!(
        state.apply(other),
        Err(DetachedChartWindowError::NotDetached)
    );

    assert_eq!(
        state.apply(reattach()),
        Err(DetachedChartWindowError::NotDetached)
    );
}

#[test]
fn granted_session_rejects_a_second_detach_for_a_different_chart() {
    let mut state = DetachedChartWindowState {
        detached: Some((
            key("cpu", "main-graph"),
            ChartWindowDetachMode::SeparateWindow,
        )),
    };
    assert_eq!(
        state.apply(DetachedChartWindowCommand::Detach {
            key: key("gpu:0", "main-graph"),
            mode: ChartWindowDetachMode::LayerShellSurface,
        }),
        Err(DetachedChartWindowError::NotDetached)
    );
    assert_eq!(
        state.detached(),
        Some((
            &key("cpu", "main-graph"),
            ChartWindowDetachMode::SeparateWindow
        ))
    );
    assert_eq!(state.apply(reattach()), Ok(ChartWindowDetachMode::Embedded));
    assert!(!state.is_detached());
    assert_eq!(
        state.apply(reattach()),
        Err(DetachedChartWindowError::NotDetached)
    );
}

#[test]
fn command_errors_are_observable_and_human_readable() {
    let error = DetachedChartWindowError::CapabilityUnavailable;
    assert_eq!(
        error.to_string(),
        "detached chart windows are unavailable on this host"
    );
    assert_eq!(
        DetachedChartKeyError::EmptyIdentity.to_string(),
        "chart identity cannot be empty"
    );
}
