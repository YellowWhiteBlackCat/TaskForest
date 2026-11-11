//! The toolkit-neutral presentation contract: the layer ABOVE the semantic
//! contracts that decides what every shape must AGREE on about measurement,
//! and the registry that turns each deliberate difference into a
//! reason-bearing, gate-checked fact.
//!
//! The line this layer draws is the same line the semantic layer draws
//! elsewhere: intent, metric, and classification are shared ("求同"); units,
//! deduction, and realization belong to the shape ("存异"). A frontend supplies
//! what only it can know — which chrome it actually spends, and which length
//! type it paints with — and consumes the verdict. It never re-derives a bucket,
//! a padding, a rung, an admission rule, a control measurement, or a motion
//! class. Prose that claims another shape is "at parity" is a defect under
//! ARCH § 8.3; parity here is computed by [`parity::unexplained_differences`]
//! against the bands registered in [`thresholds`].
//!
//! Layering inside this module is one-way:
//!
//! ```text
//! vocabulary   the buckets, folds and shared bands (no per-shape numbers)
//!    ↓
//! thresholds   each shape's registered bands + the debt census
//!    ↓
//! budget       measured slot → typed page and slot allocation
//!    ↓
//! card_flow    whole-group admission + card height clamp (one arithmetic)
//!    ↓
//! card_width   whole-column admission + header fact group (one arithmetic)
//!    ↓
//! dashboard    the product metric count + the paged metric window
//!    ↓
//! parity       the probe grid and the unexplained-difference rule
//! ```
//!
//! Measurement and motion ride the same law: [`sizes`] owns the control-role
//! matrix, [`motion`] owns the motion-role matrix, and `taskmanager-theme`
//! owns the numbers those roles resolve to (ADR-063 keeps every design token
//! with one owner). The numbers enter this layer as plain value points at
//! each frontend's binding seam, so the publishable contract crate carries
//! zero token-codec dependency, and a cross-crate parity gate welds the two
//! numeric tables so neither can drift.

pub mod budget;
pub mod card_flow;
pub mod card_width;
pub mod dashboard;
pub mod load_tiers;
pub mod motion;
pub mod parity;
pub mod resolution;
pub mod settings;
pub mod sizes;
pub mod thresholds;
pub mod vocabulary;
