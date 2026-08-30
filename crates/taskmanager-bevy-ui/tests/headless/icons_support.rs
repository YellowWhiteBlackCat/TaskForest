//! Test-side plate-resolution counter for the icon registry.

use super::IconPlates;

impl IconPlates {
    /// How many semantic ids resolved to a drawable plate.
    #[must_use]
    pub(crate) fn resolved(&self) -> usize {
        self.plates.len()
    }
}
