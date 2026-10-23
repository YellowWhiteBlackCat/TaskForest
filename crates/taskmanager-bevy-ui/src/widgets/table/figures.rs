//! Tabular figures for the table's numeric values.
//!
//! A telemetry column refreshes its digits continuously. With proportional
//! figures every digit has its own width, so the value jitters sideways on
//! every refresh. The numeric values therefore force the font's tabular
//! figures feature: every digit gets the same advance, and the column stands
//! still.

use bevy::ecs::component::Component;
use bevy::ecs::query::Changed;
use bevy::ecs::system::Query;
use bevy::text::{FontFeatureTag, FontFeatures, TextFont};

/// Marker on a cell's text: a numeric value's digits must keep their width
/// while the value refreshes. The flag says whether THIS value is numeric —
/// the marker rides every cell so the scene shape stays uniform.
#[derive(Component, Clone, Copy, Debug, Default)]
pub(crate) struct TabularFigures(pub(crate) bool);

/// The font features a tabular value carries.
fn tabular_features() -> FontFeatures {
    FontFeatures::builder()
        .enable(FontFeatureTag::TABULAR_FIGURES)
        .build()
}

/// Keep the tabular-figures feature enabled on the marked values.
///
/// The type-role observer owns the font handle, size and weight, and it
/// rewrites the whole font record — so the feature is re-applied whenever the
/// font record changes. The write is conditional, so the re-application never
/// becomes a change-detection loop.
pub(crate) fn apply_tabular_figures(
    mut values: Query<(&TabularFigures, &mut TextFont), Changed<TextFont>>,
) {
    let features = tabular_features();
    for (marker, mut font) in &mut values {
        if !marker.0 {
            continue;
        }
        if font.font_features != features {
            font.font_features = features.clone();
        }
    }
}
