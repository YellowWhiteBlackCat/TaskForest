//! Typed command, key, and modifier vocabulary.

pub(crate) mod spec;

pub use spec::{CommandId, CommandPaletteDisposition, CommandPaletteExclusionReason};

/// Commands suitable for a command-palette entry, in registry order.
///
/// Frontends may add their own local actions after these shared commands;
/// toolkit executors remain owned by each frontend.
#[must_use]
pub fn command_palette_commands() -> impl Iterator<Item = CommandId> {
    CommandId::ALL
        .into_iter()
        .filter(|command| command.palette_disposition().is_available())
}

/// Key values used by the default command map.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum KeyCode {
    F,
    /// Bare F1 toggles the help sheet (frontend-local; no router binding).
    F1,
    Digit1,
    Digit2,
    Digit3,
    Digit4,
    Digit5,
    Digit6,
    Digit7,
    /// Alt+8: the alerts-management page chord (Alt+1..7 select the shared
    /// pages; the alerts route is frontend-owned).
    Digit8,
    PageUp,
    PageDown,
    ArrowUp,
    ArrowDown,
    /// Arrow keys for tree/group expand-collapse (frontend-local; no router
    /// binding).
    ArrowLeft,
    ArrowRight,
    Tab,
    F5,
    F9,
    A,
    C,
    Delete,
    Enter,
    Escape,
    Space,
    /// Home: jump the process selection to the first visible row.
    Home,
    /// End: jump the process selection to the last visible row.
    End,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum KeyParseError {
    UnknownKey,
}

impl KeyCode {
    /// Parse a canonical, case-insensitive key name at a frontend boundary.
    pub fn parse(name: &str) -> Result<Self, KeyParseError> {
        match name.trim().to_ascii_lowercase().as_str() {
            "f" => Ok(Self::F),
            "f1" => Ok(Self::F1),
            "1" => Ok(Self::Digit1),
            "2" => Ok(Self::Digit2),
            "3" => Ok(Self::Digit3),
            "4" => Ok(Self::Digit4),
            "5" => Ok(Self::Digit5),
            "6" => Ok(Self::Digit6),
            "7" => Ok(Self::Digit7),
            "8" => Ok(Self::Digit8),
            "pageup" => Ok(Self::PageUp),
            "pagedown" => Ok(Self::PageDown),
            "arrowup" => Ok(Self::ArrowUp),
            "arrowdown" => Ok(Self::ArrowDown),
            "arrowleft" => Ok(Self::ArrowLeft),
            "arrowright" => Ok(Self::ArrowRight),
            "tab" => Ok(Self::Tab),
            "f5" => Ok(Self::F5),
            "f9" => Ok(Self::F9),
            "a" => Ok(Self::A),
            "c" => Ok(Self::C),
            "delete" => Ok(Self::Delete),
            "enter" => Ok(Self::Enter),
            "escape" => Ok(Self::Escape),
            "space" => Ok(Self::Space),
            "home" => Ok(Self::Home),
            "end" => Ok(Self::End),
            _ => Err(KeyParseError::UnknownKey),
        }
    }
}

/// Exact modifier state; companion modifiers are never silently accepted.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct Modifiers {
    pub control: bool,
    pub alt: bool,
    pub shift: bool,
    pub platform: bool,
}

impl Modifiers {
    pub const NONE: Self = Self::new(false, false, false, false);
    pub const CONTROL: Self = Self::new(true, false, false, false);
    pub const ALT: Self = Self::new(false, true, false, false);
    pub const SHIFT: Self = Self::new(false, false, true, false);

    #[must_use]
    pub const fn new(control: bool, alt: bool, shift: bool, platform: bool) -> Self {
        Self {
            control,
            alt,
            shift,
            platform,
        }
    }
}

/// One fully specified keyboard shortcut.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct KeyChord {
    pub key: KeyCode,
    pub modifiers: Modifiers,
}

impl KeyChord {
    #[must_use]
    pub const fn new(key: KeyCode, modifiers: Modifiers) -> Self {
        Self { key, modifiers }
    }

    /// Build a chord from a canonical key name and already-normalized modifiers.
    pub fn from_key_name(name: &str, modifiers: Modifiers) -> Result<Self, KeyParseError> {
        KeyCode::parse(name).map(|key| Self::new(key, modifiers))
    }
}

#[cfg(test)]
#[path = "../tests/headless/application_command_tests.rs"]
mod tests;
