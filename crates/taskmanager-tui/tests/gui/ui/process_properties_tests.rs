//! Parity with the neutral process-details VM: a fixed fixture folds to
//! the same row counts and values the application-layer VM produces —
//! the TUI modal only adds labels, joins two VM fields per combined row,
//! and appends peak metadata (behavior acceptance, not source text).
use super::*;
use std::path::PathBuf;
use taskmanager_application::process_details_vm::{DetailValue, ProcessDetailsField, detail_value};
use taskmanager_core::core::metrics::ScalarObservation;
use taskmanager_core::core::process::ProcessItem;

use ratatui::Terminal;
use ratatui::backend::TestBackend;
use ratatui::layout::Rect;
use taskmanager_application::SurfaceKind;
use taskmanager_application::i18n::{Language, set_language};
use taskmanager_application::process_details_vm::process_details_rows_with_local_time;
use taskmanager_core::core::process::{ProcessMetadataObservations, ProcessOwner};
use taskmanager_core::core::time::{LocalTimeRules, LocalTimeRulesObservation};
use taskmanager_test_support::ProcessItemFixtureBuilder;

fn fixture() -> ProcessItem {
    // Every measurement enters through the typed fixture builder or the
    // canonical observation group; no schema-v1 row mirror participates.
    let mut item = ProcessItemFixtureBuilder::new()
        .pid(4242)
        .parent_pid(Some(1))
        .name("sample".to_owned())
        .cmdline("sample --flag value".to_owned())
        .current_cpu_percentage(12.5)
        .current_memory_bytes(100 * 1024 * 1024)
        .current_disk_read_bytes_per_sec(1536)
        .current_disk_write_bytes_per_sec(1024 * 1024)
        .status("S".to_owned())
        .metadata_observations(ProcessMetadataObservations::current(
            ProcessOwner::opaque("root"),
            Some(PathBuf::from("/usr/bin/sample")),
            42,
        ))
        .current_threads(8)
        .current_start_time_secs(1_600_000_000)
        .current_cpu_time_secs(3_690)
        .current_fds(42)
        .current_nice(10)
        .build();
    let mut observations = *item.scalar_observations();
    observations.start_token = ScalarObservation::available(600, 42);
    observations.memory_pss_bytes = ScalarObservation::available(50 * 1024 * 1024, 42);
    observations.swap_bytes = ScalarObservation::available(2 * 1024 * 1024, 42);
    observations.disk_read_bytes_total = ScalarObservation::available(10 * 1024 * 1024, 42);
    observations.disk_write_bytes_total = ScalarObservation::available(20 * 1024 * 1024, 42);
    // The two per-process memory counters the definitions name: the anonymous
    // transparent-huge-page charge (`smaps` AnonHugePages) rides the shared
    // scalar group; the minor/major fault counters are typed `ProcessItem`
    // fields (`/proc/<pid>/stat` minflt/majflt).
    observations.memory_anon_huge_pages_bytes = ScalarObservation::available(8 * 1024 * 1024, 42);
    item.apply_scalar_observations(observations);
    item.minor_page_faults = Some(1_234_567);
    item.major_page_faults = Some(42);
    item
}

fn vm(field: ProcessDetailsField) -> String {
    let rows = process_details_rows_with_local_time(
        &fixture(),
        &UnitPreferences::default(),
        &LocalTimeRulesObservation::current(LocalTimeRules::utc(), 0),
    );
    match detail_value(&rows, field) {
        DetailValue::Text(text) => text.clone(),
        DetailValue::Missing => "—".to_owned(),
    }
}

#[test]
fn overview_rows_mirror_the_neutral_vm() {
    let pairs = overview_pairs(
        &fixture(),
        &LocalTimeRulesObservation::current(LocalTimeRules::utc(), 0),
    );
    assert_eq!(pairs.len(), 16);
    let fields = [
        ProcessDetailsField::Name,
        ProcessDetailsField::Pid,
        ProcessDetailsField::ParentPid,
        ProcessDetailsField::User,
        ProcessDetailsField::Status,
        ProcessDetailsField::Threads,
        ProcessDetailsField::Pss,
        ProcessDetailsField::Uss,
        ProcessDetailsField::Shared,
        ProcessDetailsField::AnonHugePages,
        ProcessDetailsField::SchedPolicy,
        ProcessDetailsField::OomScore,
        ProcessDetailsField::PageFaults,
        ProcessDetailsField::NetworkRate,
        ProcessDetailsField::CancelledWriteBytes,
        ProcessDetailsField::StartTime,
    ];
    for (row, field) in pairs.iter().zip(fields) {
        assert_eq!(row.1, vm(field), "{field:?} value must come from the VM");
    }
}

#[test]
fn command_rows_mirror_the_neutral_vm() {
    let pairs = command_pairs(
        &fixture(),
        &LocalTimeRulesObservation::current(LocalTimeRules::utc(), 0),
    );
    assert_eq!(pairs.len(), 3);
    assert_eq!(pairs[0].1, vm(ProcessDetailsField::Name));
    assert_eq!(pairs[1].1, vm(ProcessDetailsField::Exe));
    assert_eq!(pairs[2].1, vm(ProcessDetailsField::Cmdline));
}

#[test]
fn performance_currents_mirror_the_neutral_vm() {
    set_language(Language::En);
    let pairs = performance_pairs(
        &fixture(),
        &LocalTimeRulesObservation::current(LocalTimeRules::utc(), 0),
    );
    assert_eq!(pairs.len(), 4);
    // With an empty history window the peak floors at the live reading,
    // so every row renders "{current} (peak {current})".
    for (row, field) in pairs.iter().zip([
        ProcessDetailsField::Cpu,
        ProcessDetailsField::Memory,
        ProcessDetailsField::DiskReadRate,
        ProcessDetailsField::DiskWriteRate,
    ]) {
        let current = vm(field);
        assert_eq!(
            row.1,
            format!("{current} (peak {current})"),
            "{field:?} current must come from the VM"
        );
    }
}

// ── Modal host contract ──────────────────────────────────────────────────────

/// The Properties modal delegates its host to the shared plain-titled `Modal`
/// component: a complete accent border frame whose title row carries the
/// frozen identity ("Process details <name> · <pid>", no icon).
#[test]
fn properties_modal_host_paints_border_and_identity_title() {
    let _guard = crate::ui::test_support::LANG_TEST_GUARD
        .lock()
        .expect("lang test guard");
    set_language(Language::En);
    let app = crate::demo_app();
    let theme = crate::TuiTheme::default();
    let target = ProcessPropertiesTarget {
        item: fixture(),
        section: ProcessDetailsSection::Overview,
        scroll: 0,
    };
    let focus = crate::ui::frame_plan::TuiFocusPlan {
        target: crate::ui::frame_plan::TuiFocusTarget::SharedSurface(
            SurfaceKind::ProcessProperties,
        ),
        order: crate::ui::frame_plan::TuiFocusOrder::None,
        control: crate::ui::frame_plan::TuiFocusControl::PropertiesTab(
            ProcessDetailsSection::Overview,
        ),
    };
    let popup = Rect::new(2, 1, 60, 14);
    let mut terminal = Terminal::new(TestBackend::new(70, 18)).expect("test terminal");
    terminal
        .draw(|frame| {
            super::render_process_properties_at(frame, &target, &app, theme, focus, popup);
        })
        .expect("draw");

    let buffer = terminal.backend().buffer();
    // The full border frame paints the accent tone (the shared Modal host).
    assert_eq!(buffer[(popup.x, popup.y)].symbol(), "┌");
    assert_eq!(buffer[(popup.right() - 1, popup.y)].symbol(), "┐");
    assert_eq!(buffer[(popup.x, popup.bottom() - 1)].symbol(), "└");
    assert_eq!(
        buffer[(popup.right() - 1, popup.bottom() - 1)].symbol(),
        "┘"
    );
    assert_eq!(buffer[(popup.x, popup.y)].style().fg, Some(theme.accent));

    // The title row carries the plain padded identity title (no icon glyph).
    let width = buffer.area.width as usize;
    let start = popup.y as usize * width;
    let title_row: String = buffer.content[start..start + width]
        .iter()
        .map(|cell| cell.symbol())
        .collect();
    assert!(
        title_row.contains(&format!(
            " {} {} · {} ",
            t("prop.process_details"),
            "sample",
            "4242"
        )),
        "the title row must carry the frozen identity, got: {title_row:?}"
    );
}
