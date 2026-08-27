use super::*;

#[test]
fn default_config_round_trips_through_json() {
    let original = Config::default();
    let json = serde_json::to_string_pretty(&original).unwrap();
    let reloaded = serde_json::from_str(&json).unwrap();
    assert_eq!(original, reloaded);
}

#[test]
fn custom_config_round_trips_through_json() {
    let original = Config {
        skin: "KDE".to_string(),
        mode: "Dark".to_string(),
        hc: true,
        ui_font: "MiSans VF".to_string(),
        mono_font: "Roboto Mono".to_string(),
        density: DENSITY_COMPACT.to_string(),
        ui_size: "future-size-token".to_string(),
        text_rendering: TEXT_RENDERING_SUBPIXEL.to_string(),
        motion: MOTION_REDUCED.to_string(),
        show_cpu: true,
        show_memory: false,
        show_disks: true,
        show_network: false,
        show_network_wired: false,
        show_network_wireless: true,
        show_network_vpn: false,
        show_network_virtual: true,
        show_network_other: false,
        show_gpus: true,
        memory_use_bytes: false,
        memory_use_base2: false,
        drive_use_bytes: false,
        drive_use_base2: false,
        network_use_bytes: true,
        network_use_base2: true,
        graph_data_points: 240,
        sliding_graphs: true,
        network_dynamic_scaling: false,
        sidebar_order: vec!["memory".into(), "cpu".into()],
        sidebar_device_overrides: vec![SidebarDeviceOverrideConfig {
            device: "disk:nvme0n1".into(),
            visible: false,
        }],
        gray_zero_values: true,
        notify_enabled: true,
        notify_quiet_hours: Some((1320, 420)),
        refresh_ms: 2500,
        last_page: "apps".to_string(),
        startup_page: STARTUP_PAGE_PROCESSES.to_string(),
        process_sort_col: "Memory".to_string(),
        process_sort_asc: true,
        process_hidden_columns: vec!["PID".into(), "User".into()],
        process_hidden_columns_configured: true,
        process_col_widths: vec![
            ColumnWidthConfig {
                column: "Memory".into(),
                width: 200.0,
            },
            ColumnWidthConfig {
                column: "CPU".into(),
                width: 40.0,
            },
        ],
        saved_process_views: vec![ProcessViewPresetConfig::new(
            "Investigate sleepers".into(),
            "Sleeping".into(),
            "Memory".into(),
            false,
            vec!["User".into(), "FDs".into()],
        )],
        sidebar_width: default_sidebar_width(),
        language: Some("zh".to_string()),
        history_persistence: true,
    };
    let json = serde_json::to_string_pretty(&original).unwrap();
    let reloaded = serde_json::from_str(&json).unwrap();
    assert_eq!(original, reloaded);
}

#[test]
fn system_color_scheme_token_round_trips_without_resolving_a_palette() {
    let original = Config {
        mode: COLOR_SCHEME_SYSTEM.to_string(),
        ..Config::default()
    };
    let json = serde_json::to_string(&original).unwrap();
    let reloaded: Config = serde_json::from_str(&json).unwrap();
    assert_eq!(reloaded.mode, COLOR_SCHEME_SYSTEM);
}

#[test]
fn old_config_missing_new_fields_uses_the_default_matrix() {
    // A config file written by an older version lacks every field added
    // after the original preference schema. serde `#[serde(default)]` /
    // `#[serde(default = ...)]` must fill the migration matrix so the file
    // still LOADS (rather than falling back to a blanket Config::default()
    // that would also wipe the recorded skin/mode/refresh).
    let old_json = r#"{
            "skin": "KDE",
            "mode": "Dark",
            "hc": false,
            "show_cpu": true,
            "show_memory": true,
            "show_disks": true,
            "show_network": true,
            "show_gpus": true,
            "refresh_ms": 2500,
            "last_page": "apps"
        }"#;
    let c: Config = serde_json::from_str(old_json).expect("old config must load");
    // Recorded existing preferences survive the parse.
    assert_eq!(c.skin, "KDE");
    assert_eq!(c.mode, "Dark");
    assert_eq!(c.refresh_ms, 2500);
    assert_eq!(c.last_page, "apps");
    // Network-category fields added after the original master toggle use
    // true serde defaults, preserving the old all-network-visible view.
    assert!(c.show_network_wired);
    assert!(c.show_network_wireless);
    assert!(c.show_network_vpn);
    assert!(c.show_network_virtual);
    assert!(c.show_network_other);
    // Units added after the original settings surface retain Mission
    // Center's defaults when an old JSON file omits them.
    assert!(c.memory_use_bytes);
    assert!(c.memory_use_base2);
    assert!(c.drive_use_bytes);
    assert!(c.drive_use_base2);
    assert!(!c.network_use_bytes);
    assert!(!c.network_use_base2);
    assert_eq!(c.graph_data_points, 60);
    assert!(!c.sliding_graphs);
    assert!(c.network_dynamic_scaling);
    // Font tokens added later default to "" = system fonts.
    assert_eq!(c.ui_font, "");
    assert_eq!(c.mono_font, "");
    // Density token added later defaults to "" = comfortable geometry.
    assert_eq!(c.density, "");
    // UI size is independent from row density; empty resolves to Standard at
    // the renderer boundary.
    assert_eq!(c.ui_size, "");
    // Text-rendering token added later defaults to "" = platform default.
    assert_eq!(c.text_rendering, "");
    // Motion token added later defaults to the full-motion "normal" token,
    // so an old config keeps the animated behavior instead of an empty
    // string the consumers would have to interpret.
    assert_eq!(c.motion, MOTION_NORMAL);
    // Apps zero-value styling added later defaults to off, preserving the
    // previous table colors for an existing config file.
    assert!(!c.gray_zero_values);
    // Startup-page token added later defaults to "" = remember last page.
    assert_eq!(c.startup_page, STARTUP_PAGE_REMEMBER);
    // New fields take their serde defaults (not a blanket Default wipe).
    assert_eq!(c.process_sort_col, "");
    assert!(!c.process_sort_asc);
    assert!(c.process_hidden_columns.is_empty());
    assert!(!c.process_hidden_columns_configured);
    // Column widths added later default to empty (every column at its
    // built-in default width), so an old config does not fabricate widths.
    assert!(c.process_col_widths.is_empty());
    assert_eq!(c.sidebar_width, 260.0);
    assert!(c.saved_process_views.is_empty());
    assert!(c.sidebar_order.is_empty());
    assert!(c.sidebar_device_overrides.is_empty());
    // The language preference added later (G-22) defaults to None — no
    // recorded token, so frontends keep their fallback chain exactly as
    // before the field existed.
    assert_eq!(c.language, None);
}

#[test]
fn language_preference_round_trips_and_defaults_to_none() {
    // Cold start: no recorded preference.
    assert_eq!(Config::default().language, None);
    // A recorded token survives a save/load cycle verbatim — core stores
    // the opaque string; interpreting/validating it belongs to consumers.
    let original = Config {
        language: Some("zh".to_string()),
        ..Config::default()
    };
    let json = serde_json::to_string(&original).unwrap();
    let reloaded: Config = serde_json::from_str(&json).unwrap();
    assert_eq!(reloaded.language.as_deref(), Some("zh"));
    assert_eq!(
        reloaded, original,
        "the language field must not perturb the rest of the round trip"
    );
}

#[test]
fn motion_preference_defaults_to_full_motion_and_round_trips() {
    // Cold start: the full-motion token, not an empty sentinel.
    assert_eq!(Config::default().motion, MOTION_NORMAL);
    // A minimal file omitting the field takes the serde default (the same
    // full-motion token), and a missing key never fails the whole load.
    let minimal: Config = serde_json::from_str(r#"{"refresh_ms": 500}"#).unwrap();
    assert_eq!(minimal.motion, MOTION_NORMAL);
    // Every recorded token survives a save/load cycle verbatim — core
    // stores the opaque string; validating it belongs to the consumers.
    for token in [MOTION_NORMAL, MOTION_REDUCED, MOTION_NONE, "warp-speed"] {
        let original = Config {
            motion: token.to_string(),
            ..Config::default()
        };
        let reloaded: Config =
            serde_json::from_str(&serde_json::to_string(&original).unwrap()).unwrap();
        assert_eq!(reloaded.motion, token);
        assert_eq!(
            reloaded, original,
            "the motion field must not perturb the rest of the round trip"
        );
    }
}

#[test]
fn minimal_config_file_parses_with_defaults() {
    // The capture harness (and any hand-edited config) may write a
    // minimal file naming ONLY the field it wants to flip; every other
    // field must come from its serde default, not fail the whole load.
    let minimal: Config = serde_json::from_str(r#"{"history_persistence": true}"#)
        .expect("minimal config file must parse");
    assert!(minimal.history_persistence);
    assert_eq!(
        minimal.graph_data_points,
        Config::default().graph_data_points
    );
    let empty: Config = serde_json::from_str("{}").expect("empty object must parse to defaults");
    assert!(!empty.history_persistence);
}

#[test]
fn history_persistence_defaults_off_and_round_trips() {
    // Privacy default: an old config file (no field) stays disabled, and
    // the default config writes nothing to disk.
    assert!(!Config::default().history_persistence);
    let legacy: Config = serde_json::from_str(
        &serde_json::to_string(&Config {
            history_persistence: true,
            ..Config::default()
        })
        .unwrap()
        .replace("history_persistence", "removed_by_test"),
    )
    .unwrap();
    assert!(!legacy.history_persistence);

    let enabled = Config {
        history_persistence: true,
        ..Config::default()
    };
    let reloaded: Config = serde_json::from_str(&serde_json::to_string(&enabled).unwrap()).unwrap();
    assert!(reloaded.history_persistence);
    assert_eq!(reloaded, enabled);
}

#[test]
fn current_config_serialization_omits_the_retired_process_mode() {
    let value = serde_json::to_value(Config::default()).unwrap();
    assert!(value.get("process_view_mode").is_none());
}

#[test]
fn legacy_process_modes_stop_at_the_private_config_and_preset_wire_boundaries() {
    for legacy in [
        PROCESS_VIEW_MODE_FLAT,
        PROCESS_VIEW_MODE_TREE,
        PROCESS_VIEW_MODE_GROUP_BY_APP,
        PROCESS_VIEW_MODE_GROUP_BY_TYPE,
    ] {
        let json = format!(
            r#"{{"process_view_mode":"{legacy}","saved_process_views":[{{"name":"old","mode":"{legacy}","filter":"All","sort":"CPU","sort_asc":false}}]}}"#
        );
        let config: Config = serde_json::from_str(&json).unwrap();
        assert_eq!(config.saved_process_views.len(), 1);
        let serialized = serde_json::to_value(&config).unwrap();
        assert!(serialized.get("process_view_mode").is_none());
        assert!(serialized["saved_process_views"][0].get("mode").is_none());
    }

    let config: Config = serde_json::from_str(
        r#"{"process_view_mode":"FutureMode","saved_process_views":[{"name":"future","mode":"FutureMode","filter":"All","sort":"CPU","sort_asc":false}]}"#,
    )
    .unwrap();
    assert!(config.saved_process_views.is_empty());
}

#[test]
fn default_process_columns_all_visible_and_sort_empty() {
    // Default hidden-columns set is empty (all columns visible) and no
    // recorded sort preference (the view applies its CPU-descending
    // built-in default). Default column widths are empty too (every column
    // at its built-in default width), so a cold start never fabricates a
    // resized layout.
    let d = Config::default();
    assert!(d.process_hidden_columns.is_empty());
    assert!(d.process_col_widths.is_empty());
    assert_eq!(d.process_sort_col, "");
    assert!(!d.process_sort_asc);
}

#[test]
fn column_width_entries_round_trip_through_json() {
    // The persisted column-width form is a plain {column,width} record so
    // an external reader (or a future schema migration) can edit it
    // without pulling in the GPUI layer. The token↔SortCol mapping is
    // owned by `gpui_app`; core only round-trips the opaque strings +
    // the f32 width.
    let entries = vec![
        ColumnWidthConfig {
            column: "Memory".into(),
            width: 200.0,
        },
        ColumnWidthConfig {
            column: "CPU".into(),
            width: 40.0,
        },
    ];
    let json = serde_json::to_string_pretty(&entries).unwrap();
    let decoded: Vec<ColumnWidthConfig> = serde_json::from_str(&json).unwrap();
    assert_eq!(decoded, entries);
    // A single entry round-trips on its own too.
    let one = ColumnWidthConfig {
        column: "PID".into(),
        width: 88.0,
    };
    assert_eq!(
        serde_json::from_str::<ColumnWidthConfig>(&serde_json::to_string(&one).unwrap()).unwrap(),
        one
    );
}

#[test]
fn default_last_page_is_performance_token() {
    // The cold-start page token must match PAGE_PERFORMANCE so a missing
    // config selects the Performance page exactly like pre-persistence.
    assert_eq!(Config::default().last_page, PAGE_PERFORMANCE);
}

#[test]
fn default_startup_page_remembers_the_last_page() {
    // Cold-start + serde default for startup_page must be the remember-last
    // token ("" sentinel) so a missing config (and an old config file)
    // opens the last page exactly like pre-persistence behavior — never a
    // forced page.
    assert_eq!(Config::default().startup_page, STARTUP_PAGE_REMEMBER);
    // The fixed-page tokens mirror the page tokens `last_page` round-trips,
    // so a fixed startup page can be applied by overriding the restored page.
    assert_eq!(STARTUP_PAGE_PERFORMANCE, PAGE_PERFORMANCE);
    assert_eq!(STARTUP_PAGE_PROCESSES, "apps");
}
