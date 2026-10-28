//! Feature semantic specifications: what counts as delivered.
//!
//! Every [`FeatureId`] carries one explicit, toolkit-neutral
//! [`FeatureSemanticSpec`] ([`FeatureId::semantic_spec`]) defining the
//! user-facing surface a frontend must render before it may claim
//! `Reference`/`Ported`/`Native`. The type and its fold live here so the
//! coverage registry stays within the source line budget.

mod definitions;

use super::FeatureId;

/// Explicit, toolkit-neutral semantic specification for a product feature.
///
/// It defines what "delivered" means for the shared coverage registry: the
/// user-facing surface a frontend must render before it may claim
/// [`crate::capabilities::CapabilitySupport::Reference`],
/// [`crate::capabilities::CapabilitySupport::Ported`], or
/// [`crate::capabilities::CapabilitySupport::Native`]. A narrower surface
/// declares [`crate::capabilities::CapabilitySupport::Divergent`] with the
/// missing part; an absent one declares
/// [`crate::capabilities::CapabilitySupport::Unsupported`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct FeatureSemanticSpec {
    /// The feature described by this specification.
    pub feature: FeatureId,
    /// The shared surface that counts as delivered for this feature.
    pub delivery_definition: &'static str,
}

impl FeatureId {
    /// The explicit, toolkit-neutral semantic specification defining what
    /// counts as delivered for this feature.
    ///
    /// A frontend declaration is honest only when its status matches this
    /// definition: a surface that renders less than the specification must
    /// declare [] with the
    /// missing part, and an absent one
    /// [].
    #[must_use]
    pub const fn semantic_spec(self) -> FeatureSemanticSpec {
        FeatureSemanticSpec {
            feature: self,
            delivery_definition: definitions::delivery_definition(self),
        }
    }
}
