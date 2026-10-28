//! Direct-track Services viewing state.

/// The Services page's search query for a direct-track window.
///
/// The matching rule is [`crate::matches_service_query`]; this struct only
/// owns the text, mirroring [`super::ProcessViewing`]'s query slot on the
/// composed track. GPUI renders its own search box but never keeps a second
/// copy of this value in a view field.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ServiceViewing {
    query: String,
}

impl ServiceViewing {
    /// The raw query (trimmed at the projection boundary).
    #[must_use]
    pub fn query(&self) -> &str {
        &self.query
    }

    /// Replace the whole query (a search box reports absolute text).
    pub fn set_query(&mut self, query: impl Into<String>) {
        self.query = query.into();
    }
}
