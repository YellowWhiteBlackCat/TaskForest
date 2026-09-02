//! Reviewable, privacy-safe diagnostic bundles.
//!
//! Callers provide named text sources. [`DiagnosticBundlePlan::prepare`] removes
//! usernames, filesystem paths, and IP addresses before retaining any content.
//! The returned preview and the eventual export share the same sanitized bytes,
//! so confirming a preview cannot write different or newly-collected data.

use std::net::IpAddr;

use serde::{Deserialize, Serialize};

mod error;

pub use error::{DiagnosticBundleError, DiagnosticBundleErrorKind};

const PREVIEW_CHARS: usize = 800;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DiagnosticSource {
    /// Logical bundle name, not a host filesystem path.
    pub name: String,
    pub contents: String,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct RedactionSummary {
    pub usernames: usize,
    pub paths: usize,
    pub ipv4_addresses: usize,
    pub ipv6_addresses: usize,
}

impl RedactionSummary {
    pub fn total(self) -> usize {
        self.usernames + self.paths + self.ipv4_addresses + self.ipv6_addresses
    }

    fn add_assign(&mut self, other: Self) {
        self.usernames += other.usernames;
        self.paths += other.paths;
        self.ipv4_addresses += other.ipv4_addresses;
        self.ipv6_addresses += other.ipv6_addresses;
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DiagnosticPreviewFile {
    pub name: String,
    pub bytes: usize,
    /// A bounded excerpt of already-sanitized text.
    pub excerpt: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DiagnosticPreview {
    pub files: Vec<DiagnosticPreviewFile>,
    pub total_bytes: usize,
    pub redactions: RedactionSummary,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
struct SanitizedDiagnosticFile {
    name: String,
    contents: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DiagnosticBundlePlan {
    version: u8,
    files: Vec<SanitizedDiagnosticFile>,
    preview: DiagnosticPreview,
}

impl DiagnosticBundlePlan {
    pub fn prepare(
        sources: Vec<DiagnosticSource>,
        usernames: impl IntoIterator<Item = String>,
    ) -> Result<Self, DiagnosticBundleError> {
        let mut usernames: Vec<String> = usernames
            .into_iter()
            .filter(|name| !name.trim().is_empty())
            .collect();
        usernames.sort_by_key(|name| std::cmp::Reverse(name.len()));
        usernames.dedup();

        let mut names = std::collections::HashSet::new();
        let mut files = Vec::with_capacity(sources.len());
        let mut previews = Vec::with_capacity(sources.len());
        let mut total_bytes = 0usize;
        let mut redactions = RedactionSummary::default();

        for source in sources {
            validate_source_name(&source.name)?;
            if !names.insert(source.name.clone()) {
                return Err(DiagnosticBundleError::with_detail(
                    DiagnosticBundleErrorKind::InvalidSource,
                    format!("duplicate diagnostic source: {}", source.name),
                ));
            }
            let (contents, source_redactions) = redact_text(&source.contents, &usernames);
            let bytes = contents.len();
            total_bytes = total_bytes.saturating_add(bytes);
            redactions.add_assign(source_redactions);
            let mut excerpt: String = contents.chars().take(PREVIEW_CHARS).collect();
            if contents.chars().count() > PREVIEW_CHARS {
                excerpt.push('…');
            }
            previews.push(DiagnosticPreviewFile {
                name: source.name.clone(),
                bytes,
                excerpt,
            });
            files.push(SanitizedDiagnosticFile {
                name: source.name,
                contents,
            });
        }

        Ok(Self {
            version: 1,
            files,
            preview: DiagnosticPreview {
                files: previews,
                total_bytes,
                redactions,
            },
        })
    }

    pub fn preview(&self) -> &DiagnosticPreview {
        &self.preview
    }

    /// Borrow the already-sanitized contents of one logical source. The
    /// redaction algorithm and file storage remain private; consumers that
    /// need clipboard/report text must read this accessor rather than treating
    /// the encoded wire document as an internal field API.
    #[must_use]
    pub fn sanitized_contents(&self, name: &str) -> Option<&str> {
        self.files
            .iter()
            .find(|file| file.name == name)
            .map(|file| file.contents.as_str())
    }

    /// Serialize only sanitized content. No recollection or re-redaction occurs.
    pub fn encoded(&self) -> Result<Vec<u8>, DiagnosticBundleError> {
        self.encoded_with(serde_json::to_vec_pretty)
    }

    fn encoded_with(
        &self,
        encode: impl FnOnce(&Self) -> Result<Vec<u8>, serde_json::Error>,
    ) -> Result<Vec<u8>, DiagnosticBundleError> {
        encode(self).map_err(DiagnosticBundleError::encode)
    }
}

fn validate_source_name(name: &str) -> Result<(), DiagnosticBundleError> {
    if name.is_empty()
        || name.len() > 96
        || name == "."
        || name == ".."
        || name.contains('/')
        || name.contains('\\')
        || name.chars().any(char::is_control)
    {
        return Err(DiagnosticBundleError::with_detail(
            DiagnosticBundleErrorKind::InvalidSource,
            format!("invalid diagnostic source name: {name:?}"),
        ));
    }
    Ok(())
}

fn is_word_char(character: Option<char>) -> bool {
    character.is_some_and(|c| c.is_alphanumeric() || c == '_' || c == '-')
}

fn replace_username(text: &str, username: &str) -> (String, usize) {
    let mut output = String::with_capacity(text.len());
    let mut remainder = text;
    let mut count = 0;
    while let Some(index) = remainder.find(username) {
        let before = remainder[..index].chars().next_back();
        let end = index + username.len();
        let after = remainder[end..].chars().next();
        output.push_str(&remainder[..index]);
        if !is_word_char(before) && !is_word_char(after) {
            output.push_str("<redacted-user>");
            count += 1;
        } else {
            output.push_str(username);
        }
        remainder = &remainder[end..];
    }
    output.push_str(remainder);
    (output, count)
}

fn path_start(chars: &[char], index: usize) -> bool {
    let current = chars[index];
    if current == '/' {
        let previous = index.checked_sub(1).and_then(|i| chars.get(i)).copied();
        let next = chars.get(index + 1).copied();
        let token_start = chars[..index]
            .iter()
            .rposition(|character| character.is_whitespace())
            .map_or(0, |position| position + 1);
        let inside_url = chars[token_start..index]
            .windows(3)
            .any(|window| window == [':', '/', '/']);
        if next == Some('/') {
            return !inside_url
                && previous != Some(':')
                && previous != Some('/')
                && previous.is_none_or(|character| {
                    character.is_whitespace()
                        || matches!(character, '=' | '(' | '[' | '{' | '"' | '\'' | ',' | ';')
                });
        }
        return !inside_url && previous != Some(':') && previous != Some('/') && next != Some('/');
    }
    if current == '\\' && chars.get(index + 1) == Some(&'\\') {
        let previous = index.checked_sub(1).and_then(|i| chars.get(i)).copied();
        return previous.is_none_or(|character| {
            character.is_whitespace()
                || matches!(character, '=' | '(' | '[' | '{' | '"' | '\'' | ',' | ';')
        });
    }
    if current == '~' && chars.get(index + 1) == Some(&'/') {
        return true;
    }
    current.is_ascii_alphabetic()
        && index
            .checked_sub(1)
            .and_then(|position| chars.get(position))
            .is_none_or(|previous| !previous.is_alphanumeric())
        && chars.get(index + 1) == Some(&':')
        && matches!(chars.get(index + 2), Some('/' | '\\'))
}

fn is_path_end(character: char) -> bool {
    character.is_whitespace() || matches!(character, '"' | '\'' | ',' | ';' | ')' | ']' | '}')
}

fn redact_paths(text: &str) -> (String, usize) {
    let chars: Vec<char> = text.chars().collect();
    let mut output = String::with_capacity(text.len());
    let mut index = 0;
    let mut count = 0;
    while index < chars.len() {
        if path_start(&chars, index) {
            output.push_str("<redacted-path>");
            count += 1;
            index += 1;
            while index < chars.len() && !is_path_end(chars[index]) {
                index += 1;
            }
        } else {
            output.push(chars[index]);
            index += 1;
        }
    }
    (output, count)
}

fn ip_delimiter(character: char) -> bool {
    character.is_whitespace()
        || matches!(
            character,
            '"' | '\'' | ',' | ';' | '(' | ')' | '[' | ']' | '{' | '}'
        )
}

fn redact_ip_addresses(text: &str) -> (String, RedactionSummary) {
    let chars: Vec<char> = text.chars().collect();
    let mut output = String::with_capacity(text.len());
    let mut index = 0;
    let mut summary = RedactionSummary::default();
    while index < chars.len() {
        if ip_delimiter(chars[index]) {
            output.push(chars[index]);
            index += 1;
            continue;
        }
        let start = index;
        while index < chars.len() && !ip_delimiter(chars[index]) {
            index += 1;
        }
        let token: String = chars[start..index].iter().collect();
        let (leading, candidate, trailing) = trim_ip_token(&token);
        if let Ok(address) = candidate.parse::<IpAddr>() {
            output.push_str(leading);
            match address {
                IpAddr::V4(_) => {
                    output.push_str("<redacted-ipv4>");
                    summary.ipv4_addresses += 1;
                }
                IpAddr::V6(_) => {
                    output.push_str("<redacted-ipv6>");
                    summary.ipv6_addresses += 1;
                }
            }
            output.push_str(trailing);
        } else {
            output.push_str(&token);
        }
    }
    (output, summary)
}

fn trim_ip_token(token: &str) -> (&str, &str, &str) {
    let allowed =
        |character: char| character.is_ascii_hexdigit() || character == '.' || character == ':';
    let mut run_start = None;
    for (index, character) in token
        .char_indices()
        .chain(std::iter::once((token.len(), ' ')))
    {
        if allowed(character) {
            run_start.get_or_insert(index);
            continue;
        }
        if let Some(start) = run_start.take() {
            let candidate = &token[start..index];
            if candidate.parse::<IpAddr>().is_ok() {
                return (&token[..start], candidate, &token[index..]);
            }
        }
    }
    (token, "", "")
}

fn redact_text(text: &str, usernames: &[String]) -> (String, RedactionSummary) {
    // Paths go first so a username embedded in `/home/<user>/...` is counted as
    // one path redaction rather than splitting the path around a replacement.
    let (mut output, path_count) = redact_paths(text);
    let mut summary = RedactionSummary {
        paths: path_count,
        ..RedactionSummary::default()
    };
    for username in usernames {
        let (next, count) = replace_username(&output, username);
        output = next;
        summary.usernames += count;
    }
    let (output, ip_summary) = redact_ip_addresses(&output);
    summary.add_assign(ip_summary);
    (output, summary)
}

#[cfg(test)]
#[path = "../../tests/headless/diagnostics.rs"]
mod tests;

#[cfg(test)]
#[path = "../../tests/headless/core_core_diagnostics_diagnostics_gap_tests.rs"]
mod diagnostics_gap_tests;
