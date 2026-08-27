use super::*;

#[test]
fn secure_boot_probe_reads_only_the_proven_value_byte() {
    let root = crate::test_support::repo_temp_dir()
        .join(format!("taskmanager-efivars-{}", std::process::id()));
    std::fs::create_dir_all(&root).expect("create efivar fixture");
    let name = root.join("SecureBoot-8be4df61-93ca-11d2-aa0d-00e098032b8c");
    std::fs::write(&name, [0, 0, 0, 0, 1]).expect("write enabled fixture");
    assert_eq!(probe_secure_boot_from(&root), Some(true));
    std::fs::write(&name, [0, 0, 0, 0, 0]).expect("write disabled fixture");
    assert_eq!(probe_secure_boot_from(&root), Some(false));
    std::fs::write(&name, [0, 0, 0, 0, 2]).expect("write unknown fixture");
    assert_eq!(probe_secure_boot_from(&root), None);
    std::fs::remove_dir_all(&root).expect("remove efivar fixture");
    assert_eq!(probe_secure_boot_from(&root), None);
}
