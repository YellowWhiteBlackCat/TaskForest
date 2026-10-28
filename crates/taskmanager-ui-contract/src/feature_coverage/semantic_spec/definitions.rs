//! Feature semantic specification delivery definitions.

mod batch_a;
mod batch_b;
mod batch_c;

use crate::feature_coverage::FeatureId;

/// Single authority for what counts as delivered for each feature.
#[must_use]
pub const fn delivery_definition(feature: FeatureId) -> &'static str {
    if let Some(def) = batch_a::delivery_definition(feature) {
        def
    } else if let Some(def) = batch_b::delivery_definition(feature) {
        def
    } else {
        batch_c::delivery_definition(feature)
    }
}
