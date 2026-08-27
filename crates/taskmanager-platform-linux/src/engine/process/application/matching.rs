//! Deterministic argv disambiguation for shared desktop executables.

use super::CatalogEntry;

pub(super) fn select_candidate<'a>(
    candidates: Vec<(&'a CatalogEntry, u8)>,
    argv: &[std::borrow::Cow<'_, str>],
) -> Option<&'a CatalogEntry> {
    let has_required_args = candidates
        .iter()
        .any(|(entry, _)| required_args(entry).next().is_some());
    // An unreadable argv cannot resolve a shared browser/PWA bucket.
    if argv.is_empty() && has_required_args {
        return None;
    }

    let mut best = None;
    let mut best_score = (0_usize, 0_u8);
    let mut tied = false;
    for (candidate, executable_score) in candidates {
        let required = required_args(candidate).collect::<Vec<_>>();
        if !required
            .iter()
            .all(|argument| argument_matches(argument, argv))
        {
            continue;
        }
        let score = (required.len(), executable_score);
        match best {
            None => {
                best = Some(candidate);
                best_score = score;
                tied = false;
            }
            Some(_) if score > best_score => {
                best = Some(candidate);
                best_score = score;
                tied = false;
            }
            Some(_) if score == best_score => tied = true,
            Some(_) => {}
        }
    }
    (!tied).then_some(best).flatten()
}

fn argument_matches(required: &str, argv: &[std::borrow::Cow<'_, str>]) -> bool {
    if argv.iter().any(|value| value.as_ref() == required) {
        return true;
    }
    let Some((flag, value)) = required.split_once('=') else {
        return false;
    };
    flag.starts_with("--")
        && !value.is_empty()
        && argv
            .windows(2)
            .any(|pair| pair[0].as_ref() == flag && pair[1].as_ref() == value)
}

fn required_args(entry: &CatalogEntry) -> impl Iterator<Item = &str> {
    entry
        .exec_args
        .iter()
        .map(String::as_str)
        .filter(|argument| !is_field_code(argument))
}

fn is_field_code(token: &str) -> bool {
    let bytes = token.as_bytes();
    bytes.len() == 2 && bytes[0] == b'%'
}
