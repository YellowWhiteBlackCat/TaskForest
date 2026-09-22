use super::*;
use taskmanager_application::{AppAction, AppPage, CommandContext, CommandScope, FocusDirection};

fn modifiers(control: bool, alt: bool, shift: bool) -> InputModifiers {
    InputModifiers {
        control,
        alt,
        shift,
        platform: false,
    }
}

#[test]
fn bevy_page_keys_use_the_shared_router_and_reject_wrong_chords() {
    let context = CommandContext {
        scope: CommandScope::Global,
        ..CommandContext::default()
    };
    assert_eq!(
        normalize_key(KeyCode::Digit2, modifiers(false, true, false), context),
        Some(AppAction::SelectPage(AppPage::Applications))
    );
    assert_eq!(
        normalize_key(KeyCode::Digit2, InputModifiers::default(), context),
        None
    );
}

#[test]
fn focus_traversal_and_process_shortcuts_honor_scope_and_text_input() {
    let global = CommandContext {
        scope: CommandScope::Global,
        ..CommandContext::default()
    };
    assert_eq!(
        normalize_key(KeyCode::Tab, modifiers(false, false, true), global),
        Some(AppAction::MoveFocus(FocusDirection::Previous))
    );

    let process_list = CommandContext {
        scope: CommandScope::ProcessList,
        process_selected: true,
        ..CommandContext::default()
    };
    assert_eq!(
        normalize_key(KeyCode::Delete, InputModifiers::default(), process_list),
        Some(AppAction::RequestEndTask)
    );

    let ime_focused = CommandContext {
        text_input_focused: true,
        ..process_list
    };
    assert_eq!(
        normalize_key(KeyCode::Delete, InputModifiers::default(), ime_focused),
        None
    );
    assert_eq!(
        normalize_key(KeyCode::ArrowDown, InputModifiers::default(), ime_focused),
        None
    );
}

#[test]
fn semantic_addresses_are_stable_across_rebuilds() {
    use taskmanager_core::core::metrics::ScalarObservation;
    use taskmanager_core::core::process::{ProcessItem, ProcessScalarObservations};
    use taskmanager_shell::ShellApp;
    use taskmanager_shell::fixture;
    use taskmanager_shell::process_semantic_key;

    fn fixture_process(pid: u32, name: &str, cpu: f32) -> ProcessItem {
        let mut process = ProcessItem::new(pid, name);
        process.apply_scalar_observations(ProcessScalarObservations {
            cpu_percentage: ScalarObservation::available(cpu, 1),
            start_token: ScalarObservation::available(u64::from(pid) * 10_000, 1),
            ..Default::default()
        });
        process
    }

    // Addresses are derived from the production row identity, so the stability
    // claim must hold across a real projection refold — not only across two
    // calls with the same literal.
    let mut shell = ShellApp::new();
    fixture::edit_processes(&mut shell, |processes| {
        *processes = Some(vec![
            fixture_process(100, "alpha", 12.5),
            fixture_process(200, "beta", 3.0),
        ]);
    });
    let _ = shell.apply_action(AppAction::SelectPage(AppPage::Applications));
    let addresses = |shell: &ShellApp| {
        shell
            .visible_processes()
            .iter()
            .map(|row| stable_semantic_address("process-row", &process_semantic_key(row)))
            .collect::<Vec<_>>()
    };

    let before = addresses(&shell);
    assert_eq!(before.len(), 2, "the fixture projects both rows");
    assert_ne!(
        before[0], before[1],
        "distinct process identities own distinct addresses"
    );

    // A refold over the same identities (new observations, new revision) must
    // not re-address the rows.
    fixture::edit_processes(&mut shell, |processes| {
        if let Some(processes) = processes {
            for process in processes.iter_mut() {
                let mut observations = *process.scalar_observations();
                observations.cpu_percentage = ScalarObservation::available(99.0, 9);
                process.apply_scalar_observations(observations);
            }
        }
    });
    assert_eq!(
        before,
        addresses(&shell),
        "a refold must keep every row's semantic address"
    );

    // The accessibility snapshot carries the same stable row identity the
    // address names, so an AT user's join survives the refold too.
    let snapshot = crate::semantic::build_snapshot(&shell).expect("valid semantic snapshot");
    let node_ids: Vec<_> = snapshot.nodes().map(|node| node.id().clone()).collect();
    for row in shell.visible_processes() {
        let expected = taskmanager_ui_contract::SemanticNodeId::owned(format!(
            "row:{}",
            process_semantic_key(row)
        ));
        assert!(
            node_ids.contains(&expected),
            "the AT snapshot must announce the production row identity {expected:?}"
        );
    }
}
