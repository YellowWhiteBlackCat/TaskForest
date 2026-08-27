use super::*;

#[test]
fn known_user_folders_are_absolute() {
    for folder in [
        KnownFolder::RoamingAppData,
        KnownFolder::LocalAppData,
        KnownFolder::Startup,
    ] {
        let path = known_folder_path(folder).expect("Windows Known Folder should resolve");
        assert!(path.is_absolute());
        assert!(!path.as_os_str().is_empty());
    }
}
