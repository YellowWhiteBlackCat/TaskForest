use super::*;
use taskmanager_core::core::config::SAVED_VIEW_TRANSFER_FORMAT;

#[test]
fn test_export_import_roundtrip() {
    let mut presets = default_built_in_presets();
    let mut custom = SavedViewPreset::restored(
        "Triage View".to_string(),
        ProcessStatusFilter::Running,
        SortCol::Memory,
        true,
        HashSet::from([SortCol::User, SortCol::Nice]),
    );
    custom.id = 10;
    presets.push(custom);

    let json = export_saved_views_json(&presets).expect("export should succeed");
    assert!(json.contains("Triage View"));
    assert!(json.contains(SAVED_VIEW_TRANSFER_FORMAT));

    let mut target = default_built_in_presets();
    let mut next_id = 100;
    let summary =
        import_saved_views_json(&mut target, &mut next_id, &json).expect("import should succeed");
    assert_eq!(summary.imported, 1);
    assert_eq!(summary.renamed, 0);
    assert_eq!(target.len(), 4);
    assert_eq!(target.last().unwrap().custom_name, "Triage View");
    let document: serde_json::Value = serde_json::from_str(&json).unwrap();
    assert_eq!(
        document["presets"][0],
        serde_json::json!({
            "name": "Triage View",
            "filter": "Running",
            "sort": "Memory",
            "sort_asc": true,
            "hidden_columns": ["Nice", "User"]
        })
    );
}

#[test]
fn test_duplicate_name_dedup() {
    let mut target = default_built_in_presets();
    let mut custom = SavedViewPreset::restored(
        "Work".to_string(),
        ProcessStatusFilter::All,
        SortCol::Cpu,
        false,
        HashSet::new(),
    );
    custom.id = 4;
    target.push(custom);

    let mut incoming = default_built_in_presets();
    let mut to_export = SavedViewPreset::restored(
        "Work".to_string(),
        ProcessStatusFilter::Running,
        SortCol::Cpu,
        false,
        HashSet::new(),
    );
    to_export.id = 10;
    incoming.push(to_export);

    let json = export_saved_views_json(&incoming).unwrap();
    let mut next_id = 5;
    let summary = import_saved_views_json(&mut target, &mut next_id, &json).unwrap();
    assert_eq!(summary.imported, 1);
    assert_eq!(summary.renamed, 1);
    assert_eq!(target.last().unwrap().custom_name, "Work (2)");
}

#[test]
fn legacy_wire_modes_import_into_the_canonical_runtime_preset() {
    let json = r#"{
        "format":"taskmanager.saved-process-views",
        "version":1,
        "presets":[{
            "name":"Legacy",
            "mode":"Tree",
            "filter":"Running",
            "sort":"CPU",
            "sort_asc":false,
            "hidden_columns":[]
        }]
    }"#;
    let mut target = default_built_in_presets();
    let mut next_id = 100;
    let summary = import_saved_views_json(&mut target, &mut next_id, json).unwrap();
    assert_eq!(summary.imported, 1);
    assert_eq!(target.last().unwrap().filter, ProcessStatusFilter::Running);
    let exported = export_saved_views_json(&target).unwrap();
    let document: serde_json::Value = serde_json::from_str(&exported).unwrap();
    assert_eq!(
        document["presets"][0],
        serde_json::json!({
            "name": "Legacy",
            "filter": "Running",
            "sort": "CPU",
            "sort_asc": false,
            "hidden_columns": []
        })
    );
}
