//! test-intent: behavior
//!
//! Unit test for the shared controls' pure identity helper: the device-sidebar
//! title bound keeps a clipped row ending in an explicit ellipsis instead of a
//! truncated opening parenthesis.

use super::bounded_device_title;

#[test]
fn device_sidebar_title_uses_an_explicit_ellipsis_before_clipping() {
    assert_eq!(bounded_device_title("CPU"), "CPU");
    assert_eq!(
        bounded_device_title("Intel Graphics (xe)"),
        "Intel Graphics…"
    );
    assert_eq!(
        bounded_device_title("Intel Graphics (xe) integrated extra"),
        "Intel Graphics…"
    );
}
