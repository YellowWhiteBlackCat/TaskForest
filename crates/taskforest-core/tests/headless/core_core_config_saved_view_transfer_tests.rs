//! Behavior coverage for the platform-neutral saved-view transfer contract:
//! the byte-stable v1 document, the strict import rejections, the portable
//! name rule, and the collision/id bookkeeping the clipboard import needs.

use super::*;

fn preset(name: &str, sort: &str) -> ProcessViewPresetConfig {
    ProcessViewPresetConfig::new(
        name.to_string(),
        "All".to_string(),
        sort.to_string(),
        false,
        Vec::new(),
    )
}

#[test]
fn json_v1_is_byte_stable_and_round_trips() {
    let json = export_saved_views_document(&[preset("Hot cores", "CPU")]).unwrap();
    assert_eq!(
        json,
        concat!(
            "{\n",
            "  \"format\": \"taskmanager.saved-process-views\",\n",
            "  \"version\": 1,\n",
            "  \"presets\": [\n",
            "    {\n",
            "      \"name\": \"Hot cores\",\n",
            "      \"filter\": \"All\",\n",
            "      \"sort\": \"CPU\",\n",
            "      \"sort_asc\": false,\n",
            "      \"hidden_columns\": []\n",
            "    }\n",
            "  ]\n",
            "}"
        )
    );
    assert_eq!(
        import_saved_views_document(&json).unwrap(),
        vec![preset("Hot cores", "CPU")]
    );
}

#[test]
fn import_rejects_foreign_documents_whole() {
    let wrong_format = r#"{"format":"other.product","version":1,"presets":[]}"#;
    assert_eq!(
        import_saved_views_document(wrong_format),
        Err(SavedViewTransferError::UnsupportedFormat)
    );
    let wrong_version = r#"{"format":"taskmanager.saved-process-views","version":2,"presets":[]}"#;
    assert_eq!(
        import_saved_views_document(wrong_version),
        Err(SavedViewTransferError::UnsupportedVersion { found: 2 })
    );
    // Unknown fields are refused rather than silently dropped, so a newer
    // document can never be read as if it were understood.
    let unknown_field =
        r#"{"format":"taskmanager.saved-process-views","version":1,"presets":[],"extra":1}"#;
    assert_eq!(
        import_saved_views_document(unknown_field),
        Err(SavedViewTransferError::InvalidDocument)
    );
    // Truncated JSON and a non-portable preset name both fail whole.
    assert_eq!(
        import_saved_views_document("{not json"),
        Err(SavedViewTransferError::InvalidDocument)
    );
    let unnamed = r#"{"format":"taskmanager.saved-process-views","version":1,"presets":[
        {"name":"  ","filter":"All","sort":"CPU","sort_asc":false,"hidden_columns":[]}]}"#;
    assert_eq!(
        import_saved_views_document(unnamed),
        Err(SavedViewTransferError::InvalidPreset { index: 0 })
    );
    // A control character in the name is not portable either.
    let control = format!(
        "{{\"format\":\"{}\",\"version\":1,\"presets\":[{{\"name\":\"a\\nb\",\"filter\":\"All\",\"sort\":\"CPU\",\"sort_asc\":false,\"hidden_columns\":[]}}]}}",
        SAVED_VIEW_TRANSFER_FORMAT
    );
    assert_eq!(
        import_saved_views_document(&control),
        Err(SavedViewTransferError::InvalidPreset { index: 0 })
    );
    // The document ceiling fails closed before any state is touched.
    let oversized = format!("\"{}\"", "x".repeat(MAX_TRANSFER_BYTES + 1));
    assert_eq!(
        import_saved_views_document(&oversized),
        Err(SavedViewTransferError::TooLarge)
    );
}

#[test]
fn export_validates_names_and_the_preset_ceiling() {
    assert_eq!(
        export_saved_views_document(&[preset("", "CPU")]),
        Err(SavedViewTransferError::InvalidPreset { index: 0 })
    );
    let long_name: String = "n".repeat(MAX_PRESET_NAME_CHARS + 1);
    assert_eq!(
        export_saved_views_document(&[preset(&long_name, "CPU")]),
        Err(SavedViewTransferError::InvalidPreset { index: 0 })
    );
    let fill = (0..MAX_TRANSFER_PRESETS)
        .map(|index| preset(&format!("view-{index}"), "CPU"))
        .collect::<Vec<_>>();
    let mut over = fill.clone();
    over.push(preset("one-more", "CPU"));
    assert!(matches!(
        export_saved_views_document(&over),
        Err(SavedViewTransferError::TooManyPresets)
    ));
    // At the cap the export still succeeds.
    assert!(export_saved_views_document(&fill).is_ok());
}

#[test]
fn portable_names_exclude_empty_untrimmed_control_and_oversized() {
    assert!(saved_view_name_is_portable("Hot cores"));
    assert!(saved_view_name_is_portable(
        &"n".repeat(MAX_PRESET_NAME_CHARS)
    ));
    assert!(!saved_view_name_is_portable(""));
    assert!(!saved_view_name_is_portable(" padded"));
    assert!(!saved_view_name_is_portable("trailing "));
    assert!(!saved_view_name_is_portable("line\nbreak"));
    assert!(!saved_view_name_is_portable(
        &"n".repeat(MAX_PRESET_NAME_CHARS + 1)
    ));
}

#[test]
fn unique_names_keep_a_free_base_and_suffix_a_collision() {
    let free = HashSet::new();
    assert_eq!(unique_saved_view_name("Games", &free).unwrap(), "Games");

    let occupied = ["Games".to_string()].into_iter().collect::<HashSet<_>>();
    assert_eq!(
        unique_saved_view_name("Games", &occupied).unwrap(),
        "Games (2)"
    );

    // A name at the ceiling is truncated by the suffix length, never
    // exceeding the portable limit after the rename.
    let long: String = "n".repeat(MAX_PRESET_NAME_CHARS);
    let occupied = [long.clone()].into_iter().collect::<HashSet<_>>();
    let renamed = unique_saved_view_name(&long, &occupied).unwrap();
    assert_eq!(renamed.chars().count(), MAX_PRESET_NAME_CHARS);
    assert_eq!(
        renamed,
        format!("{} (2)", "n".repeat(MAX_PRESET_NAME_CHARS - 4))
    );
}

#[test]
fn import_names_resolve_in_order_and_count_renames() {
    let occupied = ["Games".to_string()].into_iter().collect::<HashSet<_>>();
    let resolved = resolve_saved_view_import_names(
        &occupied,
        vec![
            "Fresh".to_string(),
            "Games".to_string(),
            "Games".to_string(),
        ],
    )
    .unwrap();
    // Both colliding presets keep their import order and get distinct names.
    assert_eq!(
        resolved.names,
        vec![
            "Fresh".to_string(),
            "Games (2)".to_string(),
            "Games (3)".to_string()
        ]
    );
    assert_eq!(resolved.renamed, 2);
    // The pre-existing name is the only rename when the document holds one.
    let single = resolve_saved_view_import_names(&occupied, vec!["Games".to_string()]).unwrap();
    assert_eq!(single.names, vec!["Games (2)".to_string()]);
    assert_eq!(single.renamed, 1);
}

#[test]
fn id_allocation_skips_occupied_ids_and_fails_closed_on_wrap() {
    let occupied = [1_u64, 2, 3].into_iter().collect::<HashSet<_>>();
    let allocation = allocate_saved_view_ids(&occupied, 2, 3).unwrap();
    assert_eq!(allocation.ids, vec![4, 5, 6]);
    assert_eq!(allocation.next_id, 7);
    // An empty allocation still reports the cursor it stopped at.
    let idle = allocate_saved_view_ids(&occupied, 9, 0).unwrap();
    assert!(idle.ids.is_empty());
    assert_eq!(idle.next_id, 9);
    // A saturated id space is a typed failure, never a reused id.
    assert_eq!(
        allocate_saved_view_ids(&HashSet::new(), u64::MAX, 2),
        Err(SavedViewTransferError::IdSpaceExhausted)
    );
}

#[test]
fn errors_render_a_stable_readable_reason() {
    assert_eq!(
        SavedViewTransferError::UnsupportedVersion { found: 7 }.to_string(),
        "unsupported saved-view version: 7"
    );
    assert_eq!(
        SavedViewTransferError::InvalidPreset { index: 3 }.to_string(),
        "invalid saved view 3"
    );
}
