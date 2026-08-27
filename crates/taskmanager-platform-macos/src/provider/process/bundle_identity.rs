//! macOS application-bundle identity derived purely from the executable path.
//!
//! Bundles are identified by their documented directory layout
//! (`<Name>.app/Contents/MacOS/<executable>`); no AppKit/Foundation API and
//! no new unsafe or binding surface is involved. The functions here are
//! deliberately free of `cfg(target_os = "macos")` so the classification and
//! the three-state observation rule are compilable and testable on every
//! host; only the provider call site lives behind macOS telemetry wiring.
//!
//! Comparisons are ASCII case-insensitive because the default macOS
//! filesystems (APFS/HFS+) are case-insensitive: a path recorded by the
//! kernel as `FOO.APP/CONTENTS/MACOS/x` still resolves into the same bundle
//! on-disk, so rejecting it would under-report real applications.

use std::ffi::OsStr;
use std::path::{Component, Path};

use taskmanager_core::{ProcessApplicationIdentity, ProcessMetadataObservation};

const APP_SUFFIX: &[u8] = b".app";
const CONTENTS_DIR: &[u8] = b"Contents";
const MACOS_DIR: &[u8] = b"MacOS";

fn eq_ascii_ignore_case(component: &OsStr, literal: &[u8]) -> bool {
    component.as_encoded_bytes().eq_ignore_ascii_case(literal)
}

/// Strip a case-insensitive `.app` suffix, requiring a real name to remain.
///
/// The byte at the split position always starts a character: the matched
/// suffix bytes are pure ASCII, so no multi-byte sequence is ever split.
fn strip_app_suffix(bundle_directory: &str) -> Option<&str> {
    let stem_len = bundle_directory.len().checked_sub(APP_SUFFIX.len())?;
    if stem_len == 0 {
        return None;
    }
    if !bundle_directory.as_bytes()[stem_len..].eq_ignore_ascii_case(APP_SUFFIX) {
        return None;
    }
    bundle_directory.get(..stem_len)
}

/// Name of the innermost `<Name>.app` bundle directory owning `exe`, if the
/// path matches the documented `…/<Name>.app/Contents/MacOS/…` layout.
///
/// When several nested bundles match (a helper inside
/// `Foo.app/Contents/Bundles/Bar.app/Contents/MacOS/…`), the innermost —
/// closest to the executable — wins, because that is the bundle the process
/// was actually launched from.
fn bundle_directory_name(exe: &Path) -> Option<String> {
    let mut bundle: Option<&OsStr> = None;
    let components: Vec<Component<'_>> = exe.components().collect();
    for triple in components.windows(3) {
        let [
            Component::Normal(bundle_dir),
            Component::Normal(contents),
            Component::Normal(macos),
        ] = triple
        else {
            continue;
        };
        if has_app_suffix(bundle_dir)
            && eq_ascii_ignore_case(contents, CONTENTS_DIR)
            && eq_ascii_ignore_case(macos, MACOS_DIR)
        {
            bundle = Some(bundle_dir);
        }
    }
    bundle.map(|name| name.to_string_lossy().into_owned())
}

/// Whether the bundle directory name ends with a real `.app`-suffixed name
/// (a directory named exactly `.app` is hidden-file noise, not a bundle).
fn has_app_suffix(bundle_directory: &OsStr) -> bool {
    let bytes = bundle_directory.as_encoded_bytes();
    let Some(stem_len) = bytes.len().checked_sub(APP_SUFFIX.len()) else {
        return false;
    };
    stem_len > 0 && bytes[stem_len..].eq_ignore_ascii_case(APP_SUFFIX)
}

/// Derive a desktop-application identity from a bundle executable path.
///
/// `display_name` is the bundle directory name without the `.app` suffix
/// (`Foo.app` → `Foo`); `launcher_id` is derived from the same bundle name
/// and keeps the suffix so it can never collide with a display name. A path
/// that does not match the bundle layout — or whose bundle directory yields
/// no usable name — returns `None`.
#[must_use]
pub(crate) fn bundle_identity_from_path(exe: &Path) -> Option<ProcessApplicationIdentity> {
    let bundle_directory = bundle_directory_name(exe)?;
    let display_name = strip_app_suffix(&bundle_directory).map(str::to_owned)?;
    ProcessApplicationIdentity::new(bundle_directory, display_name, None)
}

/// The provider's complete three-state application-identity rule.
///
/// - `Some(exe)` matching the bundle layout → `available(identity, observed_at_ms)`;
/// - `Some(exe)` confirmed NOT to be a bundle executable → `absent(observed_at_ms)`
///   (an honest Background classification, never a fabricated identity);
/// - `None` (the executable path itself is unknown) → `Unknown` — the provider
///   refuses to turn a missing path into a confirmed absence.
#[must_use]
pub(crate) fn application_identity_observation(
    exe: Option<&Path>,
    observed_at_ms: u64,
) -> ProcessMetadataObservation<ProcessApplicationIdentity> {
    match exe {
        Some(path) => match bundle_identity_from_path(path) {
            Some(identity) => ProcessMetadataObservation::available(identity, observed_at_ms),
            None => ProcessMetadataObservation::absent(observed_at_ms),
        },
        None => ProcessMetadataObservation::default(),
    }
}

#[cfg(test)]
#[path = "../../../tests/headless/macos_provider_process_bundle_identity.rs"]
mod tests;
