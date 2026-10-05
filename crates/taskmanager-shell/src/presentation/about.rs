//! Application build metadata, independent from system-information facts.

use taskmanager_application::i18n::t;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AboutMetadata<'a> {
    pub name: &'static str,
    pub description: &'static str,
    pub version: &'a str,
    pub license: &'static str,
    pub repository: &'static str,
}

#[must_use]
pub fn metadata<'a>(
    version: &'a str,
    license: &'static str,
    repository: &'static str,
) -> AboutMetadata<'a> {
    AboutMetadata {
        name: t("about.name"),
        description: t("about.description"),
        version,
        license,
        repository,
    }
}

impl AboutMetadata<'_> {
    #[must_use]
    pub fn details_text(&self) -> String {
        format!(
            "{}\n{}: {}\n{}: {}\n{}: {}",
            self.name,
            t("about.version"),
            self.version,
            t("about.license"),
            self.license,
            t("about.repository"),
            self.repository
        )
    }
}

#[cfg(test)]
#[path = "../../tests/headless/presentation/about_tests.rs"]
mod tests;
