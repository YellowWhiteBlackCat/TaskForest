use super::*;

#[test]
fn parses_pid_nice_thcount_rows_by_header_position() {
    // Verbatim-shaped `ps -Ao pid,nice,thcount` excerpt: right-aligned
    // numeric columns with the canonical BSD headers. Column widths are
    // irrelevant because the parser is header-driven.
    let stdout = "\
  PID  NICE     THCOUNT
    1    0           3
   42    5           9
  100   -5           1
";
    let map = parse_ps_facts_excerpt(stdout);
    assert_eq!(map.get(&1), Some(&(Some(0), Some(3))));
    assert_eq!(map.get(&42), Some(&(Some(5), Some(9))));
    // Negative nice values (higher scheduling priority) must survive.
    assert_eq!(map.get(&100), Some(&(Some(-5), Some(1))));
    assert_eq!(map.len(), 3);
}

#[test]
fn empty_output_yields_an_empty_map() {
    // No stdout at all (ps absent / produced nothing).
    assert!(parse_ps_facts_excerpt("").is_empty());
    // A whitespace-only header line is treated as missing.
    assert!(parse_ps_facts_excerpt("   \n").is_empty());
}

#[test]
fn missing_pid_header_yields_an_empty_map() {
    // Header present but no PID token -> nothing to key on, no facts.
    let stdout = "  NICE     THCOUNT\n    0           3\n";
    assert!(parse_ps_facts_excerpt(stdout).is_empty());
}

#[test]
fn missing_value_columns_yield_an_empty_map() {
    // PID header but neither NICE nor THCOUNT -> no useful facts to publish.
    let stdout = "  PID\n    1\n";
    assert!(parse_ps_facts_excerpt(stdout).is_empty());
}

#[test]
fn partial_columns_populate_only_the_available_field() {
    // `thcount` header absent: nice is read, threads degrade to None per
    // row, so the process-list threads scalar stays honestly Unsupported.
    let stdout = "  PID  NICE\n    1    0\n   42    5\n";
    let map = parse_ps_facts_excerpt(stdout);
    assert_eq!(map.get(&1), Some(&(Some(0), None)));
    assert_eq!(map.get(&42), Some(&(Some(5), None)));
}

#[test]
fn unparseable_cells_degrade_to_none_without_dropping_the_other_field() {
    // A non-numeric nice cell leaves nice=None but keeps the real threads
    // value; a fully unparseable row (both fields None) is dropped.
    let stdout = "\
  PID  NICE     THCOUNT
    1    ?           3
    2    5           -
    3    ?           -
";
    let map = parse_ps_facts_excerpt(stdout);
    assert_eq!(map.get(&1), Some(&(None, Some(3))));
    assert_eq!(map.get(&2), Some(&(Some(5), None)));
    // Row 3 had neither parseable field -> dropped (no useless PID-only row).
    assert!(!map.contains_key(&3));
}

#[test]
fn missing_pid_value_skips_the_row() {
    // A data row whose PID cell is empty/missing is skipped; the surviving
    // row is still parsed.
    let stdout = "\
  PID  NICE     THCOUNT
   42    5           9
";
    let map = parse_ps_facts_excerpt(stdout);
    assert_eq!(map.get(&42), Some(&(Some(5), Some(9))));
}
