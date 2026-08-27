#[test]
fn partition_label_formatting() {
    use super::partition_label;

    let win_part = taskmanager_test_support::DiskPartitionFixtureBuilder::new()
        .name("C:\\".into())
        .mount_point("C:\\".into())
        .fs_type("NTFS".into())
        .build();
    assert_eq!(partition_label(&win_part), "C:\\ · NTFS");

    let linux_part = taskmanager_test_support::DiskPartitionFixtureBuilder::new()
        .name("nvme0n1p2".into())
        .mount_point("/home".into())
        .fs_type("ext4".into())
        .build();
    if cfg!(target_os = "linux") {
        assert_eq!(
            partition_label(&linux_part),
            "/dev/nvme0n1p2 · /home · ext4"
        );
    }
}

#[test]
fn unmounted_names_collapse_to_a_compact_locale_neutral_list() {
    use super::unmounted_names;

    let first = taskmanager_test_support::DiskPartitionFixtureBuilder::new()
        .name("nvme0n1p3".into())
        .build();
    let second = taskmanager_test_support::DiskPartitionFixtureBuilder::new()
        .name("/dev/nvme0n1p4".into())
        .build();
    let windows = taskmanager_test_support::DiskPartitionFixtureBuilder::new()
        .name("C:\\".into())
        .build();
    assert_eq!(unmounted_names(&[&first, &second]), "nvme0n1p3 · nvme0n1p4");
    // Windows drive names never carry a /dev/ prefix and stay intact.
    assert_eq!(unmounted_names(&[&windows]), "C:\\");
    assert_eq!(unmounted_names(&[]), "");
}
