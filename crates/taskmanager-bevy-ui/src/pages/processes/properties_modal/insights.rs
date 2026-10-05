//! Complete identity-matched process insight review, including honest partial states.

use super::super::details::{
    InsightDetail, environment_summary, gpu_summary, isolation_rows, network_summary,
    open_files_summary, threads_summary, unavailable_text,
};
use taskmanager_application::{
    ProcessInsightFacet, ProcessInsightFacetState, ProcessInsightUnavailable,
    ProjectedProcessInsights, i18n::t, project_process_resources,
};
use taskmanager_core::core::FailureKind;
use taskmanager_shell::presentation::{process_insight_facet_label, process_resource_rows};

fn rows<T>(
    state: Option<&ProcessInsightFacetState<T>>,
    facet: ProcessInsightFacet,
    render: impl FnOnce(&T) -> Vec<(String, String)>,
) -> Vec<(String, String)> {
    match state {
        Some(ProcessInsightFacetState::Current(value)) => render(value),
        Some(ProcessInsightFacetState::Unavailable(reason)) => {
            vec![(
                process_insight_facet_label(facet).to_owned(),
                unavailable_text(reason),
            )]
        }
        None | Some(ProcessInsightFacetState::Pending) => {
            vec![(
                process_insight_facet_label(facet).to_owned(),
                t("proc_insights.collecting").to_owned(),
            )]
        }
    }
}
pub(super) fn insight_rows(
    projection: Option<&ProjectedProcessInsights>,
    facet: ProcessInsightFacet,
) -> (Vec<(String, String)>, bool) {
    let detail = InsightDetail::Complete;
    let one = |value| vec![(process_insight_facet_label(facet).to_owned(), value)];
    let values = match facet {
        ProcessInsightFacet::Network => rows(projection.map(|p| &p.network), facet, |v| {
            one(network_summary(v, detail))
        }),
        ProcessInsightFacet::Gpu => rows(projection.map(|p| &p.gpu), facet, |v| {
            one(gpu_summary(v, detail))
        }),
        ProcessInsightFacet::Resources => rows(
            projection.map(|p| &p.resources),
            facet,
            process_resource_rows,
        ),
        ProcessInsightFacet::Isolation => {
            rows(projection.map(|p| &p.isolation), facet, isolation_rows)
        }
        ProcessInsightFacet::Threads => rows(projection.map(|p| &p.threads), facet, |v| {
            one(threads_summary(v, detail))
        }),
        ProcessInsightFacet::Environment => rows(projection.map(|p| &p.environment), facet, |v| {
            one(environment_summary(v, detail))
        }),
        ProcessInsightFacet::OpenFiles => {
            let resources = projection.and_then(|p| match &p.resources {
                ProcessInsightFacetState::Current(v) => Some(project_process_resources(v)),
                _ => None,
            });
            rows(projection.map(|p| &p.open_files), facet, |v| {
                one(open_files_summary(v, resources.as_ref(), detail))
            })
        }
    };
    let authorize = facet == ProcessInsightFacet::Network
        && projection.is_some_and(|p| match &p.network {
            ProcessInsightFacetState::Unavailable(ProcessInsightUnavailable::Provider(
                FailureKind::RequiresEscalation,
            )) => true,
            ProcessInsightFacetState::Current(v) => {
                v.traffic_failure == Some(FailureKind::RequiresEscalation)
            }
            _ => false,
        });
    (values, authorize)
}
