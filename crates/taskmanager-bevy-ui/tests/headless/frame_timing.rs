//! test-intent: behavior
//!
//! The product's frame-timing binding names this product's environment and
//! nothing else: the plugin, the systems, the recorder and the wire record
//! are owned by `zhuge-ui-kit-bevy::frame_timing`.
//!
//! The environment constructor is a pure function of the process environment,
//! so its contract is assertable without a window, a frame loop, or a file.

use super::*;

#[test]
fn the_binding_names_this_products_own_environment_variables() {
    let env = env();
    assert_eq!(env.sink_var, "TM_FRAME_TIMING_SINK");
    assert_eq!(env.budget_var, "TM_FRAME_TIMING_BUDGET_MS");
    assert_eq!(env.identity.product, "taskforest");
    assert_eq!(env.identity.target, "taskforest-bevy");
    assert_eq!(env.identity.frontend, "bevy");
}

#[test]
fn the_binding_leaves_the_shared_numbers_to_the_owner() {
    // The budget and the sample ceiling are shared facts, so they come from
    // one place rather than being re-spelled per product.
    let env = env();
    assert_eq!(env.default_budget_ms, 16.0);
    assert_eq!(env.max_samples, 240);
}
