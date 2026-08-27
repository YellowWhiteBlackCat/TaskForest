use super::*;

fn sample(revision: u64, value: Option<f64>) -> HistoricalSample {
    HistoricalSample {
        revision,
        completed_at_ms: revision * 100,
        measured_at_ms: Some(revision * 100),
        value,
    }
}

#[test]
fn records_round_trip_through_jsonl_lines() {
    for sample in [sample(1, Some(42.5)), sample(2, None)] {
        let line = encode_line(&sample);
        assert!(!line.contains('\n'));
        assert_eq!(decode_line(&line), Some(sample));
    }
    assert_eq!(decode_line(""), None);
    assert_eq!(decode_line("not json"), None);
    assert_eq!(decode_line("{\"r\":\"x\"}"), None);
}

#[test]
fn gap_values_serialize_as_null_not_zero() {
    let line = encode_line(&sample(3, None));
    assert!(line.contains("\"v\":null"), "gaps must stay null: {line}");
    assert!(
        !line.contains("\"v\":0"),
        "a gap must not read back as zero: {line}"
    );
}
