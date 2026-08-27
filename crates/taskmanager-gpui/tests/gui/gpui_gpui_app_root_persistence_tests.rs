use super::super::startup::FONT_TOKEN_SYSTEM;
use super::*;
use std::collections::HashSet;

use gpui::{AppContext, SharedString, TestAppContext};

use crate::core::config::{
    COLOR_SCHEME_DARK, STARTUP_PAGE_PROCESSES, SidebarDeviceOverrideConfig, TEXT_RENDERING_SUBPIXEL,
};
use crate::gpui_app::dashboard::SavedViewPreset;
use crate::gpui_app::dashboard::saved_view_transfer::filter_from_token;
use crate::gpui_app::processes_view::{ProcessStatusFilter, SortCol};
use crate::gpui_app::root::ProcessesState;
use crate::gpui_app::theme::{Skin, Theme};

#[gpui::test]
fn config_projection_persists_window_tokens_and_normalizes_graph_points(cx: &mut TestAppContext) {
    let root = cx.new(|cx| RootView::new(Theme::dark(), cx));
    let config = root.update(cx, |view, _cx| {
        let mut presentation = view.presentation_snapshot();
        presentation.appearance.color_scheme = COLOR_SCHEME_DARK;
        presentation.startup_page = SharedString::from(STARTUP_PAGE_PROCESSES);
        presentation.appearance.text_rendering = TEXT_RENDERING_SUBPIXEL;
        presentation.graphs.data_points = u32::MAX;
        view.replace_presentation(presentation);
        config_from_view(view)
    });

    assert_eq!(config.mode, COLOR_SCHEME_DARK);
    assert_eq!(config.startup_page, STARTUP_PAGE_PROCESSES);
    assert_eq!(
        config.text_rendering,
        crate::core::config::TEXT_RENDERING_PLATFORM_DEFAULT
    );
    assert_eq!(config.graph_data_points, 600);
}

#[gpui::test]
fn config_projection_echoes_the_history_persistence_opt_in(cx: &mut TestAppContext) {
    // The visible Settings toggle and persistence projection share one typed
    // runtime preference authority.
    let root = cx.new(|cx| RootView::new(Theme::dark(), cx));
    let (default, echoed) = root.update(cx, |view, _cx| {
        let default = config_from_view(view);
        view.history_runtime.request(true);
        (default, config_from_view(view))
    });
    assert!(!default.history_persistence);
    assert!(echoed.history_persistence);
}

#[gpui::test]
fn config_projection_persists_explicit_language_choice(cx: &mut TestAppContext) {
    let root = cx.new(|cx| RootView::new(Theme::dark(), cx));
    let config = root.update(cx, |view, _cx| {
        let mut presentation = view.presentation_snapshot();
        presentation.appearance.language = Some(crate::i18n::Language::Zh);
        view.replace_presentation(presentation);
        config_from_view(view)
    });

    assert_eq!(config.language.as_deref(), Some("zh"));
}

#[gpui::test]
fn config_projection_keeps_auto_skin_following_native_appearance(cx: &mut TestAppContext) {
    let root = cx.new(|cx| RootView::new(Theme::dark(), cx));
    let (automatic, explicit) = root.update(cx, |view, _cx| {
        view.theme.set_skin(Skin::Windows);
        let automatic = config_from_view(view);
        let mut presentation = view.presentation_snapshot();
        presentation.appearance.skin = Some(Skin::Windows);
        view.replace_presentation(presentation);
        (automatic, config_from_view(view))
    });
    assert!(automatic.skin.is_empty());
    assert_eq!(explicit.skin, "Windows");
}

#[gpui::test]
fn config_projection_round_trips_sidebar_order_and_device_overrides(cx: &mut TestAppContext) {
    let root = cx.new(|cx| RootView::new(Theme::dark(), cx));
    let config = root.update(cx, |view, _cx| {
        let mut presentation = view.presentation_snapshot();
        presentation.sidebar.order = vec![
            "disk:disk:wwid:stable".into(),
            "cpu".into(),
            "future:device".into(),
        ];
        presentation.sidebar.device_overrides = vec![SidebarDeviceOverrideConfig {
            device: "disk:disk:wwid:stable".into(),
            visible: false,
        }];
        view.replace_presentation(presentation);
        config_from_view(view)
    });

    let json = serde_json::to_string_pretty(&config).unwrap();
    let restored: Config = serde_json::from_str(&json).unwrap();
    assert_eq!(
        restored.sidebar_order,
        ["disk:disk:wwid:stable", "cpu", "future:device"]
    );
    assert_eq!(
        restored.sidebar_device_overrides,
        [SidebarDeviceOverrideConfig {
            device: "disk:disk:wwid:stable".into(),
            visible: false,
        }]
    );
}

#[test]
fn font_tokens_round_trip_system_and_bundled_choices() {
    // System everywhere -> the EXPLICIT "system" token ("" is reserved
    // for "never chosen", which the loader resolves to the bundled
    // product faces).
    let (ui, mono) = font_tokens(FontPreference {
        ui: FontChoice::System,
        mono: FontChoice::System,
    });
    assert_eq!(ui, FONT_TOKEN_SYSTEM);
    assert_eq!(mono, FONT_TOKEN_SYSTEM);

    // The default preference is bundled-first: UI persists MiSans VF and
    // the mono role persists Roboto Mono.
    let (ui, mono) = font_tokens(FontPreference::default());
    assert_eq!(ui, FONT_MISANS_VF);
    assert_eq!(mono, FONT_ROBOTO_MONO);

    // Bundled choices -> the exact family names the loader expects.
    let (ui, mono) = font_tokens(FontPreference {
        ui: FontChoice::Bundled,
        mono: FontChoice::Bundled,
    });
    assert_eq!(ui, FONT_MISANS_VF);
    assert_eq!(mono, FONT_ROBOTO_MONO);

    // Mixed per-role.
    let (ui, mono) = font_tokens(FontPreference {
        ui: FontChoice::Bundled,
        mono: FontChoice::System,
    });
    assert_eq!(ui, FONT_MISANS_VF);
    assert_eq!(mono, FONT_TOKEN_SYSTEM);

    let (ui, mono) = font_tokens(FontPreference {
        ui: FontChoice::Custom("Fira Sans"),
        mono: FontChoice::Custom("Cascadia Code"),
    });
    assert_eq!(ui, "Fira Sans");
    assert_eq!(mono, "Cascadia Code");
}

#[test]
fn density_tokens_round_trip_and_unknown_tokens_are_ignored() {
    use crate::gpui_app::theme::tokens::RowDensity;

    // Both densities map to their stable tokens.
    assert_eq!(density_token(RowDensity::Comfortable), DENSITY_COMFORTABLE);
    assert_eq!(density_token(RowDensity::Compact), DENSITY_COMPACT);

    // Tokens parse back; the empty sentinel (old configs) resolves to the
    // built-in comfortable default.
    assert_eq!(
        density_from_token(DENSITY_COMFORTABLE),
        Some(RowDensity::Comfortable)
    );
    assert_eq!(
        density_from_token(DENSITY_COMPACT),
        Some(RowDensity::Compact)
    );
    assert_eq!(density_from_token(""), Some(RowDensity::Comfortable));

    // An unknown token (newer version) is ignored — never a panic, never
    // a fabricated geometry.
    assert_eq!(density_from_token("UltraCompact"), None);
}

fn custom_preset() -> SavedViewPreset {
    SavedViewPreset::restored(
        "Sleeping memory".into(),
        ProcessStatusFilter::Sleeping,
        SortCol::Memory,
        false,
        HashSet::from([SortCol::User, SortCol::Fds]),
    )
}

#[test]
fn custom_presets_roundtrip_without_serializing_builtins_or_capture_fixture() {
    let mut source = DashboardState::default();
    source.restore_user_saved_views(vec![custom_preset()]);
    source.add_capture_saved_view();
    let configs = saved_views_to_config(&source);
    assert_eq!(configs.len(), 1);

    let json = serde_json::to_string(&configs).unwrap();
    let decoded: Vec<ProcessViewPresetConfig> = serde_json::from_str(&json).unwrap();
    let mut restored = DashboardState::default();
    restore_saved_views(&mut restored, &decoded);

    assert_eq!(restored.saved_views.len(), 4);
    let custom = restored.saved_views.last().unwrap();
    assert_eq!(custom.display_name(), "Sleeping memory");
    assert_eq!(custom.filter, ProcessStatusFilter::Sleeping);
    assert_eq!(custom.sort_col, SortCol::Memory);
    assert!(!custom.sort_asc);
    assert_eq!(
        custom.hidden_cols,
        HashSet::from([SortCol::User, SortCol::Fds])
    );
}

#[test]
fn strict_unknown_tokens_skip_whole_preset_and_preserve_process_defaults() {
    assert!(filter_from_token(" Running").is_none());
    assert!(sort_from_token("cpu").is_none());

    let invalid = ProcessViewPresetConfig::new(
        "Future preset".into(),
        "FutureFilter".into(),
        "CPU".into(),
        false,
        Vec::new(),
    );
    assert!(preset_from_config(&invalid).is_none());

    // Mirrors `apply_process_config`'s split state: hidden columns are
    // GPUI-local chrome state, while sort lives in the shell viewing slot.
    let mut processes = ProcessesState::default();
    let mut viewing = taskmanager_shell::ProcessViewing::default();
    viewing.set_sort(SortCol::Nice, taskmanager_shell::SortDir::Desc);
    let config = Config {
        process_sort_col: "future-sort".into(),
        process_sort_asc: true,
        process_hidden_columns: vec!["future-column".into()],
        ..Config::default()
    };
    if let Some(sort) = sort_from_token(&config.process_sort_col) {
        viewing.set_sort(
            sort,
            if config.process_sort_asc {
                taskmanager_shell::SortDir::Asc
            } else {
                taskmanager_shell::SortDir::Desc
            },
        );
    }
    if let Some(hidden) = hidden_from_tokens(&config.process_hidden_columns) {
        processes.hidden_cols = hidden;
    }
    assert_eq!(
        viewing.sort(),
        (SortCol::Nice, taskmanager_shell::SortDir::Desc)
    );
    // Invalid hidden-column tokens are ignored, so hidden stays at the
    // built-in default (the MC 8-column set) instead of being wiped to empty.
    assert_eq!(processes.hidden_cols, ProcessesState::default().hidden_cols);
}

#[test]
fn hidden_column_tokens_are_deterministic_and_strict() {
    let hidden = HashSet::from([SortCol::Fds, SortCol::Swap, SortCol::User]);
    assert_eq!(hidden_tokens(&hidden), ["FDs", "Swap", "User"]);
    assert_eq!(hidden_from_tokens(&hidden_tokens(&hidden)), Some(hidden));
    assert!(hidden_from_tokens(&["Name".into()]).is_none());
    assert!(hidden_from_tokens(&["Future".into()]).is_none());
}

#[test]
fn swap_sort_token_is_stable_and_distinct_from_memory() {
    assert_eq!(sort_token(SortCol::Swap), "Swap");
    assert_eq!(sort_from_token("Swap"), Some(SortCol::Swap));
    assert_ne!(sort_token(SortCol::Swap), sort_token(SortCol::Memory));
}

#[test]
fn desktop_notification_policy_round_trips_through_config_serialization() {
    // BN-07: the notification opt-in and quiet hours persist through the
    // exact Config JSON form ConfigStore writes and restore into the pure
    // gate policy (opt-in default: off).
    let default = Config::default();
    assert!(!default.notify_enabled, "notifications must default to off");
    assert_eq!(default.notify_quiet_hours, None);

    let config = Config {
        notify_enabled: true,
        notify_quiet_hours: Some((22 * 60, 7 * 60)),
        ..Config::default()
    };
    let json = serde_json::to_string_pretty(&config).unwrap();
    let reloaded: Config = serde_json::from_str(&json).unwrap();
    assert!(reloaded.notify_enabled);
    assert_eq!(reloaded.notify_quiet_hours, Some((1320, 420)));

    let policy = crate::core::alerts::NotificationPolicy {
        enabled: reloaded.notify_enabled,
        quiet_hours: reloaded.notify_quiet_hours.map(|(start, end)| {
            crate::core::alerts::QuietHours {
                start_minutes: start,
                end_minutes: end,
            }
        }),
        ..crate::core::alerts::NotificationPolicy::default()
    };
    assert!(policy.enabled);
    let hours = policy.quiet_hours.expect("quiet hours restored");
    assert!(hours.contains_minute(23 * 60));
    assert!(!hours.contains_minute(9 * 60));
}

#[test]
fn column_widths_round_trip_through_config_serialization() {
    // ConfigStore persists `Config` as `serde_json::to_string_pretty` +
    // `fs::write`, and loads it back with `fs::read_to_string` +
    // `serde_json::from_str` (see `config_store.rs`). The filesystem
    // round-trip itself is crate-agnostic and covered by
    // `config_store`'s tests + `core::config`'s round-trip tests; this
    // test exercises the IDENTICAL serde format end-to-end through the
    // token mappers, without the GPUI frontend touching the filesystem
    // (the dependency-firewall forbids `std::fs::` in the GPUI view tree).
    let mut widths = HashMap::new();
    widths.insert(SortCol::Memory, Pixels::from(200.0));
    widths.insert(SortCol::Cpu, Pixels::from(40.0));
    let config = Config {
        process_col_widths: col_widths_to_config(&widths),
        ..Config::default()
    };

    // Save → reload through the exact JSON form ConfigStore writes.
    let json = serde_json::to_string_pretty(&config).unwrap();
    let reloaded: Config = serde_json::from_str(&json).unwrap();

    // Map back into the live view-state form: widths round-trip exactly
    // (Memory=200, Cpu=40).
    let restored = col_widths_from_config(&reloaded.process_col_widths);
    assert_eq!(restored.len(), 2);
    assert_eq!(restored.get(&SortCol::Memory), Some(&Pixels::from(200.0)));
    assert_eq!(restored.get(&SortCol::Cpu), Some(&Pixels::from(40.0)));

    // The serialized list is stable across a re-save (deterministic order
    // — see [`col_widths_save_is_deterministic_and_excludes_name`]).
    assert_eq!(col_widths_to_config(&restored), reloaded.process_col_widths);
}

#[test]
fn missing_or_empty_col_widths_yield_defaults() {
    // No recorded widths → empty map → every column at its built-in
    // default_width (the pre-persistence layout), never fabricated state.
    assert!(col_widths_from_config(&[]).is_empty());
    // The empty map serializes back to an empty config list.
    assert!(col_widths_to_config(&HashMap::new()).is_empty());

    // A default-config round-trip yields no column-width entries, so a
    // first launch (no config file) and a freshly-reset config behave the
    // same as the pre-persistence table.
    let json = serde_json::to_string(&Config::default()).unwrap();
    let decoded: Config = serde_json::from_str(&json).unwrap();
    assert!(decoded.process_col_widths.is_empty());

    // A default ProcessesState starts with empty col_widths.
    assert!(ProcessesState::default().col_widths.is_empty());
}

#[test]
fn corrupt_or_partial_col_widths_fall_back_gracefully() {
    // Unknown column token → dropped; valid entries alongside it survive
    // (per-entry drop, not an all-or-nothing wipe).
    let widths = col_widths_from_config(&[
        ColumnWidthConfig {
            column: "Memory".into(),
            width: 150.0,
        },
        ColumnWidthConfig {
            column: "FutureColumn".into(),
            width: 999.0,
        },
    ]);
    assert_eq!(widths.len(), 1);
    assert_eq!(widths.get(&SortCol::Memory), Some(&Pixels::from(150.0)));

    // `Name` is a known token but non-resizable → dropped, so a hand-edited
    // config can never pin the identity column to a fixed width.
    let widths = col_widths_from_config(&[ColumnWidthConfig {
        column: "Name".into(),
        width: 300.0,
    }]);
    assert!(widths.is_empty());

    // Non-finite / below-floor widths are dropped; an oversized width
    // clamps to the column-width ceiling (mirroring the drag clamp).
    let widths = col_widths_from_config(&[
        ColumnWidthConfig {
            column: "CPU".into(),
            width: f32::NAN,
        },
        ColumnWidthConfig {
            column: "Memory".into(),
            width: 3.0,
        }, // below the 10px floor
        ColumnWidthConfig {
            column: "PID".into(),
            width: 999_999.0,
        }, // clamps to 1200
    ]);
    assert_eq!(widths.len(), 1);
    assert_eq!(widths.get(&SortCol::Pid), Some(&Pixels::from(1200.0)));

    // Duplicate tokens: first occurrence wins (no panic). The save side is
    // already deduplicated + sorted, so this only matters for hand-edited
    // files.
    let widths = col_widths_from_config(&[
        ColumnWidthConfig {
            column: "Memory".into(),
            width: 200.0,
        },
        ColumnWidthConfig {
            column: "Memory".into(),
            width: 400.0,
        },
    ]);
    assert_eq!(widths.len(), 1);
    assert_eq!(widths.get(&SortCol::Memory), Some(&Pixels::from(200.0)));
}

#[test]
fn col_widths_save_is_deterministic_and_excludes_name() {
    // HashMap iteration order is random, but the serialized output is
    // sorted by token → byte-identical across runs, so the config file
    // does not churn on every save when the layout is unchanged.
    let mut widths = HashMap::new();
    widths.insert(SortCol::Cpu, Pixels::from(40.0));
    widths.insert(SortCol::Memory, Pixels::from(200.0));
    widths.insert(SortCol::Pid, Pixels::from(88.0));
    let a = col_widths_to_config(&widths);
    let b = col_widths_to_config(&widths);
    assert_eq!(a, b);
    // Sorted by token: CPU, Memory, PID.
    assert_eq!(
        a.iter().map(|e| e.column.as_str()).collect::<Vec<_>>(),
        ["CPU", "Memory", "PID"]
    );

    // `Name` is non-resizable → never persisted even if present in the
    // live map (defensive: the resize handle never mounts on Name, so this
    // entry should not exist, but the save path drops it regardless).
    let mut with_name = widths.clone();
    with_name.insert(SortCol::Name, Pixels::from(500.0));
    let saved = col_widths_to_config(&with_name);
    assert!(saved.iter().all(|e| e.column != "Name"));
    assert_eq!(saved.len(), 3);
}
