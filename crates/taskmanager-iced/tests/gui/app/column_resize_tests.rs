// test-intent: behavior
//! Column-sizing behavior for the Applications process table: width overrides
//! replace contract defaults (never on the identity column), hostile widths
//! clamp or are ignored, drag sessions derive width from pointer deltas and
//! close on release, the keyboard/menu stepper path publishes the same
//! transition in fixed steps, and the override set round-trips through the
//! persisted `process_col_widths` config token (unknown tokens and hostile
//! values degrade, never panic). The mouse event sequence itself (edge press
//! → raw cursor moves → release) is not headless-drivable in iced; these
//! tests exercise the pure state transitions the subscription feeds.

use std::collections::HashSet;
use std::time::Instant;

use iced::Point;
use taskmanager_application::ConfigStore;
use taskmanager_core::core::config::ColumnWidthConfig;

use taskmanager_shell::SortCol;

use super::{
    ColumnWidthOverrides, MAX_PROCESS_COLUMN_WIDTH, MIN_PROCESS_COLUMN_WIDTH,
    PROCESS_COLUMN_STEPPER_COALESCE, ProcessColumnSizing, StepperPersistGate, stepper_commit_now,
    stepper_flush_due,
};
use crate::IcedApp;
use crate::app::{Message, SettingsChange, keyboard_resize_width};
use crate::test_support::temp_dir;
use crate::ui::applications::{
    apps_table_width, apps_table_width_with, column_resizable, column_width, visible_apps_columns,
    visible_apps_columns_with,
};

/// A stored override replaces the contract default for that column only; the
/// identity column (`Name`) mirrors the contract's identity rule and never
/// accepts a resize.
#[test]
fn overrides_replace_defaults_and_skip_the_identity_column() {
    let mut app = IcedApp::demo();
    let _ = app.update(Message::ResizeProcessColumn {
        column: SortCol::Cpu,
        width: 133.0,
    });
    assert_eq!(app.process_column_width(SortCol::Cpu), 133.0);
    // Untouched columns keep contract truth.
    assert_eq!(
        app.process_column_width(SortCol::Memory),
        column_width(SortCol::Memory)
    );
    // The identity column ignores both the direct set and the drag session.
    let _ = app.update(Message::ResizeProcessColumn {
        column: SortCol::Name,
        width: 300.0,
    });
    assert_eq!(
        app.process_column_width(SortCol::Name),
        column_width(SortCol::Name)
    );
    assert!(
        app.process_column_sizing
            .overrides
            .get(SortCol::Name)
            .is_none()
    );
    assert!(!column_resizable(SortCol::Name));
    assert!(column_resizable(SortCol::Cpu));
    assert!(column_resizable(SortCol::Pss));
}

/// Stored widths round to whole pixels and clamp into the sizing domain;
/// non-finite input is ignored outright instead of reaching layout code.
#[test]
fn hostile_widths_clamp_or_are_ignored() {
    let mut sizing = ProcessColumnSizing::default();
    sizing.overrides.set(SortCol::Cpu, 5.0);
    assert_eq!(
        sizing.overrides.get(SortCol::Cpu),
        Some(MIN_PROCESS_COLUMN_WIDTH)
    );
    sizing.overrides.set(SortCol::Cpu, 10_000.0);
    assert_eq!(
        sizing.overrides.get(SortCol::Cpu),
        Some(MAX_PROCESS_COLUMN_WIDTH)
    );
    sizing.overrides.set(SortCol::Cpu, 133.7);
    assert_eq!(sizing.overrides.get(SortCol::Cpu), Some(134.0));
    sizing.overrides.set(SortCol::Cpu, f32::NAN);
    sizing.overrides.set(SortCol::Cpu, f32::INFINITY);
    sizing.overrides.set(SortCol::Cpu, f32::NEG_INFINITY);
    assert_eq!(
        sizing.overrides.get(SortCol::Cpu),
        Some(134.0),
        "non-finite input must be ignored, not clamped or cleared"
    );
}

/// A drag session anchors on the first tracked pointer motion, then stores
/// `start_width + dx` clamped; release closes the session so later motion is
/// inert; motion without a session does nothing.
#[test]
fn drag_session_derives_width_from_pointer_deltas() {
    let mut app = IcedApp::demo();
    let _ = app.update(Message::ProcessColumnDragMoved(Point::new(500.0, 8.0)));
    assert!(
        app.process_column_sizing.drag.is_none(),
        "motion without an open session must be inert"
    );

    let _ = app.update(Message::BeginProcessColumnDrag {
        column: SortCol::Memory,
        start_width: 100.0,
    });
    // First tracked motion anchors the origin; width is unchanged.
    let _ = app.update(Message::ProcessColumnDragMoved(Point::new(300.0, 5.0)));
    assert_eq!(app.process_column_width(SortCol::Memory), 100.0);
    assert_eq!(
        app.process_column_sizing.drag.unwrap().origin_x(),
        Some(300.0)
    );

    // +65px of horizontal motion widens the column by exactly 65px.
    let _ = app.update(Message::ProcessColumnDragMoved(Point::new(365.0, 40.0)));
    assert_eq!(app.process_column_width(SortCol::Memory), 165.0);

    // A hostile leftward delta clamps at the domain floor, not below it.
    let _ = app.update(Message::ProcessColumnDragMoved(Point::new(-5_000.0, 40.0)));
    assert_eq!(
        app.process_column_width(SortCol::Memory),
        MIN_PROCESS_COLUMN_WIDTH
    );

    // A non-finite position is ignored; the width survives.
    let _ = app.update(Message::ProcessColumnDragMoved(Point::new(f32::NAN, 0.0)));
    assert_eq!(
        app.process_column_width(SortCol::Memory),
        MIN_PROCESS_COLUMN_WIDTH
    );

    // Release closes the session; subsequent motion no longer resizes.
    let _ = app.update(Message::ProcessColumnDragReleased);
    assert!(app.process_column_sizing.drag.is_none());
    let _ = app.update(Message::ProcessColumnDragMoved(Point::new(900.0, 5.0)));
    assert_eq!(
        app.process_column_width(SortCol::Memory),
        MIN_PROCESS_COLUMN_WIDTH
    );
}

/// A drag cannot be opened on the identity column, and a hostile start width
/// never seeds a session.
#[test]
fn drag_begin_is_gated_by_resizability_and_finite_start() {
    let mut app = IcedApp::demo();
    let _ = app.update(Message::BeginProcessColumnDrag {
        column: SortCol::Name,
        start_width: 120.0,
    });
    let _ = app.update(Message::ProcessColumnDragMoved(Point::new(300.0, 5.0)));
    let _ = app.update(Message::ProcessColumnDragMoved(Point::new(400.0, 5.0)));
    assert!(
        app.process_column_sizing.drag.is_none(),
        "the identity column never opens a drag session"
    );
    assert!(
        app.process_column_sizing
            .overrides
            .get(SortCol::Name)
            .is_none()
    );

    let _ = app.update(Message::BeginProcessColumnDrag {
        column: SortCol::Cpu,
        start_width: f32::NAN,
    });
    assert!(
        app.process_column_sizing.drag.is_none(),
        "a non-finite start width cannot seed a session"
    );
}

/// Without overrides the geometry seams are byte-identical to the historical
/// contract-derived values, and one override moves the table extent by
/// exactly its delta (header, body and scroll extent share the one source).
#[test]
fn table_geometry_seams_are_unchanged_without_overrides() {
    let hidden: HashSet<SortCol> = HashSet::new();
    let empty = ColumnWidthOverrides::default();

    assert_eq!(
        apps_table_width(true, &hidden),
        apps_table_width_with(true, &hidden, &empty)
    );
    assert_eq!(
        visible_apps_columns(true, &hidden),
        visible_apps_columns_with(true, &hidden, &empty)
    );

    let base = apps_table_width_with(true, &hidden, &empty);
    let mut widths = ColumnWidthOverrides::default();
    widths.set(SortCol::Cpu, column_width(SortCol::Cpu) + 30.0);
    assert_eq!(
        apps_table_width_with(true, &hidden, &widths),
        base + 30.0,
        "one +30px override must widen the table extent by exactly 30px"
    );
    // Hidden columns' overrides are excluded from the extent like the columns
    // themselves: a State override must not change the State-hidden extent.
    widths.set(SortCol::State, column_width(SortCol::State) + 50.0);
    let with_state_hidden: HashSet<SortCol> = HashSet::from([SortCol::State]);
    assert_eq!(
        apps_table_width_with(true, &with_state_hidden, &widths),
        apps_table_width_with(true, &with_state_hidden, &empty) + 30.0,
        "only the visible Cpu override may move the State-hidden extent"
    );
}

/// The keyboard/menu stepper path advances a column by exactly one 16px step
/// per activation, saturates at both clamp edges instead of erroring, and
/// never moves the identity column.
#[test]
fn keyboard_steppers_step_clamp_and_skip_the_identity_column() {
    let mut app = IcedApp::demo();

    // Two widen steps move the column by exactly 32px over its default.
    let mut current = app.process_column_width(SortCol::Cpu);
    for _ in 0..2 {
        current = keyboard_resize_width(current, true);
    }
    let _ = app.update(Message::ResizeProcessColumn {
        column: SortCol::Cpu,
        width: current,
    });
    assert_eq!(
        app.process_column_width(SortCol::Cpu),
        column_width(SortCol::Cpu) + 32.0
    );

    // Stepping past the ceiling saturates at the domain maximum.
    let mut wide = MAX_PROCESS_COLUMN_WIDTH - 8.0;
    for _ in 0..4 {
        wide = keyboard_resize_width(wide, true);
    }
    let _ = app.update(Message::ResizeProcessColumn {
        column: SortCol::Cpu,
        width: wide,
    });
    assert_eq!(
        app.process_column_width(SortCol::Cpu),
        MAX_PROCESS_COLUMN_WIDTH,
        "steps past the ceiling must saturate, not overshoot"
    );

    // Stepping below the floor saturates at the domain minimum.
    let mut narrow = MIN_PROCESS_COLUMN_WIDTH + 8.0;
    for _ in 0..4 {
        narrow = keyboard_resize_width(narrow, false);
    }
    let _ = app.update(Message::ResizeProcessColumn {
        column: SortCol::Cpu,
        width: narrow,
    });
    assert_eq!(
        app.process_column_width(SortCol::Cpu),
        MIN_PROCESS_COLUMN_WIDTH,
        "steps past the floor must saturate, not undershoot"
    );

    // The identity column ignores stepper transitions exactly like drags.
    let _ = app.update(Message::ResizeProcessColumn {
        column: SortCol::Name,
        width: keyboard_resize_width(column_width(SortCol::Name), true),
    });
    assert_eq!(
        app.process_column_width(SortCol::Name),
        column_width(SortCol::Name)
    );
    assert!(
        app.process_column_sizing
            .overrides
            .get(SortCol::Name)
            .is_none()
    );
}

/// The override set round-trips through the persisted config token: the
/// contract-id spelling, whole-pixel values, token-sorted output, and
/// graceful degradation for unknown tokens, the identity column,
/// non-finite widths, duplicates, and out-of-domain values.
#[test]
fn overrides_round_trip_through_the_config_token() {
    let mut widths = ColumnWidthOverrides::default();
    widths.set(SortCol::Cpu, 133.7); // rounds to 134
    widths.set(SortCol::Pss, 90.0); // shell-superset column persists too
    widths.set(SortCol::Memory, 40.0);
    let token = widths.to_config();
    assert_eq!(
        token
            .iter()
            .map(|entry| entry.column.as_str())
            .collect::<Vec<_>>(),
        ["CPU", "Memory", "PSS"],
        "entries serialize in token order so unchanged layouts are byte-stable"
    );
    assert!(token.iter().all(|entry| entry.width.fract() == 0.0));
    assert_eq!(
        ColumnWidthOverrides::from_config(&token),
        widths,
        "a clean token must restore the exact override set"
    );

    // Hostile entries degrade individually instead of failing the load.
    let hostile = vec![
        ColumnWidthConfig {
            column: "Watts".to_string(),
            width: 300.0,
        },
        ColumnWidthConfig {
            column: "Name".to_string(),
            width: 300.0,
        },
        ColumnWidthConfig {
            column: "FDs".to_string(),
            width: f32::NAN,
        },
        ColumnWidthConfig {
            column: "Swap".to_string(),
            width: -5.0,
        },
        ColumnWidthConfig {
            column: "Nice".to_string(),
            width: 10_000.0,
        },
        ColumnWidthConfig {
            column: "User".to_string(),
            width: 50.0,
        },
        ColumnWidthConfig {
            column: "User".to_string(),
            width: 60.0,
        },
    ];
    let parsed = ColumnWidthOverrides::from_config(&hostile);
    assert!(
        parsed.get(SortCol::Cpu).is_none(),
        "untouched columns stay untouched"
    );
    assert!(
        parsed.get(SortCol::Name).is_none(),
        "the identity column never accepts a persisted width"
    );
    assert!(
        parsed.get(SortCol::Fds).is_none(),
        "a non-finite width drops, not clamps"
    );
    assert_eq!(
        parsed.get(SortCol::Swap),
        Some(MIN_PROCESS_COLUMN_WIDTH),
        "a below-floor value clamps up into the domain"
    );
    assert_eq!(
        parsed.get(SortCol::Nice),
        Some(MAX_PROCESS_COLUMN_WIDTH),
        "an oversized value clamps down into the domain"
    );
    assert_eq!(
        parsed.get(SortCol::User),
        Some(50.0),
        "the first occurrence wins on a duplicate token"
    );
    assert_eq!(
        ColumnWidthOverrides::from_config(&[]),
        ColumnWidthOverrides::default(),
        "an empty token (first launch, old config) restores no overrides"
    );
}

/// The resize transition and the drag release commit the override set into
/// the configuration draft's `process_col_widths` token; live drag motion
/// alone (no release) and a press-and-release without motion commit nothing
/// new.
#[test]
fn resize_and_release_commit_the_config_token() {
    let mut app = IcedApp::demo();
    let _ = app.update(Message::ResizeProcessColumn {
        column: SortCol::Cpu,
        width: 200.0,
    });
    let token = &app.config_draft().process_col_widths;
    assert_eq!(token.len(), 1);
    assert_eq!(token[0].column, "CPU");
    assert_eq!(token[0].width, 200.0);

    // A full drag session persists exactly its final width on release.
    let _ = app.update(Message::BeginProcessColumnDrag {
        column: SortCol::Memory,
        start_width: 100.0,
    });
    let _ = app.update(Message::ProcessColumnDragMoved(Point::new(300.0, 5.0)));
    let _ = app.update(Message::ProcessColumnDragMoved(Point::new(365.0, 5.0)));
    assert_eq!(
        app.config_draft().process_col_widths.len(),
        1,
        "live drag motion alone must not commit"
    );
    let _ = app.update(Message::ProcessColumnDragReleased);
    let token = &app.config_draft().process_col_widths;
    assert_eq!(token.len(), 2, "release is the drag's single commit point");
    assert!(
        token
            .iter()
            .any(|entry| entry.column == "Memory" && entry.width == 165.0)
    );

    // A press-and-release without motion changes nothing: the draft stays
    // byte-identical instead of growing a default-width entry.
    let before = app.config_draft().process_col_widths.clone();
    let _ = app.update(Message::BeginProcessColumnDrag {
        column: SortCol::Nice,
        start_width: 60.0,
    });
    let _ = app.update(Message::ProcessColumnDragReleased);
    assert_eq!(
        app.config_draft().process_col_widths,
        before,
        "an unchanged layout must not re-commit"
    );
}

/// The persisted token survives a restart through the real configuration
/// channel: the stepper/drag write reaches disk, a fresh launch restores the
/// widths, later (non-startup) publications never clobber the live session,
/// and a hostile config file degrades without a panic.
#[test]
fn persisted_widths_round_trip_a_restart_and_survive_later_publications() {
    let dir = temp_dir("column-widths-persistence");
    let path = dir.join("config.json");
    let mut app = IcedApp::with_config_store(None, ConfigStore::new(&path));

    let _ = app.update(Message::ResizeProcessColumn {
        column: SortCol::Cpu,
        width: 200.0,
    });
    app.wait_for_config_where(|config| {
        config
            .process_col_widths
            .iter()
            .any(|entry| entry.column == "CPU" && entry.width == 200.0)
    });
    let on_disk = ConfigStore::new(&path).load_or_default();
    assert!(
        on_disk
            .process_col_widths
            .iter()
            .any(|entry| entry.column == "CPU" && entry.width == 200.0),
        "token write-through to disk"
    );

    // A fresh launch restores the persisted width through the startup snapshot.
    let mut reloaded = IcedApp::with_config_store(None, ConfigStore::new(&path));
    assert_eq!(reloaded.process_column_width(SortCol::Cpu), 200.0);

    // Later publications (any settings echo) apply non-startup snapshots and
    // must leave the live override set alone.
    let _ = reloaded.update(Message::SettingsChanged(SettingsChange::HighContrast(true)));
    reloaded.wait_for_config_where(|config| config.hc);
    assert_eq!(
        reloaded.process_column_width(SortCol::Cpu),
        200.0,
        "a settings publication must not clobber live column widths"
    );

    // A hostile hand-edited file degrades per entry: unknown token dropped,
    // identity column dropped, out-of-domain values clamped into the domain.
    let hostile_store = ConfigStore::new(&path);
    let mut config = hostile_store.load_or_default();
    config.process_col_widths = vec![
        ColumnWidthConfig {
            column: "Watts".to_string(),
            width: 300.0,
        },
        ColumnWidthConfig {
            column: "Name".to_string(),
            width: 300.0,
        },
        ColumnWidthConfig {
            column: "Memory".to_string(),
            width: 3.0,
        },
        ColumnWidthConfig {
            column: "PID".to_string(),
            width: 5_000.0,
        },
    ];
    hostile_store.save(&config).unwrap();
    let hostile = IcedApp::with_config_store(None, ConfigStore::new(&path));
    assert_eq!(
        hostile.process_column_width(SortCol::Memory),
        MIN_PROCESS_COLUMN_WIDTH
    );
    assert_eq!(
        hostile.process_column_width(SortCol::Pid),
        MAX_PROCESS_COLUMN_WIDTH
    );
    assert_eq!(
        hostile.process_column_width(SortCol::Name),
        column_width(SortCol::Name),
        "the identity column keeps its contract width"
    );

    drop(hostile);
    drop(reloaded);
    drop(app);
    let _ = std::fs::remove_dir_all(dir);
}

/// The column menu's reset restores the whole default layout: every override
/// clears, columns fall back to contract widths, and the persisted token
/// empties (the documented recovery path for a stored width).
#[test]
fn reset_clears_overrides_and_empties_the_persisted_token() {
    let mut app = IcedApp::demo();
    let _ = app.update(Message::ResizeProcessColumn {
        column: SortCol::Cpu,
        width: 200.0,
    });
    let _ = app.update(Message::BeginProcessColumnDrag {
        column: SortCol::Memory,
        start_width: 100.0,
    });
    let _ = app.update(Message::ProcessColumnDragMoved(Point::new(10.0, 5.0)));
    let _ = app.update(Message::ProcessColumnDragMoved(Point::new(70.0, 5.0)));
    let _ = app.update(Message::ProcessColumnDragReleased);
    assert!(!app.config_draft().process_col_widths.is_empty());

    let _ = app.update(Message::ResetProcessColumns);
    assert_eq!(
        app.process_column_width(SortCol::Cpu),
        column_width(SortCol::Cpu)
    );
    assert_eq!(
        app.process_column_width(SortCol::Memory),
        column_width(SortCol::Memory)
    );
    assert!(
        app.config_draft().process_col_widths.is_empty(),
        "reset empties the persisted width token"
    );
}

/// The stepper persistence gate's window rule: an isolated activation (no
/// previous commit) commits straight through; one inside the coalescing
/// window defers; at or past the window a deferred commit is due; and a
/// gate with nothing deferred never flushes.
#[test]
fn stepper_gate_window_rule() {
    let now = Instant::now();
    assert!(
        stepper_commit_now(None, now),
        "no previous commit: isolated activation commits"
    );
    let committed_at = now;
    assert!(
        !stepper_commit_now(Some(committed_at), now),
        "a repeat inside the window defers"
    );
    let past_window = committed_at + PROCESS_COLUMN_STEPPER_COALESCE;
    assert!(
        stepper_commit_now(Some(committed_at), past_window),
        "at the window edge the gate reopens"
    );
    assert!(
        stepper_commit_now(
            Some(committed_at),
            committed_at + PROCESS_COLUMN_STEPPER_COALESCE * 2
        ),
        "well past the window the gate reopens"
    );

    let deferred = StepperPersistGate {
        last_commit: Some(committed_at),
        pending: true,
    };
    assert!(!stepper_flush_due(deferred, now), "window not yet elapsed");
    assert!(
        stepper_flush_due(deferred, past_window),
        "pending commit flushes once the window elapsed"
    );
    let idle = StepperPersistGate {
        last_commit: Some(committed_at),
        pending: false,
    };
    assert!(
        !stepper_flush_due(idle, past_window),
        "nothing deferred: the flush point is a no-op"
    );
}

/// Keyboard auto-repeat must not flood the configuration worker: the first
/// activation commits immediately, a repeat inside the coalescing window
/// updates the live override but defers the commit, and the poll-tick flush
/// lands the deferred width once the window has elapsed.
#[test]
fn stepper_auto_repeat_defers_the_commit_to_the_flush_point() {
    let mut app = IcedApp::demo();
    // Isolated activation: straight-through commit, as before the debounce.
    let _ = app.update(Message::ResizeProcessColumn {
        column: SortCol::Cpu,
        width: 116.0,
    });
    assert_eq!(app.config_draft().process_col_widths[0].width, 116.0);
    assert!(!app.process_column_sizing.stepper_gate.pending);

    // Auto-repeat arrives microseconds later (well inside the window): the
    // override follows every repeat, but the commit defers.
    let _ = app.update(Message::ResizeProcessColumn {
        column: SortCol::Cpu,
        width: 132.0,
    });
    assert_eq!(
        app.process_column_width(SortCol::Cpu),
        132.0,
        "the live override follows every repeat"
    );
    assert_eq!(
        app.config_draft().process_col_widths[0].width,
        116.0,
        "the repeat's commit defers inside the window"
    );
    assert!(app.process_column_sizing.stepper_gate.pending);

    // A flush point inside the window stays a no-op.
    app.poll_process_column_persist();
    assert_eq!(
        app.config_draft().process_col_widths[0].width,
        116.0,
        "the window has not elapsed: no flush"
    );

    // Once the window has elapsed (aged past it, as the 100 ms poll tick
    // would observe it), the flush lands the final width exactly once.
    app.process_column_sizing.stepper_gate.last_commit =
        Some(Instant::now() - PROCESS_COLUMN_STEPPER_COALESCE);
    app.poll_process_column_persist();
    assert_eq!(
        app.config_draft().process_col_widths[0].width,
        132.0,
        "the deferred commit lands after the window"
    );
    assert!(!app.process_column_sizing.stepper_gate.pending);

    // A drag release subsumes any pending stepper commit and reopens the
    // gate: the next isolated activation commits straight through again.
    let _ = app.update(Message::ResizeProcessColumn {
        column: SortCol::Cpu,
        width: 148.0,
    });
    assert!(app.process_column_sizing.stepper_gate.pending);
    let _ = app.update(Message::BeginProcessColumnDrag {
        column: SortCol::Memory,
        start_width: 100.0,
    });
    let _ = app.update(Message::ProcessColumnDragMoved(Point::new(10.0, 5.0)));
    let _ = app.update(Message::ProcessColumnDragReleased);
    let _ = app.update(Message::ResizeProcessColumn {
        column: SortCol::Cpu,
        width: 164.0,
    });
    assert_eq!(
        app.config_draft().process_col_widths[0].width,
        164.0,
        "after a direct persist the next isolated activation commits through"
    );
    assert!(!app.process_column_sizing.stepper_gate.pending);
}
