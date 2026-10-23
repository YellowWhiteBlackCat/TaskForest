// test-intent: behavior
//! Headless behaviour tests for the Containers overlay's DECLARED quota
//! (CORE-46): the page compares a container against the ceilings the
//! container declared, never against a cap the page invented.
//!
//! The claims under test:
//! - the over-limit state is raised ONLY for a declared ceiling whose
//!   observed share the observation actually crossed;
//! - a container that declares nothing produces no over-limit state and no
//!   percentage;
//! - a declaration the platform could not read reads "unknown" — never a
//!   "0%" under-the-limit state;
//! - an overrun stays visible: the share is never clamped to 100%;
//! - the declared CPU budget is honoured, and a `cpu.max` that names no
//!   budget declares no ceiling at all.

use super::*;
use crate::demo_app;
use ratatui::Terminal;
use ratatui::backend::TestBackend;
use ratatui::style::Style;
use taskmanager_application::i18n::{Language, set_language};
use taskmanager_core::core::device_state::DeviceState;
use taskmanager_core::core::failure::FailureKind;
use taskmanager_core::core::metrics::ScalarObservation;
use taskmanager_core::core::process_telemetry::{
    ContainerCpuQuota, ContainerQuota, ContainerRollup, ContainerSummary, IsolationKind,
};
use taskmanager_shell::fixture::{ProjectionSeedFact, seed_projection_fact};

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

/// Render the containers overlay over a seeded rollup and return the buffer.
fn overlay_buffer(containers: Vec<ContainerSummary>) -> Terminal<TestBackend> {
    let _guard = crate::ui::test_support::LANG_TEST_GUARD
        .lock()
        .expect("lang test guard");
    set_language(Language::En);
    let mut app = demo_app();
    seed_projection_fact(
        &mut app.shell,
        ProjectionSeedFact::Containers(Some(ContainerRollup {
            state: DeviceState::healthy(1_000),
            containers,
        })),
    );
    let backend = TestBackend::new(120, 36);
    let mut terminal = Terminal::new(backend).expect("test terminal");
    terminal
        .draw(|frame| {
            super::super::containers_support::render_containers_overlay(
                frame,
                &app,
                crate::TuiTheme::default(),
                frame.area(),
            );
        })
        .expect("draw");
    terminal
}

/// The style of the first cell that carries `needle` in the rendered buffer.
fn first_cell_style(terminal: &Terminal<TestBackend>, needle: &str) -> Option<Style> {
    let buffer = terminal.backend().buffer();
    let symbols: Vec<char> = buffer
        .content
        .iter()
        .map(|cell| cell.symbol().chars().next().unwrap_or(' '))
        .collect();
    let needle: Vec<char> = needle.chars().collect();
    let position = symbols
        .windows(needle.len())
        .position(|window| window == needle)?;
    Some(buffer.content[position].style())
}

/// The overlay paints the DECLARED quota: the overrun shows its real share,
/// an undeclared quota says so, and an unreadable one says unknown — never a
/// "0%" under-the-limit state.
#[test]
fn the_overlay_paints_the_declared_quota_and_never_an_invented_limit() {
    let terminal = overlay_buffer(vec![
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
    ]);
    let text = terminal.backend().to_string();
    assert!(
        text.contains("150.0%"),
        "the overrun paints the share it actually reached: {text}"
    );
    assert!(
        !text.contains("100.0%"),
        "an overrun never reads as exactly the declared ceiling: {text}"
    );
    assert!(
        text.contains(t("proc_insights.not_declared")),
        "an undeclared quota reads as not declared: {text}"
    );
    assert!(
        text.contains(t("proc_insights.unknown")),
        "an unreadable declaration reads as unknown: {text}"
    );
}

/// The over-limit state rides the row: only the row that crossed a DECLARED
/// ceiling wears the warning ink.
#[test]
fn only_the_row_that_crossed_a_declared_ceiling_wears_the_warning_ink() {
    let terminal = overlay_buffer(vec![
        summary_with_quota("over", Some(5.0), Some(3 * GIB), soft_ceiling(2 * GIB)),
        summary_with_quota(
            "plain",
            Some(5.0),
            Some(64 * GIB),
            ContainerQuota::not_declared(),
        ),
    ]);
    let theme = crate::TuiTheme::default();
    let over = first_cell_style(&terminal, "over").expect("the over-limit row paints");
    assert_eq!(
        over.fg,
        Some(theme.warn),
        "the row that crossed a declared ceiling wears the warning ink"
    );
    let plain = first_cell_style(&terminal, "plain").expect("the plain row paints");
    assert_ne!(
        plain.fg,
        Some(theme.warn),
        "a container that declares nothing wears no warning ink"
    );
}

/// The quota column keeps its slot: the readout stays inside the overlay's
/// bordered panel, never past its right edge.
#[test]
fn the_quota_readout_never_paints_past_the_panel_border() {
    let terminal = overlay_buffer(vec![summary_with_quota(
        "over",
        Some(5.0),
        Some(3 * GIB),
        soft_ceiling(2 * GIB),
    )]);
    let buffer = terminal.backend().buffer();
    let width = buffer.area.width as usize;
    let mut readout_column = None;
    let mut border_column = None;
    for y in 0..buffer.area.height {
        let row: String = buffer.content[y as usize * width..(y as usize + 1) * width]
            .iter()
            .map(|cell| cell.symbol())
            .collect();
        if readout_column.is_none()
            && let Some(position) = row.find("150.0%")
        {
            readout_column = Some(position);
        }
        if border_column.is_none()
            && let Some(position) = row.rfind('│')
        {
            border_column = Some(position);
        }
    }
    let readout_column = readout_column.expect("the quota readout paints");
    let border_column = border_column.expect("the overlay paints a bordered panel");
    assert!(
        readout_column + "150.0%".len() <= border_column,
        "the quota readout stays inside the panel: {readout_column} >= {border_column}"
    );
    let text = terminal.backend().to_string();
    assert!(
        text.contains("Resource limits"),
        "the quota column keeps its header: {text}"
    );
}
