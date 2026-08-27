use super::*;

#[test]
fn resolves_theme_directory_and_inherited_fallback() {
    let root = tempfile_dir("icon-theme");
    let data = root.join("data");
    let theme = data.join("icons/TaskTheme/apps");
    let inherited = data.join("icons/hicolor/scalable/apps");
    std::fs::create_dir_all(&theme).expect("theme directory");
    std::fs::create_dir_all(&inherited).expect("inherited directory");
    std::fs::write(
        data.join("icons/TaskTheme/index.theme"),
        "[Icon Theme]\nDirectories=apps\nInherits=hicolor\n",
    )
    .expect("theme index");
    std::fs::write(inherited.join("editor.svg"), b"<svg></svg>").expect("inherited SVG");

    let (asset, failure) = resolve_icon_asset_from_dirs_with_themes(
        std::slice::from_ref(&data),
        Some("editor"),
        &["TaskTheme".to_owned()],
    );
    assert_eq!(failure, None);
    assert_eq!(
        asset.map(|asset| asset.format),
        Some(ApplicationIconFormat::Svg)
    );
    remove_temp_dir(root);
}

#[test]
fn rejects_parent_traversal_and_bad_magic_without_reading_unbounded_data() {
    let root = tempfile_dir("icon-safety");
    let data = root.join("data");
    let pixmaps = data.join("pixmaps");
    std::fs::create_dir_all(&pixmaps).expect("pixmaps directory");
    std::fs::write(pixmaps.join("broken.png"), b"not-a-png").expect("bad icon");

    let (asset, traversal_failure) = resolve_icon_asset_from_dirs_with_themes(
        std::slice::from_ref(&data),
        Some("../broken"),
        &["hicolor".to_owned()],
    );
    assert_eq!(asset, None);
    assert_eq!(traversal_failure, Some(ProcessMetadataFailure::Unsupported));

    let (asset, format_failure) = resolve_icon_asset_from_dirs_with_themes(
        std::slice::from_ref(&data),
        Some("broken.png"),
        &["hicolor".to_owned()],
    );
    assert_eq!(asset, None);
    assert_eq!(format_failure, Some(ProcessMetadataFailure::Unsupported));
    remove_temp_dir(root);
}

fn tempfile_dir(label: &str) -> PathBuf {
    let path = crate::test_support::repo_temp_dir()
        .join(format!("taskmanager-{label}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&path);
    path
}

fn remove_temp_dir(path: PathBuf) {
    let _ = std::fs::remove_dir_all(path);
}
