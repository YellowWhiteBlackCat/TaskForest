//! Containers page: the DECLARED quota (CORE-46).
//!
//! The claims under test:
//! - a declared ceiling whose observed share TOUCHES the declared line raises
//!   the over-limit state, and the printed share is never clamped;
//! - a container that declares nothing raises no over-limit state and shows
//!   no percentage at all;
//! - a declaration the platform could not read reads "unknown" — never a
//!   "0%" under-the-limit state;
//! - the declared CPU budget counts, and a `cpu.max` that names no budget
//!   declares no ceiling;
//! - the quota cell keeps its slot: it never leaves the row's content box.

use super::*;
use crate::gpui_app::root::{RootView, TopPage};
use gpui::{AppContext, TestAppContext, VisualTestContext, px};
use taskforest_core::core::device_state::DeviceState;
use taskforest_core::core::failure::FailureKind;
use taskforest_core::core::metrics::ScalarObservation;
use taskforest_core::core::process_telemetry::{
    ContainerCpuQuota, ContainerQuota, ContainerRollup, ContainerSummary, IsolationKind,
};
use taskmanager_application::i18n::t;
use taskmanager_theme::Theme;

const GIB: u64 = 1024 * 1024 * 1024;

/// One container summary with an explicit declared quota.
fn summary_with_quota(
    name: &str,
    cpu_percent: Option<f32>,
    memory_bytes: Option<u64>,
    quota: ContainerQuota,
) -> ContainerSummary {
    ContainerSummary {
        id: format!("/docker/{name}"),
        name: name.to_owned(),
        runtime: Some(IsolationKind::Docker),
        cgroup_path: format!("/docker/{name}"),
        cpu_percentage: cpu_percent.map_or_else(
            || ScalarObservation::unavailable(FailureKind::TimedOut),
            |value| ScalarObservation::available(value, 1_000),
        ),
        memory_bytes: memory_bytes.map_or_else(
            || ScalarObservation::unavailable(FailureKind::TimedOut),
            |value| ScalarObservation::available(value, 1_000),
        ),
        member_pids: vec![42],
        quota,
    }
}

/// A quota that declares a soft memory ceiling.
fn soft_ceiling(bytes: u64) -> ContainerQuota {
    ContainerQuota {
        memory_soft_limit_bytes: Some(bytes),
        ..ContainerQuota::not_declared()
    }
}

/// A quota that declares a CPU budget of whole CPUs.
fn cpu_budget(cpus: u64) -> ContainerQuota {
    ContainerQuota {
        cpu: Some(ContainerCpuQuota {
            quota_us: Some(cpus * 100_000),
            period_us: 100_000,
        }),
        ..ContainerQuota::not_declared()
    }
}

/// A quota whose CPU declaration names no budget at all ("max").
fn cpu_without_budget() -> ContainerQuota {
    ContainerQuota {
        cpu: Some(ContainerCpuQuota {
            quota_us: None,
            period_us: 100_000,
        }),
        ..ContainerQuota::not_declared()
    }
}

#[test]
fn the_declared_quota_state_separates_unknown_undeclared_and_declared() {
    let undeclared = container_quota_view(&summary_with_quota(
        "plain",
        Some(5.0),
        Some(GIB),
        ContainerQuota::not_declared(),
    ));
    assert_eq!(
        undeclared.state,
        ContainerQuotaState::NotDeclared,
        "a container that declares no ceiling is not declared, never under a limit"
    );
    assert!(
        undeclared.breach().is_none(),
        "an undeclared ceiling never produces an over-limit state"
    );
    assert_eq!(
        undeclared.memory_share_percent, None,
        "an undeclared ceiling has no share to show"
    );

    let unknown = container_quota_view(&summary_with_quota(
        "unreadable",
        Some(5.0),
        Some(GIB),
        ContainerQuota::unavailable(FailureKind::PermissionDenied),
    ));
    assert_eq!(
        unknown.state,
        ContainerQuotaState::Unknown,
        "an unreadable declaration is unknown, never a zero"
    );
    assert!(
        unknown.breach().is_none(),
        "an unknown declaration never claims a breach"
    );
    assert_eq!(
        unknown.memory_share_percent, None,
        "an unknown declaration has no share to show"
    );

    let declared = container_quota_view(&summary_with_quota(
        "capped",
        Some(5.0),
        Some(GIB),
        soft_ceiling(2 * GIB),
    ));
    assert_eq!(
        declared.state,
        ContainerQuotaState::Declared,
        "a declared ceiling is read as declared"
    );
    assert_eq!(
        declared.memory_share_percent,
        Some(50.0),
        "the observed share is the observed one, not a page policy"
    );
    assert!(
        declared.breach().is_none(),
        "a container below its declared ceiling is not over the limit"
    );
}

#[test]
fn an_undeclared_ceiling_produces_no_over_limit_state_and_no_percentage() {
    let view = container_quota_view(&summary_with_quota(
        "plain",
        Some(5.0),
        Some(64 * GIB),
        ContainerQuota::not_declared(),
    ));
    assert!(
        view.breach().is_none(),
        "a container that declares nothing never raises the over-limit state"
    );
    let readout = quota_readout(&view);
    assert_eq!(
        readout,
        t("proc_insights.not_declared"),
        "an undeclared quota reads as not declared, never as a percentage"
    );
    assert!(
        !readout.contains('%'),
        "an undeclared quota shows no percentage at all: {readout}"
    );
}

#[test]
fn an_unreadable_declaration_reads_unknown_instead_of_a_zero() {
    let view = container_quota_view(&summary_with_quota(
        "unreadable",
        Some(5.0),
        Some(GIB),
        ContainerQuota::unavailable(FailureKind::PermissionDenied),
    ));
    assert!(
        view.breach().is_none(),
        "an unreadable declaration never raises the over-limit state"
    );
    let readout = quota_readout(&view);
    assert_eq!(
        readout,
        t("proc_insights.unknown"),
        "the unknown quota is named, never rendered as a zero"
    );
    assert!(
        !readout.contains("0%"),
        "an unknown quota never becomes a zero percent state: {readout}"
    );
}

#[test]
fn the_over_limit_state_is_raised_when_the_observed_share_touches_the_ceiling() {
    // The soft ceiling is the throttling line: touching it is already over.
    let touching = container_quota_view(&summary_with_quota(
        "touching",
        Some(5.0),
        Some(2 * GIB),
        soft_ceiling(2 * GIB),
    ));
    assert_eq!(
        touching.breach(),
        Some(ContainerQuotaBreach::Memory {
            share_percent: 100.0
        }),
        "a share that equals the declared ceiling is already over the line"
    );

    let below = container_quota_view(&summary_with_quota(
        "below",
        Some(5.0),
        Some(GIB),
        soft_ceiling(2 * GIB),
    ));
    assert_eq!(
        below.breach(),
        None,
        "a share below the declared ceiling is not over the limit"
    );
}

#[test]
fn an_overrun_stays_visible_and_is_never_clamped() {
    let view = container_quota_view(&summary_with_quota(
        "over",
        Some(5.0),
        Some(3 * GIB),
        soft_ceiling(2 * GIB),
    ));
    assert_eq!(
        view.breach(),
        Some(ContainerQuotaBreach::Memory {
            share_percent: 150.0
        }),
        "the breach carries the real share, not a clamped hundred percent"
    );
    assert_eq!(
        quota_readout(&view),
        "150.0%",
        "the row shows the overrun as the percentage it reached"
    );
}

#[test]
fn the_declared_cpu_budget_counts_and_a_max_declaration_declares_no_ceiling() {
    let over = container_quota_view(&summary_with_quota(
        "over",
        Some(150.0),
        Some(GIB),
        cpu_budget(1),
    ));
    assert_eq!(
        over.breach(),
        Some(ContainerQuotaBreach::Cpu {
            observed_cpus: 1.5,
            budget_cpus: 1.0,
        }),
        "a load above the declared CPU budget is an over-limit state"
    );

    let within = container_quota_view(&summary_with_quota(
        "within",
        Some(100.0),
        Some(GIB),
        cpu_budget(1),
    ));
    assert_eq!(
        within.breach(),
        None,
        "a load that equals the declared budget is still within it"
    );

    let unbounded = container_quota_view(&summary_with_quota(
        "max",
        Some(400.0),
        Some(GIB),
        cpu_without_budget(),
    ));
    assert_eq!(
        unbounded.state,
        ContainerQuotaState::NotDeclared,
        "a CPU declaration without a budget declares no ceiling"
    );
    assert_eq!(
        unbounded.breach(),
        None,
        "a CPU declaration without a budget never produces an over-limit state"
    );
    assert_eq!(
        unbounded.cpu_budget_cpus, None,
        "the page claims no CPU budget the container did not declare"
    );
}

#[test]
fn a_declared_ceiling_without_an_observation_claims_no_percentage() {
    let view = container_quota_view(&summary_with_quota(
        "unobserved",
        None,
        None,
        soft_ceiling(2 * GIB),
    ));
    assert_eq!(
        view.state,
        ContainerQuotaState::Declared,
        "the declaration was read"
    );
    assert_eq!(
        view.memory_share_percent, None,
        "an unobserved usage has no share"
    );
    let readout = quota_readout(&view);
    assert!(
        !readout.contains('%'),
        "an unobserved usage never becomes a percentage: {readout}"
    );
    assert!(
        readout.contains("2.0"),
        "the row names the declared ceiling it knows: {readout}"
    );
}

/// Drive the Containers page with one rollup and return the drawn frame's
/// probe context.
fn containers_page(cx: &mut TestAppContext, rollup: ContainerRollup) -> VisualTestContext {
    let win = cx.add_window(|_window, cx| RootView::new(Theme::dark(), cx));
    let view = win.entity(cx).expect("window root RootView entity");
    view.update(cx, |v, cx| {
        v.mark_telemetry_frame_ready();
        v.page = TopPage::Containers;
        v.replace_containers_for_test(rollup);
        cx.notify();
    });
    cx.update_window(win.into(), |_, window, cx| window.draw(cx).clear(cx))
        .unwrap();
    VisualTestContext::from_window(win.into(), cx)
}

/// The over-limit marker is painted for the row that crossed a DECLARED
/// ceiling and for no other row — an undeclared or unreadable quota never
/// wears it.
#[gpui::test]
async fn only_the_row_that_crossed_a_declared_ceiling_wears_the_marker(cx: &mut TestAppContext) {
    let rollup = ContainerRollup {
        state: DeviceState::healthy(1_000),
        containers: vec![
            summary_with_quota("under", Some(5.0), Some(GIB), soft_ceiling(2 * GIB)),
            summary_with_quota("over", Some(5.0), Some(3 * GIB), soft_ceiling(2 * GIB)),
            summary_with_quota(
                "plain",
                Some(5.0),
                Some(64 * GIB),
                ContainerQuota::not_declared(),
            ),
            summary_with_quota(
                "unreadable",
                Some(5.0),
                Some(64 * GIB),
                ContainerQuota::unavailable(FailureKind::PermissionDenied),
            ),
        ],
    };
    let mut vcx = containers_page(cx, rollup);

    let over: &'static str = "tm-containers-over-limit:1";
    let under: &'static str = "tm-containers-over-limit:0";
    let plain: &'static str = "tm-containers-over-limit:2";
    let unreadable: &'static str = "tm-containers-over-limit:3";
    assert!(
        vcx.debug_bounds(over).is_some(),
        "the row that crossed the declared ceiling wears the over-limit marker"
    );
    assert!(
        vcx.debug_bounds(under).is_none(),
        "a row below its declared ceiling wears no over-limit marker"
    );
    assert!(
        vcx.debug_bounds(plain).is_none(),
        "a container that declares nothing wears no over-limit marker"
    );
    assert!(
        vcx.debug_bounds(unreadable).is_none(),
        "an unreadable declaration wears no over-limit marker"
    );
    drop(vcx);
}

/// The quota cell keeps its slot: it stays inside the row's content box and
/// keeps a measurable width of its own.
#[gpui::test]
async fn the_quota_cell_stays_inside_the_row(cx: &mut TestAppContext) {
    let rollup = ContainerRollup {
        state: DeviceState::healthy(1_000),
        containers: vec![
            summary_with_quota("over", Some(5.0), Some(3 * GIB), soft_ceiling(2 * GIB)),
            summary_with_quota(
                "plain",
                Some(5.0),
                Some(64 * GIB),
                ContainerQuota::not_declared(),
            ),
        ],
    };
    let mut vcx = containers_page(cx, rollup);

    for index in 0..2 {
        let row_selector: &'static str =
            Box::leak(format!("tm-containers-row:{index}").into_boxed_str());
        let cell_selector: &'static str =
            Box::leak(format!("tm-containers-quota:{index}").into_boxed_str());
        let row = vcx
            .debug_bounds(row_selector)
            .unwrap_or_else(|| panic!("container row {index} must paint"));
        let cell = vcx
            .debug_bounds(cell_selector)
            .unwrap_or_else(|| panic!("the quota cell of row {index} must paint"));
        assert!(
            cell.size.width > px(10.0),
            "the quota cell keeps a measurable slot at row {index}: {cell:?}"
        );
        assert!(
            cell.right() <= row.right(),
            "the quota cell stays inside the row's content box at row {index}: \
             {:?} > {:?}",
            cell.right(),
            row.right()
        );
    }
    drop(vcx);
}
