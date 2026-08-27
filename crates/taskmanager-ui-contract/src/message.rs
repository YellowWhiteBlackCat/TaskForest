//! Typed localization keys for command presentation.

use taskmanager_application::CommandId;

/// A typed key into a frontend's localization catalog. Command copy is
/// keyed directly by the application's [`CommandId`] — there is no mirror
/// enum; the key strings live in the command spec table and the catalogs.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum MessageKey {
    CommandLabel(CommandId),
    CommandDescription(CommandId),
}
