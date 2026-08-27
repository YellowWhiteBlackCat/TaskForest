use super::{LocalTimeReadError, read_bounded};

#[test]
fn directory_is_rejected_before_open_or_read() {
    let directory = crate::test_support::repo_temp_dir().join("local-time-directory");
    std::fs::create_dir(&directory).expect("create isolated local-time directory fixture");
    let result = read_bounded(&directory);
    std::fs::remove_dir(&directory).expect("remove isolated local-time directory fixture");
    assert!(matches!(result, Err(LocalTimeReadError::NotRegular)));
}
