//! Headless tests for the percent vocabulary's shared constants.

use super::*;

#[test]
fn max_percent_is_the_ceiling_percent_series_project_against() {
    assert_eq!(MAX_PERCENT, 100.0);
}
