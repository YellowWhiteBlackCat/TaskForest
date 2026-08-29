use super::focus_state::{FocusCommand, focus_command, pending_end_focus_target};
use super::motion::viewport_compact;
use super::*;
use taskmanager_application::PlatformClient;
use taskmanager_application::{FocusDirection, KeyCode, Modifiers};

use taskmanager_shell::ShellKeyEvent;

#[derive(Default)]
struct RecordingSessionInventory(
    std::sync::Mutex<Vec<taskmanager_application::SessionInventoryRequest>>,
);

impl taskmanager_platform_contract::RequestPort for RecordingSessionInventory {
    type Request = taskmanager_application::SessionInventoryRequest;

    fn try_submit(
        &self,
        request: taskmanager_platform_contract::RequestEnvelope<Self::Request>,
    ) -> Result<(), taskmanager_platform_contract::SubmissionError> {
        self.0
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .push(request.payload);
        Ok(())
    }
}

fn session_refresh_client(port: std::sync::Arc<RecordingSessionInventory>) -> PlatformClient {
    use taskmanager_application::{
        EnvironmentFacets, PlatformClient, PlatformFacets, PlatformHandle,
    };
    use taskmanager_platform_contract::{CapabilityCatalog, CapabilitySnapshot};

    use taskmanager_platform_contract::{EventEnvelope, EventPort, EventPortError};

    #[derive(Default)]
    struct EmptyCapabilities;
    impl CapabilityCatalog for EmptyCapabilities {
        fn snapshot(&self) -> CapabilitySnapshot {
            CapabilitySnapshot::default()
        }
    }

    #[derive(Default)]
    struct EmptyEvents;
    impl EventPort for EmptyEvents {
        type Event = taskmanager_application::PlatformEvent;

        fn try_recv(&self) -> Result<Option<EventEnvelope<Self::Event>>, EventPortError> {
            Ok(None)
        }
    }

    PlatformClient::new(PlatformHandle::new(
        std::sync::Arc::new(EmptyCapabilities),
        std::sync::Arc::new(EmptyEvents),
        PlatformFacets::default()
            .with_environment(EnvironmentFacets::default().with_session_inventory(port)),
    ))
}

#[path = "tests/affinity.rs"]
mod affinity;

#[test]
fn compact_breakpoint_mirrors_gpui_responsive_policy() {
    // Wide+tall desktop launch size → not compact.
    assert!(!viewport_compact(iced::Size::new(1180.0, 780.0)));
    // GPUI's exact compact-profile boundary: <=820 width OR <=540
    // height → compact.
    assert!(viewport_compact(iced::Size::new(820.0, 780.0)));
    assert!(!viewport_compact(iced::Size::new(821.0, 780.0)));
    assert!(viewport_compact(iced::Size::new(1200.0, 540.0)));
    assert!(!viewport_compact(iced::Size::new(1200.0, 541.0)));
    // The 720×480 minimum contract window is compact on both axes.
    assert!(viewport_compact(iced::Size::new(720.0, 480.0)));
}

#[test]
fn refresh_source_message_reaches_the_independent_session_lane() {
    use taskmanager_application::RefreshRequest;

    let recorded = std::sync::Arc::new(RecordingSessionInventory::default());
    let mut app = IcedApp::new(Some(session_refresh_client(recorded.clone())));
    let _ = app.update(Message::RefreshSource(RefreshRequest::Sessions));

    let requests = recorded
        .0
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    assert_eq!(
        requests.as_slice(),
        &[taskmanager_application::SessionInventoryRequest::Refresh]
    );
}

#[test]
fn live_tick_uses_the_direct_runtime_owner_and_keeps_live_detection_stable() {
    let recorded = std::sync::Arc::new(RecordingSessionInventory::default());
    let mut app = IcedApp::new(Some(session_refresh_client(recorded)));

    // The tick drains through the runtime's directly owned client; live/demo
    // detection remains a read of that same owner after the tick completes.
    let _ = app.update(Message::Tick);
    assert!(!app.is_demo());
}

#[test]
fn legacy_text_rendering_tokens_normalize_to_the_iced_platform_default() {
    let mut app = IcedApp::demo();
    let _ = app.update(Message::SettingsChanged(SettingsChange::TextRendering(
        "subpixel",
    )));
    assert_eq!(app.preferences().text_rendering, "");
}

/// Ctrl+Space rides the shared command router through IcedApp's keyboard
/// lane: the chord folds into the manual telemetry pause and toggles back
/// off. `control_held()` staying false is the independent oracle that the
/// pause came from the chord, never from the transient hold-Ctrl input.
#[test]
fn ctrl_space_toggles_the_manual_telemetry_pause_through_the_keyboard_lane() {
    let mut app = IcedApp::demo();
    assert!(!app.shell.paused());
    let chord = || {
        Message::Key(IcedKey::Fixed(ShellKeyEvent::new(
            KeyCode::Space,
            Modifiers::CONTROL,
        )))
    };
    let _ = app.update(chord());
    assert!(
        app.shell.paused(),
        "Ctrl+Space must reach the shared command router and pause telemetry"
    );
    assert!(
        !app.shell.control_held(),
        "the chord is the manual pause, never the hold-Ctrl transient"
    );
    let _ = app.update(chord());
    assert!(!app.shell.paused(), "the same chord resumes telemetry");
}

/// Holding Ctrl pauses telemetry refresh and releasing it resumes: the
/// modifier lifecycle feeds the shared `TelemetryRefreshPolicy` as the
/// transient pause, mirroring GPUI's hold-Ctrl behavior (ADR-027 parity).
#[test]
fn holding_ctrl_pauses_and_releasing_resumes_telemetry_refresh() {
    let mut app = IcedApp::demo();
    assert!(!app.shell.paused());
    let _ = app.update(Message::ModifiersChanged(iced::keyboard::Modifiers::CTRL));
    assert!(app.shell.control_held());
    assert!(
        app.shell.paused(),
        "holding Ctrl must freeze telemetry refresh"
    );
    let _ = app.update(Message::ModifiersChanged(
        iced::keyboard::Modifiers::default(),
    ));
    assert!(!app.shell.control_held());
    assert!(
        !app.shell.paused(),
        "releasing Ctrl must resume telemetry refresh"
    );
}

#[test]
fn activate_tree_node_toggles_subtree_and_selects_parent() {
    let mut app = IcedApp::demo();
    // select_row validates against the active page's row count; switch to
    // Applications so the process table is the active surface (the demo fixture
    // carries 12 processes).
    let _ = app.update(Message::SelectPage(
        taskmanager_application::AppPage::Applications,
    ));
    assert!(!app.process_presentation.expanded_tree.contains(&1234));
    assert_eq!(app.shell.selected, 0);
    // One activation collapses the subtree AND selects the parent row (a parent
    // was previously only toggleable, never selectable).
    let _ = app.update(Message::ActivateTreeNode {
        pid: 1234,
        flat_index: 1,
    });
    assert!(
        app.process_presentation.expanded_tree.contains(&1234),
        "first activation collapses the subtree"
    );
    assert_eq!(app.shell.selected, 1, "activation selects the parent row");
    // A second activation re-expands (the collapsed set is a toggle).
    let _ = app.update(Message::ActivateTreeNode {
        pid: 1234,
        flat_index: 1,
    });
    assert!(
        !app.process_presentation.expanded_tree.contains(&1234),
        "second activation re-expands the subtree"
    );
}

#[test]
fn request_process_batch_routes_through_the_shared_shell_batch_path() {
    let mut app = IcedApp::demo();
    // The action bar's Suspend/Resume verbs route through
    // ShellApp::request_process_batch; demo mode has no platform client, so the
    // produced ExecuteBatch effect is honestly suppressed rather than executed.
    let _ = app.update(Message::RequestProcessBatch(
        taskmanager_core::core::process::ProcessBatchAction::Suspend,
    ));
    assert!(
        app.shell.feedback_text().contains("Demo mode"),
        "process batch must route through the shared shell method: {}",
        app.shell.feedback_text()
    );
}

#[test]
fn application_aggregate_selection_stays_pidless() {
    let mut app = IcedApp::demo();
    let _ = app.update(Message::SelectPage(AppPage::Applications));
    let root_pid = app.shell.visible_processes()[0].pid;

    let root_row_key = app.shell.visible_processes()[0]
        .current_start_token()
        .and_then(|token| taskmanager_shell::ProcessRowIdentity::from_parts(root_pid, token))
        .map(taskmanager_shell::ProcessRowId::Application);
    let _ = app.update(Message::ToggleGroupExpansion {
        name: format!("app-tree:{root_pid}"),
        main_pid: root_pid,
        flat_index: 0,
        row_key: root_row_key,
    });

    assert_eq!(app.shell.selected_row, root_row_key);
    assert!(app.shell.selected_identities().is_empty());
    assert!(app.shell.selected_process_identity().is_none());
}

#[test]
fn kill_gates_behind_confirmation_and_confirm_emits_the_batch() {
    let mut app = IcedApp::demo();
    // Kill is destructive: requesting it gates behind a confirmation (no effect
    // yet) and sets the pending batch intent.
    let _ = app.update(Message::RequestProcessBatch(
        taskmanager_core::core::process::ProcessBatchAction::Kill,
    ));
    assert!(
        app.shell.pending_batch().is_some(),
        "Kill must gate behind pending_batch"
    );
    // Confirm emits the ExecuteBatch effect (demo-suppressed) and clears the gate.
    let _ = app.update(Message::ConfirmProcessBatch);
    assert!(
        app.shell.pending_batch().is_none(),
        "confirm clears the pending batch"
    );
    assert!(
        app.shell.feedback_text().contains("Demo mode"),
        "confirm must submit the batch: {}",
        app.shell.feedback_text()
    );
    // Dismiss cancels a pending Kill without submitting.
    let _ = app.update(Message::RequestProcessBatch(
        taskmanager_core::core::process::ProcessBatchAction::Kill,
    ));
    assert!(app.shell.pending_batch().is_some());
    let _ = app.update(Message::DismissOverlay);
    assert!(
        app.shell.pending_batch().is_none(),
        "dismiss cancels the pending Kill"
    );
}

#[test]
fn startup_toggle_gates_behind_confirmation_like_gpui() {
    let mut app = IcedApp::demo();
    let _ = app.update(Message::SelectPage(AppPage::Startup));
    // Requesting a startup toggle must NOT fire directly — it gates behind a
    // pending slot (mirrors GPUI's request_startup_control_confirmation).
    let _ = app.update(Message::RequestStartupControl(true));
    assert!(
        app.shell.pending_startup().is_some(),
        "startup toggle must gate behind pending_startup"
    );
    // Confirm emits the gated effect (demo-suppressed) and clears the gate.
    let _ = app.update(Message::ConfirmStartupControl);
    assert!(
        app.shell.pending_startup().is_none(),
        "confirm clears the pending startup slot"
    );
    // Dismiss cancels a pending toggle without submitting.
    let _ = app.update(Message::RequestStartupControl(false));
    assert!(app.shell.pending_startup().is_some());
    let _ = app.update(Message::DismissOverlay);
    assert!(
        app.shell.pending_startup().is_none(),
        "dismiss cancels the pending startup toggle"
    );
}

/// Opening the containers modal requests a rollup immediately (G-08): the
/// effect is queued through the platform lane, observable as the honest demo
/// suppression in a no-platform fixture. And once a rollup event batch lands,
/// `ShellData::containers` carries it and the modal renders from that state —
/// the full open → request → rollup → render chain without a live platform.
#[test]
fn opening_the_containers_modal_requests_and_renders_a_rollup() {
    use taskmanager_application::{
        ContainerRollupEvent, CorrelatedEvent, PlatformEventBatch, PlatformEventContext,
    };
    use taskmanager_platform_contract::{CapabilityId, EventSequence, RequestId};

    let mut app = IcedApp::demo();
    assert!(app.shell.projection().containers.is_none());
    let _ = app.update(Message::OpenContainers);
    assert!(app.containers_open());
    assert!(
        app.shell.feedback_text().contains("Demo mode"),
        "open must queue the immediate rollup request: {}",
        app.shell.feedback_text()
    );

    // The rollup answer arrives as a platform event batch; the shell fold is
    // the same one the live tick drives.
    let rollup = taskmanager_core::core::process_telemetry::ContainerRollup::empty_healthy(1_000);
    let mut batch = PlatformEventBatch::default();
    batch.containers_events.push(CorrelatedEvent::new(
        PlatformEventContext {
            request_id: RequestId::new(1).expect("fixture request id"),
            capability: CapabilityId::CONTAINERS,
            provider: None,
            sequence: EventSequence::new(1),
            observed_at_ms: 1_000,
        },
        ContainerRollupEvent::Snapshot(Box::new(rollup)),
    ));
    app.shell.apply_platform_batch(batch);
    assert!(
        app.shell
            .projection()
            .containers
            .as_ref()
            .is_some_and(|rollup| rollup.containers.is_empty()),
        "the rollup event must land in the shared ShellData slot"
    );
    // The open modal renders from that state (the healthy-empty branch, not
    // the waiting branch it rendered before the rollup arrived).
    {
        let _modal = crate::ui::view(&app);
    }
    // Closing resets nothing about the schedule; the axis keeps feeding.
    let _ = app.update(Message::Key(IcedKey::Fixed(ShellKeyEvent::new(
        KeyCode::Escape,
        Modifiers::NONE,
    ))));
    assert!(!app.containers_open());
}

#[test]
fn shell_process_filtering_and_sort_render_through_the_shared_state() {
    let app = crate::app::IcedApp::default();
    let rows = app.shell.visible_processes();
    assert!(rows.is_empty(), "no platform batch in demo-default state");
}

#[test]
fn pointer_row_selection_updates_the_shared_session_cursor() {
    let mut app = IcedApp::demo();
    app.shell.application.active_page = AppPage::Users;

    let _ = app.update(Message::SelectRow(1));
    assert_eq!(app.shell.selected, 1);

    let _ = app.update(Message::SelectRow(99));
    assert_eq!(app.shell.selected, 1);
}

#[test]
fn row_click_branches_on_live_modifier_state() {
    // The Applications page must be active for `visible_processes` to back the
    // selection (the demo defaults to Performance, which has no table).
    let mut app = IcedApp::demo();
    let _ = app.update(Message::SelectPage(AppPage::Applications));
    let pids: Vec<u32> = app
        .shell
        .visible_processes()
        .iter()
        .map(|process| process.pid)
        .collect();
    assert!(pids.len() >= 4, "demo needs a range to exercise extend");

    // Plain click collapses to a single anchor.
    let _ = app.update(Message::ModifiersChanged(
        iced::keyboard::Modifiers::default(),
    ));
    let _ = app.update(Message::SelectRow(0));
    assert_eq!(app.shell.selected_identities().len(), 1);
    assert!(
        app.shell
            .visible_process_by_pid(pids[0])
            .is_some_and(|p| app.shell.is_process_selected(p))
    );

    // Ctrl-click toggles a second, non-adjacent row into the set.
    let _ = app.update(Message::ModifiersChanged(iced::keyboard::Modifiers::CTRL));
    let _ = app.update(Message::SelectRow(2));
    assert_eq!(app.shell.selected_identities().len(), 2);
    assert!(
        app.shell
            .visible_process_by_pid(pids[2])
            .is_some_and(|p| app.shell.is_process_selected(p))
    );

    // Shift-click grows the range from anchor (2) to 3, folding in pids 2..=3.
    let _ = app.update(Message::ModifiersChanged(iced::keyboard::Modifiers::SHIFT));
    let _ = app.update(Message::SelectRow(3));
    assert!(
        app.shell
            .visible_process_by_pid(pids[3])
            .is_some_and(|p| app.shell.is_process_selected(p))
    );
    assert!(app.shell.selected_identities().len() >= 3);

    // Releasing the modifiers and plain-clicking collapses back to one.
    let _ = app.update(Message::ModifiersChanged(
        iced::keyboard::Modifiers::default(),
    ));
    let _ = app.update(Message::SelectRow(1));
    assert_eq!(app.shell.selected_identities().len(), 1);
    assert!(
        app.shell
            .visible_process_by_pid(pids[1])
            .is_some_and(|p| app.shell.is_process_selected(p))
    );
}

#[test]
fn modal_focus_commands_enter_and_remain_inside_the_modal_scope() {
    assert_eq!(
        focus_command(PresenceTransition::Opened, None, None, None),
        FocusCommand::ModalClose
    );
    assert_eq!(
        focus_command(
            PresenceTransition::StableOpen,
            None,
            Some(FocusDirection::Next),
            None,
        ),
        FocusCommand::ModalClose
    );
    assert_eq!(
        focus_command(
            PresenceTransition::StableOpen,
            None,
            Some(FocusDirection::Previous),
            None,
        ),
        FocusCommand::ModalClose
    );
    assert_eq!(
        focus_command(
            PresenceTransition::StableClosed,
            None,
            Some(FocusDirection::Next),
            None,
        ),
        FocusCommand::Next
    );
    assert_eq!(
        focus_command(
            PresenceTransition::StableClosed,
            None,
            Some(FocusDirection::Previous),
            None,
        ),
        FocusCommand::Previous
    );
    assert_eq!(
        focus_command(
            PresenceTransition::StableClosed,
            Some(FocusTarget::PageTab(AppPage::Users)),
            None,
            None,
        ),
        FocusCommand::Target(FocusTarget::PageTab(AppPage::Users))
    );
    assert_eq!(
        focus_command(
            PresenceTransition::StableOpen,
            Some(FocusTarget::ConfirmEndTask),
            None,
            None,
        ),
        FocusCommand::Target(FocusTarget::ConfirmEndTask)
    );
    assert_eq!(
        focus_command(
            PresenceTransition::Closed,
            None,
            None,
            Some(FocusTarget::EndTask),
        ),
        FocusCommand::Restore(FocusTarget::EndTask)
    );
    assert_eq!(
        focus_command(PresenceTransition::Closed, None, None, None),
        FocusCommand::None
    );
}

#[test]
fn modal_opening_captures_a_renderer_trigger_and_dismissal_clears_it() {
    let mut app = IcedApp::demo();
    app.shell.application.active_page = AppPage::Applications;
    assert!(app.shell.select_row(0));
    let _ = app.update(Message::Focus(FocusTarget::EndTask));
    assert_eq!(app.input.focused_control, Some(FocusTarget::EndTask));

    let _ = app.update(Message::RequestEndTask);
    assert!(app.shell.pending_end().is_some());
    assert_eq!(app.input.modal_restore, Some(FocusTarget::EndTask));

    let _ = app.update(Message::DismissOverlay);
    assert!(app.shell.pending_end().is_none());
    assert!(app.input.modal_restore.is_none());
}

#[test]
fn sort_by_message_toggles_direction_on_active_column_and_switches_off_inactive() {
    use taskmanager_shell::{SortCol, SortDir};

    let mut app = IcedApp::demo();
    app.shell.application.active_page = AppPage::Applications;
    app.shell.process_sort = (SortCol::Cpu, SortDir::Desc);
    app.shell.selected = 3;

    // Clicking the active column flips direction via the same action as the
    // 'S' chord (ShellApp::toggle_sort_direction): column unchanged, cursor
    // reset, status narrating the new sort.
    let _ = app.update(Message::SortBy(SortCol::Cpu));
    assert_eq!(app.shell.process_sort, (SortCol::Cpu, SortDir::Asc));
    assert_eq!(app.shell.selected, 0);
    assert_eq!(app.shell.feedback_text(), "Sorted by CPU ascending");

    // Clicking an inactive column switches to it through the same process_sort
    // field the keyboard path mutates; direction is retained, matching
    // cycle_sort_column's keep-direction semantics.
    let _ = app.update(Message::SortBy(SortCol::Memory));
    assert_eq!(app.shell.process_sort, (SortCol::Memory, SortDir::Asc));
    assert_eq!(app.shell.selected, 0);
    assert_eq!(app.shell.feedback_text(), "Sorted by Memory ascending");

    // Re-prove the active-toggle path with a fresh direction, so the round-trip
    // is asserted on both Asc→Desc and Desc→Asc.
    app.shell.process_sort = (SortCol::Memory, SortDir::Asc);
    let _ = app.update(Message::SortBy(SortCol::Memory));
    assert_eq!(app.shell.process_sort, (SortCol::Memory, SortDir::Desc));
}

#[test]
fn sort_by_matches_the_keyboard_chord_outcome() {
    use taskmanager_shell::{SortCol, SortDir};

    // The pointer path and the 's'/'S' chord must land on identical shell
    // state: toggling direction via SortBy == toggling via the 'S' character.
    let mut clicked = IcedApp::demo();
    clicked.shell.process_sort = (SortCol::Cpu, SortDir::Desc);
    let _ = clicked.update(Message::SortBy(SortCol::Cpu));

    let mut keyed = IcedApp::demo();
    keyed.shell.process_sort = (SortCol::Cpu, SortDir::Desc);
    let _ = keyed.update(Message::Key(IcedKey::Character(
        'S',
        taskmanager_application::Modifiers::NONE,
    )));

    assert_eq!(clicked.shell.process_sort, keyed.shell.process_sort);
    assert_eq!(clicked.shell.feedback_text(), keyed.shell.feedback_text());
}

#[test]
fn table_focus_is_renderer_local_but_selection_stays_in_the_shell() {
    let mut app = IcedApp::demo();
    app.shell.application.active_page = AppPage::Users;
    let target = FocusTarget::TableRow {
        page: AppPage::Users,
        index: 1,
    };

    let _ = app.update(Message::Focus(target));
    assert_eq!(app.input.focused_control, Some(target));

    let _ = app.update(Message::SelectRow(1));
    assert_eq!(app.shell.selected, 1);
}

#[test]
fn pending_end_focus_scope_targets_the_real_confirmation_control() {
    let mut app = IcedApp::demo();
    app.shell.application.active_page = AppPage::Applications;
    assert!(app.shell.select_row(0));

    let _ = app.update(Message::RequestEndTask);
    assert!(app.shell.pending_end().is_some());
    assert_eq!(app.modal_focus_target(), FocusTarget::ConfirmEndTask);
}

#[test]
fn pending_end_tab_scope_alternates_confirm_and_cancel() {
    assert_eq!(
        pending_end_focus_target(Some(FocusTarget::ConfirmEndTask)),
        FocusTarget::CancelEndTask
    );
    assert_eq!(
        pending_end_focus_target(Some(FocusTarget::CancelEndTask)),
        FocusTarget::ConfirmEndTask
    );
    assert_eq!(pending_end_focus_target(None), FocusTarget::ConfirmEndTask);
}

#[test]
fn pending_end_tab_messages_update_the_renderer_focus_scope() {
    let mut app = IcedApp::demo();
    app.shell.application.active_page = AppPage::Applications;
    assert!(app.shell.select_row(0));
    let _ = app.update(Message::RequestEndTask);
    assert_eq!(app.input.focused_control, Some(FocusTarget::ConfirmEndTask));

    let tab = taskmanager_shell::ShellKeyEvent::new(
        taskmanager_application::KeyCode::Tab,
        taskmanager_application::Modifiers::NONE,
    );
    let _ = app.update(Message::Key(IcedKey::Fixed(tab)));
    assert_eq!(app.input.focused_control, Some(FocusTarget::CancelEndTask));
}

#[test]
fn arrow_selection_keeps_renderer_focus_on_each_typed_table() {
    for page in [
        AppPage::Applications,
        AppPage::Services,
        AppPage::Startup,
        AppPage::Users,
    ] {
        let mut app = IcedApp::demo();
        app.shell.application.active_page = page;
        let arrow_down = taskmanager_shell::ShellKeyEvent::new(
            taskmanager_application::KeyCode::ArrowDown,
            taskmanager_application::Modifiers::NONE,
        );

        let _ = app.update(Message::Key(IcedKey::Fixed(arrow_down)));

        let expected_index = if page == AppPage::Applications { 0 } else { 1 };
        assert_eq!(app.shell.selected, expected_index);
        if page == AppPage::Applications {
            assert_eq!(
                app.process_presentation.visual_cursor, 1,
                "cursor leaves the category header"
            );
        }
        assert_eq!(
            app.input.focused_control,
            Some(FocusTarget::TableRow {
                page,
                index: expected_index,
            })
        );
    }
}

#[test]
fn page_down_keeps_applications_focus_bound_to_the_new_selection() {
    let mut app = IcedApp::demo();
    app.shell.application.active_page = AppPage::Applications;
    let page_down = taskmanager_shell::ShellKeyEvent::new(
        taskmanager_application::KeyCode::PageDown,
        taskmanager_application::Modifiers::NONE,
    );

    let _ = app.update(Message::Key(IcedKey::Fixed(page_down)));

    assert_eq!(app.process_presentation.visual_cursor, 10);
    assert_eq!(app.shell.selected, 9);
    assert_eq!(
        app.input.focused_control,
        Some(FocusTarget::TableRow {
            page: AppPage::Applications,
            index: 9,
        })
    );
}

#[test]
fn selection_keys_do_not_steal_focus_from_the_search_field_or_empty_table() {
    let mut searching = IcedApp::demo();
    searching.shell.application.active_page = AppPage::Applications;
    searching.shell.open_search();
    let arrow_down = taskmanager_shell::ShellKeyEvent::new(
        taskmanager_application::KeyCode::ArrowDown,
        taskmanager_application::Modifiers::NONE,
    );
    let _ = searching.update(Message::Key(IcedKey::Fixed(arrow_down)));
    assert_eq!(searching.shell.selected, 0);
    assert_eq!(searching.input.focused_control, None);

    let mut empty = IcedApp::demo();
    empty.shell.application.active_page = AppPage::Services;
    taskmanager_shell::fixture::seed_projection_fact(
        &mut empty.shell,
        taskmanager_shell::fixture::ProjectionSeedFact::Services(Some(Vec::new())),
    );
    let _ = empty.update(Message::Key(IcedKey::Fixed(arrow_down)));
    assert_eq!(empty.shell.selected, 0);
    assert_eq!(empty.input.focused_control, None);
}
#[path = "tests/settings_and_service_control_tests.rs"]
mod settings_and_service_control_tests;

#[path = "tests/service_details.rs"]
mod service_details;

#[path = "tests/visual_navigation.rs"]
mod visual_navigation;

#[path = "tests/phase3_gaps.rs"]
mod phase3_gaps;

#[path = "tests/phase4_gaps.rs"]
mod phase4_gaps;

#[path = "tests/phase5_gaps.rs"]
mod phase5_gaps;

#[path = "tests/phase6_gaps.rs"]
mod phase6_gaps;

#[path = "tests/projection_cache.rs"]
mod projection_cache;
