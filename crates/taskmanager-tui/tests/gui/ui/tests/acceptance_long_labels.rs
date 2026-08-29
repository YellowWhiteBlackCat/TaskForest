//! Long-label / long-path acceptance: processes and services whose names,
//! command lines and executable paths exceed every column budget — pure ASCII
//! and CJK-wide variants — must render with the shared truncation helpers
//! ([`crate::ui::text`]) so no row wraps into a second terminal row, nothing
//! overflows the frame, and the labels stay readable prefixes rather than
//! disappearing. Rendered at the reference and the minimum size, in both
//! locales' frame pipeline, with the raw-key oracle riding along.

use taskmanager_application::i18n::Language;
use taskmanager_application::{AppAction, AppPage};
use taskmanager_core::core::metrics::ScalarObservation;
use taskmanager_core::core::process::{ProcessMetadataObservations, ProcessOwner};

use super::acceptance_support::{
    REFERENCE_HEIGHT, REFERENCE_WIDTH, assert_frame_has_no_raw_catalog_keys, body_text,
    frame_in_language, visible_row_count, with_frame_in_language,
};
use crate::TuiApp;

/// Long pure-ASCII identity: a 90-character name and a ~200-character command
/// line whose path segments never form a dotted key shape.
const LONG_ASCII_NAME: &str =
    "long-ascii-name-abcdefghijklmnopqrstuvwxyz-0123456789-abcdefghijklmnopqrstuvwxyz";
const LONG_ASCII_CMD: &str = "/opt/very/long/install/path/bin/very-long-binary --config /opt/very/long/etc/tree/segments/main-settings --long-flag=aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";

/// Long CJK-wide identity: 60 CJK graphemes (120 terminal cells) so an
/// untruncated name would double-wrap any column.
const LONG_CJK_NAME: &str = "中文超长进程名称测试用于验收电池的宽度与截断行为验证用例";
const LONG_CJK_CMD: &str = "/opt/工具/超长安装路径/bin/可执行文件 --备注=这是一个很长的中文备注参数用于验证命令行截断行为 --长选项=测试测试测试测试测试测试测试测试";

const LONG_ASCII_PATH: &str =
    "/opt/very/long/install/path/bin/long-ascii-name-abcdefghijklmnopqrstuvwxyz";

fn long_named_process(
    pid: u32,
    name: &str,
    cmd: &str,
    path: &str,
    cpu: f32,
) -> taskmanager_core::core::process::ProcessItem {
    let mut item = taskmanager_test_support::ProcessItemFixtureBuilder::new()
        .pid(pid)
        .name(name.to_owned())
        .cmdline(cmd.to_owned())
        .status("Running".to_owned())
        .current_cpu_percentage(cpu)
        .current_memory_bytes(64 * 1024 * 1024)
        .metadata_observations(ProcessMetadataObservations::current(
            ProcessOwner::opaque("devuser"),
            Some(std::path::PathBuf::from(path)),
            1,
        ))
        .current_start_time_secs(1_600_000_000)
        .build();
    // The properties modal (like every control gate) freezes the row through
    // `FrozenProcessIdentity::from_process`, which refuses a row without the
    // provider-native start token — so the fixture must carry one, exactly
    // like the `ui::process_properties` fixture does.
    let mut observations = *item.scalar_observations();
    observations.start_token = ScalarObservation::available(u64::from(pid), 1);
    item.apply_scalar_observations(observations);
    item
}

/// The demo fixture with three top-CPU long-label processes appended (highest
/// CPU first, so they own the first visible table rows at every size) and one
/// long-named service appended to the inventory.
fn long_label_app() -> TuiApp {
    let mut app = TuiApp::demo();
    taskmanager_shell::fixture::edit_processes(&mut app.shell, |processes| {
        if let Some(items) = processes {
            items.push(long_named_process(
                431,
                LONG_ASCII_NAME,
                LONG_ASCII_CMD,
                LONG_ASCII_PATH,
                99.0,
            ));
            items.push(long_named_process(
                432,
                LONG_CJK_NAME,
                LONG_CJK_CMD,
                LONG_ASCII_PATH,
                98.0,
            ));
        }
    });
    taskmanager_shell::fixture::seed_projection_fact(
        &mut app.shell,
        taskmanager_shell::fixture::ProjectionSeedFact::Services(Some(vec![
            taskmanager_core::core::services::ServiceItem::from_inventory(
                taskmanager_core::core::target::ServiceId::new("fixture.service:long.service".to_owned()),
                "a-very-long-service-name-abcdefghijklmnop-qrstuv-abcdefghijklmnopqrstuvwxyz.service"
                    .to_owned(),
                taskmanager_core::core::services::ServiceStatus::Active,
                "一个很长很长的中文服务描述用于验证服务表截断行为没有问题",
                "",
                "",
                "",
            ),
        ])),
    );
    app
}

#[test]
fn long_process_labels_stay_on_one_row_at_reference_and_minimum_sizes() {
    let mut app = long_label_app();
    let _ = app.apply_action(AppAction::SelectPage(AppPage::Applications));

    // At the reference size the whole fixture list is inside the table window,
    // so both long identities must paint readable prefixes. The 54x16 floor
    // fits a single data row (the search field, table chrome and the details
    // band consume the rest of the 9-row body), so only the top row's prefix
    // is asserted there — a row scrolled below the fold is not a truncation
    // defect.
    for (width, height, ascii_prefix, cjk_prefix) in [
        (
            REFERENCE_WIDTH,
            REFERENCE_HEIGHT,
            "long-ascii-name-abcdefghij",
            "中文超长进程",
        ),
        (54, 16, "long-ascii", ""),
    ] {
        let frame = frame_in_language(&app, width, height, Language::En);
        // The rows must be truncated, not dropped: a readable prefix of the
        // visible ASCII identity paints at every size, and the CJK-wide
        // identity joins it wherever both rows are inside the window.
        assert!(
            frame.contains(ascii_prefix),
            "{width}x{height} truncated the long ASCII name to something without {ascii_prefix:?}"
        );
        if !cjk_prefix.is_empty() {
            assert!(
                frame.contains(cjk_prefix),
                "{width}x{height} truncated the long CJK name to something without {cjk_prefix:?}"
            );
        }
        // No wrap may inflate the layout: the body region still fills exactly
        // its pinned row budget (29 rows at 120x36, 9 at 54x16) — extra rows
        // would mean a label escaped the truncation helpers.
        let body = body_text(&frame, height);
        let expected_rows = usize::from(height.saturating_sub(4 + 3));
        assert_eq!(
            visible_row_count(&body),
            expected_rows,
            "{width}x{height} body must fill exactly its {expected_rows}-row budget"
        );
        assert_frame_has_no_raw_catalog_keys(&frame, "long-label applications page", Language::En);
    }
}

#[test]
fn long_service_names_truncate_on_the_services_page_without_inflating_rows() {
    let mut app = long_label_app();
    let _ = app.apply_action(AppAction::SelectPage(AppPage::Services));
    let frame = frame_in_language(&app, REFERENCE_WIDTH, REFERENCE_HEIGHT, Language::En);
    assert!(
        frame.contains("a-very-long-service-name"),
        "the long service row must paint a readable prefix"
    );
    let body = body_text(&frame, REFERENCE_HEIGHT);
    assert_eq!(
        visible_row_count(&body),
        29,
        "a long service label must not inflate the services body rows"
    );
    assert_frame_has_no_raw_catalog_keys(&frame, "long-label services page", Language::En);
}

#[test]
fn process_properties_overlay_survives_long_paths_and_cjk_at_both_sizes() {
    let mut app = long_label_app();
    let _ = app.apply_action(AppAction::SelectPage(AppPage::Applications));
    assert!(
        app.open_process_properties(),
        "a long-label row opens properties"
    );
    // The modal opens on Overview; the long executable path lives on the
    // Command tab (Overview → Performance → Command), which also repaints
    // the identity name — so one tab covers every long-label surface.
    app.process_properties_next_tab();
    app.process_properties_next_tab();

    for (width, height) in [(REFERENCE_WIDTH, REFERENCE_HEIGHT), (54, 16)] {
        with_frame_in_language(&app, width, height, Language::En, |frame| {
            let title = taskmanager_application::i18n::t("prop.process_details");
            assert_ne!(title, "prop.process_details");
            assert!(
                frame.contains(title),
                "{width}x{height} properties modal must keep its title under clamping"
            );
            // The frozen identity stays readable (truncated, never dropped)
            // and the long executable path paints inside the modal bounds.
            assert!(
                frame.contains("long-ascii-name-abcdefghij"),
                "{width}x{height} properties must show the truncated long name"
            );
            assert!(
                frame.contains("/opt/very/long/install/path"),
                "{width}x{height} properties must show the truncated long path"
            );
            assert_frame_has_no_raw_catalog_keys(frame, "long-label properties", Language::En);
        });
    }
}
