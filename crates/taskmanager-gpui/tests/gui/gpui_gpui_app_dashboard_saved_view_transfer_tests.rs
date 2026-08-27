use super::*;

fn preset(name: &str) -> SavedViewPreset {
    SavedViewPreset::restored(
        name.into(),
        ProcessStatusFilter::Sleeping,
        SortCol::Memory,
        false,
        HashSet::from([SortCol::Fds, SortCol::User]),
    )
}

#[test]
fn versioned_json_roundtrips_deterministically() {
    let mut source = DashboardState::default();
    source.restore_user_saved_views(vec![preset("Sleeping memory")]);
    source.add_capture_saved_view();

    let json = export_saved_views_json(&source).unwrap();
    assert_eq!(json, export_saved_views_json(&source).unwrap());
    assert!(json.contains("\"format\": \"taskmanager.saved-process-views\""));
    assert!(json.contains("\"version\": 1"));
    assert!(json.contains("\"hidden_columns\": [\n        \"FDs\",\n        \"User\""));
    assert!(!json.contains("CPU hotspots"));
    assert!(!json.contains("Production triage"));

    let mut restored = DashboardState::default();
    let summary = import_saved_views_json(&mut restored, &json).unwrap();
    assert_eq!(
        summary,
        SavedViewImportSummary {
            imported: 1,
            renamed: 0,
        }
    );
    let custom = restored.saved_views.last().unwrap();
    assert_eq!(custom.display_name(), "Sleeping memory");
    assert_eq!(custom.filter, ProcessStatusFilter::Sleeping);
    assert_eq!(custom.sort_col, SortCol::Memory);
    assert_eq!(
        custom.hidden_cols,
        HashSet::from([SortCol::Fds, SortCol::User])
    );
}

#[test]
fn canonical_config_and_transfer_write_no_runtime_mode_field() {
    let preset = SavedViewPreset::restored(
        "Category triage".into(),
        ProcessStatusFilter::All,
        SortCol::Cpu,
        false,
        HashSet::new(),
    );

    // Config persistence round-trip.
    let config = preset_to_config(&preset).expect("config projection");
    assert!(serde_json::to_value(&config).unwrap().get("mode").is_none());
    let restored = preset_from_config(&config).expect("config restore");
    assert_eq!(restored.display_name(), "Category triage");

    // Transfer JSON uses the same canonical, mode-free shape.
    let mut source = DashboardState::default();
    source.restore_user_saved_views(vec![preset]);
    let json = export_saved_views_json(&source).unwrap();
    assert!(!json.contains("\"mode\""));
    let mut target = DashboardState::default();
    let summary = import_saved_views_json(&mut target, &json).unwrap();
    assert_eq!(summary.imported, 1);
    assert_eq!(
        target.saved_views.last().unwrap().display_name(),
        "Category triage"
    );
}

#[test]
fn legacy_wire_modes_are_accepted_but_reexported_without_mode() {
    let mut state = DashboardState::default();
    for (index, mode) in [
        "Flat",
        "Tree",
        "GroupByApp",
        "GroupByType",
        "GroupByCategory",
    ]
    .into_iter()
    .enumerate()
    {
        let json = format!(
            r#"{{"format":"taskmanager.saved-process-views","version":1,"presets":[{{"name":"Legacy {index}","mode":"{mode}","filter":"Running","sort":"CPU","sort_asc":false,"hidden_columns":[]}}]}}"#
        );
        assert_eq!(
            import_saved_views_json(&mut state, &json).unwrap().imported,
            1
        );
    }
    let exported = export_saved_views_json(&state).unwrap();
    assert!(!exported.contains("\"mode\""));
}

#[test]
fn malformed_unknown_and_invalid_data_are_atomic() {
    let mut state = DashboardState::default();
    let before: Vec<_> = state.saved_views.iter().map(|preset| preset.id).collect();
    for invalid in [
        "not json",
        r#"{"format":"taskmanager.saved-process-views","version":2,"presets":[]}"#,
        r#"{"format":"taskmanager.saved-process-views","version":1,"presets":[],"future":true}"#,
        r#"{"format":"taskmanager.saved-process-views","version":1,"presets":[{"name":"Bad","mode":"Future","filter":"All","sort":"CPU","sort_asc":false,"hidden_columns":[]}]}"#,
        r#"{"format":"taskmanager.saved-process-views","version":1,"presets":[{"name":"Bad","mode":"Flat","filter":"All","sort":"CPU","sort_asc":false,"hidden_columns":["FDs","FDs"]}]}"#,
    ] {
        assert!(import_saved_views_json(&mut state, invalid).is_err());
        assert_eq!(
            state
                .saved_views
                .iter()
                .map(|preset| preset.id)
                .collect::<Vec<_>>(),
            before
        );
    }
}

#[test]
fn conflicts_append_predictable_names_without_touching_builtins() {
    let mut state = DashboardState::default();
    let builtins: Vec<_> = state
        .saved_views
        .iter()
        .map(|preset| (preset.id, preset.display_name(), preset.built_in))
        .collect();
    let protected_name = state.saved_views[0].display_name();
    state.restore_user_saved_views(vec![preset("Team view"), preset("Team view (2)")]);
    let json = serde_json::json!({
        "format": SAVED_VIEW_TRANSFER_FORMAT,
        "version": SAVED_VIEW_TRANSFER_VERSION,
        "presets": [
            {
                "name": "Team view",
                "filter": "All",
                "sort": "CPU",
                "sort_asc": false,
                "hidden_columns": [],
            },
            {
                "name": "Team view",
                "filter": "All",
                "sort": "CPU",
                "sort_asc": false,
                "hidden_columns": [],
            },
            {
                "name": protected_name,
                "filter": "All",
                "sort": "CPU",
                "sort_asc": false,
                "hidden_columns": [],
            },
        ],
    })
    .to_string();
    let summary = import_saved_views_json(&mut state, &json).unwrap();
    assert_eq!(summary.imported, 3);
    assert_eq!(summary.renamed, 3);
    let names: Vec<_> = state
        .saved_views
        .iter()
        .filter_map(SavedViewPreset::user_name)
        .collect();
    assert_eq!(
        &names[names.len() - 3..],
        [
            "Team view (3)",
            "Team view (4)",
            &format!("{} (2)", builtins[0].1)
        ]
    );
    assert_eq!(
        state
            .saved_views
            .iter()
            .take(3)
            .map(|preset| (preset.id, preset.display_name(), preset.built_in))
            .collect::<Vec<_>>(),
        builtins
    );
}
