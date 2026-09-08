use super::*;

#[test]
fn parse_psi_window_parses_real_kernel_line() {
    let line = "some avg10=0.03 avg60=0.17 avg300=0.19 total=703209861";
    let window = parse_psi_window(line).expect("must parse psi window");
    assert!((window.avg10 - 0.03).abs() < f32::EPSILON);
    assert!((window.avg60 - 0.17).abs() < f32::EPSILON);
    assert!((window.avg300 - 0.19).abs() < f32::EPSILON);
    assert_eq!(window.total_us, 703209861);
}

#[test]
fn parse_psi_file_parses_some_and_full() {
    let content = "some avg10=1.25 avg60=0.50 avg300=0.10 total=50000\nfull avg10=0.10 avg60=0.05 avg300=0.01 total=12000\n";
    let resource = parse_psi_file(content).expect("must parse psi file");
    assert!((resource.some.avg10 - 1.25).abs() < f32::EPSILON);
    assert_eq!(resource.some.total_us, 50000);
    let full = resource.full.expect("full window must be present");
    assert!((full.avg10 - 0.10).abs() < f32::EPSILON);
    assert_eq!(full.total_us, 12000);
}

#[test]
fn parse_psi_file_handles_some_only_like_cpu() {
    let content = "some avg10=0.00 avg60=0.00 avg300=0.00 total=0\n";
    let resource = parse_psi_file(content).expect("must parse psi file with some only");
    assert_eq!(resource.some.total_us, 0);
    assert!(resource.full.is_none());
}

#[test]
fn malformed_or_impossible_psi_values_are_absent_not_zero() {
    assert!(parse_psi_window("some avg10=bad avg60=0 avg300=0 total=1").is_none());
    assert!(parse_psi_window("some avg10=-1 avg60=0 avg300=0 total=1").is_none());
    assert!(parse_psi_window("some avg10=101 avg60=0 avg300=0 total=1").is_none());
    assert!(parse_psi_window("some avg10=NaN avg60=0 avg300=0 total=1").is_none());
    assert!(parse_psi_window("some avg10=0 avg60=0 avg300=0 total=bad").is_none());
    assert!(parse_psi_window("some avg10=0 avg60=0 total=1").is_none());
    assert!(parse_psi_window("unknown avg10=0 avg60=0 avg300=0 total=1").is_none());
}
