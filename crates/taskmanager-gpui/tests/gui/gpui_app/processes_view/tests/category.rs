//! Canonical category-hierarchy behavior tests (real mouse path + projection).

use super::*;
use std::collections::HashSet;

/// A primary double-click on a category aggregate row must expand and
/// collapse the same stable-keyed bucket as the chevron and directional keys,
/// through the real mouse event path. The fixture mixes a confirmed-absent
/// pair (Background) with an Unknown-identity process (Uncategorized), so the
/// test also pins the honest three-bucket split end-to-end: two headers
/// collapsed, members revealed only while expanded.
#[gpui::test]
async fn category_group_double_click_expands_and_collapses_the_row(cx: &mut TestAppContext) {
    taskmanager_application::i18n::set_language(taskmanager_application::i18n::Language::En);
    let (win, view) = wrapped_root(cx);
    let background = |pid: u32, name: &str, cpu: f32| {
        taskmanager_test_support::ProcessItemFixtureBuilder::new()
            .pid(pid)
            .name(name.into())
            .current_cpu_percentage(cpu)
            .status("S".into())
            .application_identity_observation(ProcessMetadataObservation::<
                ProcessApplicationIdentity,
            >::absent(10))
            .build()
    };
    let unknown = |pid: u32, name: &str, cpu: f32| {
        taskmanager_test_support::ProcessItemFixtureBuilder::new()
            .pid(pid)
            .name(name.into())
            .current_cpu_percentage(cpu)
            .status("S".into())
            .build()
    };
    view.update(cx, |v, cx| {
        v.mark_telemetry_frame_ready();
        v.page = TopPage::Apps;
        v.processes_state.expanded_apps.clear();
        v.replace_processes_for_test(vec![
            background(101, "syslogd", 4.0),
            background(102, "cron", 2.0),
            unknown(103, "mystery", 0.5),
        ]);
        cx.notify();
    });
    draw(cx, win);

    let mut vcx = VisualTestContext::from_window(win.into(), cx);
    let collapsed = vcx
        .debug_bounds("tm-proc-row-root:0")
        .expect("the collapsed Background aggregate row must render");
    assert!(
        vcx.debug_bounds("tm-proc-row-root:1").is_some()
            && vcx.debug_bounds("tm-proc-row-root:2").is_none(),
        "collapsed canonical hierarchy renders exactly the two non-empty bucket headers"
    );

    let position = collapsed.center();
    vcx.simulate_event(MouseDownEvent {
        position,
        modifiers: Modifiers::none(),
        button: MouseButton::Left,
        click_count: 2,
        first_mouse: false,
    });
    vcx.simulate_event(MouseUpEvent {
        position,
        modifiers: Modifiers::none(),
        button: MouseButton::Left,
        click_count: 2,
    });
    assert!(
        view.read_with(cx, |v, _cx| v
            .processes_state
            .expanded_apps
            .contains("category:background")),
        "a primary double-click must expand the background category bucket"
    );

    draw(cx, win);
    view.update(cx, |v, _cx| {
        let (rows, _, _) = v.processes_projection();
        let names: Vec<&str> = rows.iter().map(|row| row.name.as_str()).collect();
        assert_eq!(
            names,
            ["Background processes", "syslogd", "cron", "Uncategorized",],
            "expanded members follow the CPU%-desc sort at depth 1, and the \
             unknown-identity process stays in the honest Uncategorized bucket"
        );
        assert_eq!(rows[1].depth, 1, "member rows render one level deep");
    });

    let expanded = vcx
        .debug_bounds("tm-proc-row-root:0")
        .expect("the expanded category aggregate row must remain rendered");
    let position = expanded.center();
    vcx.simulate_event(MouseDownEvent {
        position,
        modifiers: Modifiers::none(),
        button: MouseButton::Left,
        click_count: 2,
        first_mouse: false,
    });
    vcx.simulate_event(MouseUpEvent {
        position,
        modifiers: Modifiers::none(),
        button: MouseButton::Left,
        click_count: 2,
    });
    assert!(
        view.read_with(cx, |v, _cx| !v
            .processes_state
            .expanded_apps
            .contains("category:background")),
        "a second primary double-click must collapse the same category bucket"
    );
    view.update(cx, |v, _cx| {
        let (rows, _, _) = v.processes_projection();
        assert_eq!(
            rows.len(),
            2,
            "collapsed canonical hierarchy contains only its two headers"
        );
    });
}

/// Real GPUI event path for the new Applications hierarchy: the category
/// expands to a PID-less app total, and a second double-click expands that
/// total to process rows with distinct PIDs. The aggregate click selects the
/// application row itself and must not select a hidden representative PID.
#[gpui::test]
async fn application_category_opens_pidless_total_then_real_process_tree(cx: &mut TestAppContext) {
    taskmanager_application::i18n::set_language(taskmanager_application::i18n::Language::En);
    let (win, view) = wrapped_root(cx);
    let identity =
        ProcessApplicationIdentity::new("org.example.MissionCenter", "Mission Center", None)
            .expect("fixture identity must be non-empty");
    let process = |pid: u32, name: &str, cpu: f32, parent_pid: Option<u32>| {
        taskmanager_test_support::ProcessItemFixtureBuilder::new()
            .pid(pid)
            .parent_pid(parent_pid)
            .name(name.into())
            .current_cpu_percentage(cpu)
            .current_memory_bytes(100)
            .status("S".into())
            .application_identity_observation(ProcessMetadataObservation::available(
                identity.clone(),
                10,
            ))
            .build()
    };
    view.update(cx, |v, cx| {
        v.mark_telemetry_frame_ready();
        v.page = TopPage::Apps;
        v.processes_state.expanded_apps = HashSet::from(["category:application".to_owned()]);
        v.replace_processes_for_test(vec![
            process(100, "missioncenter", 10.0, None),
            process(101, "missioncenter-magpie", 2.0, Some(100)),
            process(102, "bwrap", 3.0, Some(100)),
        ]);
        cx.notify();
    });
    draw(cx, win);

    let mut vcx = VisualTestContext::from_window(win.into(), cx);
    let app_total = vcx
        .debug_bounds("tm-proc-row-root:1")
        .expect("expanded category must render its application total");
    assert!(
        vcx.debug_bounds("tm-proc-row-root:2").is_none(),
        "the collapsed application total must hide its process tree"
    );
    view.update(cx, |v, _cx| {
        let (rows, _, _) = v.processes_projection();
        assert_eq!(rows[0].process_pid, None);
        assert_eq!(rows[1].process_pid, None);
        assert_eq!(rows[1].cell_text.pid, "");
        assert_eq!(rows[1].name, "Mission Center");
    });

    vcx.simulate_event(MouseDownEvent {
        position: app_total.center(),
        modifiers: Modifiers::none(),
        button: MouseButton::Left,
        click_count: 2,
        first_mouse: false,
    });
    vcx.simulate_event(MouseUpEvent {
        position: app_total.center(),
        modifiers: Modifiers::none(),
        button: MouseButton::Left,
        click_count: 2,
    });
    assert!(view.read_with(cx, |v, _| {
        v.processes_state.expanded_apps.contains("app-tree:100")
    }));
    assert!(view.read_with(cx, |v, _| v.selected_process_count() == 0));
    assert_eq!(
        view.read_with(cx, |v, _| v.selected_process_row()),
        Some(application_row_id(100)),
        "the PID-less application aggregate is the selected row identity"
    );

    draw(cx, win);
    view.update(cx, |v, _cx| {
        let (rows, _, _) = v.processes_projection();
        assert_eq!(
            rows.iter().map(|row| row.process_pid).collect::<Vec<_>>(),
            [None, None, Some(100), Some(102), Some(101)]
        );
        assert_eq!(rows[2].cpu, Some(10.0));
        assert_eq!(rows[3].cpu, Some(3.0));
    });
}

/// The expected row id of one fixture process (token from
/// `fixture_start_token`, the builder's single source).
fn row_id(pid: u32) -> taskmanager_shell::ProcessRowId {
    taskmanager_shell::ProcessRowId::Process(
        taskmanager_shell::ProcessRowIdentity::from_parts(
            pid,
            taskmanager_test_support::fixture_start_token(pid),
        )
        .expect("fixture pid and token are non-zero"),
    )
}

fn application_row_id(pid: u32) -> taskmanager_shell::ProcessRowId {
    taskmanager_shell::ProcessRowId::Application(
        taskmanager_shell::ProcessRowIdentity::from_parts(
            pid,
            taskmanager_test_support::fixture_start_token(pid),
        )
        .expect("fixture pid and token are non-zero"),
    )
}
