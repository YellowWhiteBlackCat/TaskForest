//! test-intent: behavior
//! Shared transfer, configuration and stale-control identity rules.

use super::*;

fn custom(name: &str) -> SavedViewPreset {
    SavedViewPreset::restored(
        name.into(),
        ProcessStatusFilter::Sleeping,
        SortCol::Pss,
        false,
        HashSet::from([SortCol::User, SortCol::Fds]),
    )
}

#[test]
fn user_views_roundtrip_and_import_is_atomic_with_collision_renaming() {
    let mut views = default_built_in_presets();
    let mut next = 4;
    save_current_view(&mut views, &mut next, custom("Triage")).expect("save");
    let document = export_saved_views_json(&views).expect("export");
    assert!(document.contains("MemoryPss"));
    assert!(!document.contains("cpu_hotspots"));
    let summary = import_saved_views_json(&mut views, &mut next, &document).expect("import");
    assert_eq!(
        summary,
        SavedViewImportSummary {
            imported: 1,
            renamed: 1
        }
    );
    assert_eq!(
        views.last().expect("imported").user_name(),
        Some("Triage (2)")
    );
    assert_eq!(views.last().expect("imported").sort_col, SortCol::Pss);
    let before = views.clone();
    assert!(import_saved_views_json(&mut views, &mut next, "invalid JSON").is_err());
    assert_eq!(views, before);
    let invalid = document.replace("FDs", "Name");
    assert!(import_saved_views_json(&mut views, &mut next, &invalid).is_err());
    assert_eq!(views, before);
}

#[test]
fn published_pss_payload_canonicalizes_at_ingress_and_reexports_current_column_id() {
    let mut views = default_built_in_presets();
    let mut next = 4;
    let wire = r#"{"format":"taskmanager.saved-process-views","version":1,"presets":[{"name":"Legacy memory","filter":"All","sort":"PSS","sort_asc":false,"hidden_columns":["PSS"]}]}"#;
    import_saved_views_json(&mut views, &mut next, wire).expect("published ingress");
    assert_eq!(
        views.last().expect("row").hidden_cols,
        HashSet::from([SortCol::Pss])
    );
    let exported = export_saved_views_json(&views).expect("export");
    assert!(exported.contains("MemoryPss"));
    assert!(!exported.contains("\"PSS\""));
}

#[test]
fn config_echo_preserves_ids_but_removed_ids_never_redirect_to_new_names() {
    let mut views = default_built_in_presets();
    let mut next = 4;
    let id = save_current_view(&mut views, &mut next, custom("Original")).expect("save");
    let configs: Vec<_> = views.iter().filter_map(preset_to_config).collect();
    restore_saved_views(&mut views, &mut next, &configs).expect("echo");
    assert_eq!(views.last().expect("same identity").id, id);
    let mut replacement = configs[0].clone();
    replacement.name = "Replacement".into();
    restore_saved_views(&mut views, &mut next, &[replacement]).expect("external replacement");
    assert!(views.iter().all(|entry| entry.id != id));
    assert_eq!(
        views.last().expect("replacement").user_name(),
        Some("Replacement")
    );
}

#[test]
fn saved_view_review_keeps_user_controls_first_without_changing_persisted_identity_order() {
    let mut views = default_built_in_presets();
    let mut next = 4;
    let first = save_current_view(&mut views, &mut next, custom("First")).expect("first");
    let second = save_current_view(&mut views, &mut next, custom("Second")).expect("second");
    assert_eq!(
        review_rows(&views).map(|row| row.id).collect::<Vec<_>>(),
        vec![first, second, 1, 2, 3]
    );
    assert_eq!(
        views
            .iter()
            .filter_map(preset_to_config)
            .map(|preset| preset.name)
            .collect::<Vec<_>>(),
        vec!["First", "Second"]
    );
}
