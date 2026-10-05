//! Independent application facet states, with bounded current-data pages.

use super::{
    MAX_INSIGHT_CARD_ROWS, ProcessInsightsViewProps, controls, environment_card,
    escalation_control, gpu_card, gpu_engines, isolation_card, message_panel, network_card,
    open_files, resource_card, threads,
};
#[cfg(any(test, feature = "test-support"))]
use gpui::InteractiveElement;
use gpui::{Div, ParentElement, Styled, div};
use taskmanager_application::ProcessInsightFacet;
use taskmanager_application::project_process_resources;
use taskmanager_application::{
    ProcessInsightFacetState, ProcessInsightUnavailable, ProjectedProcessInsights,
};
use taskmanager_core::core::FailureKind;
use taskmanager_platform_contract::SubmissionErrorKind;
use taskmanager_theme::tokens;
use taskmanager_ui::theme_binding::definite_length;

pub(super) fn render_projection(
    props: &ProcessInsightsViewProps<'_>,
    projection: &ProjectedProcessInsights,
) -> Div {
    let theme = props.theme;
    let labels = props.labels;
    match props.facet {
        ProcessInsightFacet::Network => facet(
            props,
            &projection.network,
            |network| network.connections.len(),
            |network, width, first| network_card(props, network, width, first),
        ),
        ProcessInsightFacet::Gpu => facet(
            props,
            &projection.gpu,
            |_| 0,
            |gpu, width, _| {
                div()
                    .flex()
                    .flex_col()
                    .w_full()
                    .gap(definite_length(tokens::SPACE_8))
                    .child(gpu_card(theme, gpu, labels, width, props.units))
                    .child(gpu_engines::gpu_engines_card(theme, gpu, labels, width))
            },
        ),
        ProcessInsightFacet::Resources => facet(
            props,
            &projection.resources,
            |_| 0,
            |resources, width, _| {
                resource_card(
                    theme,
                    project_process_resources(resources),
                    labels,
                    width,
                    props.units,
                )
            },
        ),
        ProcessInsightFacet::Isolation => facet(
            props,
            &projection.isolation,
            |_| 0,
            |isolation, width, _| isolation_card(theme, isolation, labels, width),
        ),
        ProcessInsightFacet::Threads => facet(
            props,
            &projection.threads,
            |threads| threads.threads.len(),
            |threads, width, first| threads::threads_card(theme, threads, labels, width, first),
        ),
        ProcessInsightFacet::OpenFiles => {
            let resources = match &projection.resources {
                ProcessInsightFacetState::Current(resources) => Some(resources),
                ProcessInsightFacetState::Pending | ProcessInsightFacetState::Unavailable(_) => {
                    None
                }
            };
            facet(
                props,
                &projection.open_files,
                |files| files.entries.len(),
                |files, width, first| {
                    open_files::open_files_card(theme, files, resources, labels, width, first)
                },
            )
        }
        ProcessInsightFacet::Environment => facet(
            props,
            &projection.environment,
            |environment| environment.entries.len(),
            |environment, width, first| environment_card(theme, environment, labels, width, first),
        ),
    }
}

fn facet<T>(
    props: &ProcessInsightsViewProps<'_>,
    state: &ProcessInsightFacetState<T>,
    count: impl FnOnce(&T) -> usize,
    render: impl FnOnce(&T, f32, usize) -> Div,
) -> Div {
    let width = props.available_width.max(240.0);
    match state {
        ProcessInsightFacetState::Pending => {
            state_panel(props, props.labels.loading, "pending", width)
        }
        ProcessInsightFacetState::Unavailable(reason) => {
            let label = match reason {
                ProcessInsightUnavailable::Submission(
                    SubmissionErrorKind::UnsupportedCapability,
                ) => props.labels.unsupported,
                ProcessInsightUnavailable::Submission(SubmissionErrorKind::RuntimeStopped) => {
                    props.labels.worker_disconnected
                }
                ProcessInsightUnavailable::Submission(_) => props.labels.provider_unavailable,
                ProcessInsightUnavailable::Provider(
                    FailureKind::PermissionDenied | FailureKind::RequiresEscalation,
                ) => props.labels.permission_denied,
                ProcessInsightUnavailable::Provider(FailureKind::Unsupported) => {
                    props.labels.unsupported
                }
                ProcessInsightUnavailable::Provider(FailureKind::MissingDependency) => {
                    props.labels.provider_unavailable
                }
                ProcessInsightUnavailable::Provider(_) => props.labels.stale,
            };
            let mut panel = state_panel(props, label, "unavailable", width);
            if props.facet == ProcessInsightFacet::Network
                && matches!(
                    reason,
                    ProcessInsightUnavailable::Provider(FailureKind::RequiresEscalation)
                )
            {
                panel = panel.child(escalation_control(
                    props.theme,
                    props.net_escalation,
                    props.entity.clone(),
                ));
            }
            panel
        }
        ProcessInsightFacetState::Current(data) => {
            let total = count(data);
            let first = props
                .first
                .min(total.saturating_sub(1) / MAX_INSIGHT_CARD_ROWS * MAX_INSIGHT_CARD_ROWS);
            div()
                .w_full()
                .flex()
                .flex_col()
                .gap(definite_length(tokens::SPACE_8))
                .child(controls::page_controls(
                    props.theme,
                    &props.entity,
                    first,
                    total,
                ))
                .child(render(data, width, first))
        }
    }
}

fn state_panel(
    props: &ProcessInsightsViewProps<'_>,
    label: &'static str,
    state: &'static str,
    width: f32,
) -> Div {
    let panel = message_panel(props.theme, label, props.theme.fg_dim, width);
    #[cfg(any(test, feature = "test-support"))]
    let panel = {
        let facet = props.facet;
        panel.debug_selector(move || format!("properties-insight-state:{facet:?}:{state}:{label}"))
    };
    #[cfg(not(any(test, feature = "test-support")))]
    let _ = state;
    panel
}
