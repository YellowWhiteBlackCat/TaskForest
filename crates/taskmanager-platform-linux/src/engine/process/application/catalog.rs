//! Desktop-entry discovery, parsing, and executable matching rules.

use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::{Path, PathBuf};

use super::*;

#[derive(Debug, PartialEq, Eq)]
pub(super) struct ParsedDesktopEntry {
    pub(super) executable: String,
    pub(super) exec_args: Vec<String>,
    pub(super) snap_package: Option<String>,
    pub(super) identity: ProcessApplicationIdentity,
}

pub(super) fn load_catalog_from_dirs(
    data_dirs: &[PathBuf],
) -> (Vec<CatalogEntry>, Option<ProcessMetadataFailure>) {
    let mut entries_by_id = HashMap::new();
    let mut failure = None;
    let mut file_count = 0;
    let mut limit_reached = false;

    for directory in data_dirs {
        if limit_reached {
            break;
        }
        let applications = directory.join("applications");
        let read_dir = match fs::read_dir(&applications) {
            Ok(read_dir) => read_dir,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => continue,
            Err(error) => {
                record_failure(&mut failure, classify_io(&error));
                continue;
            }
        };
        for entry in read_dir {
            let entry = match entry {
                Ok(entry) => entry,
                Err(error) => {
                    record_failure(&mut failure, classify_io(&error));
                    continue;
                }
            };
            if file_count >= MAX_DESKTOP_FILES {
                record_failure(&mut failure, ProcessMetadataFailure::ProviderFault);
                limit_reached = true;
                break;
            }
            let path = entry.path();
            if path.extension().and_then(|extension| extension.to_str()) != Some("desktop") {
                continue;
            }
            file_count += 1;
            let metadata = match entry.metadata() {
                Ok(metadata) => metadata,
                Err(error) => {
                    record_failure(&mut failure, classify_io(&error));
                    continue;
                }
            };
            if metadata.len() > MAX_DESKTOP_FILE_BYTES {
                record_failure(&mut failure, ProcessMetadataFailure::ProviderFault);
                continue;
            }
            let text = match fs::read_to_string(&path) {
                Ok(text) => text,
                Err(error) => {
                    record_failure(&mut failure, classify_io(&error));
                    continue;
                }
            };
            let Some(desktop_id) = path.file_name().and_then(|name| name.to_str()) else {
                record_failure(&mut failure, ProcessMetadataFailure::ProviderFault);
                continue;
            };
            let Some(parsed) = parse_desktop_entry(&text, desktop_id) else {
                continue;
            };
            let Some(executable) =
                executable_selector(&parsed.executable, parsed.snap_package.as_deref())
            else {
                continue;
            };
            let id = parsed.identity.launcher_id.clone();
            // XDG order gives the user's entry precedence.
            entries_by_id.entry(id).or_insert(CatalogEntry {
                identity: parsed.identity,
                executable,
                exec_args: parsed.exec_args,
            });
        }
    }

    let mut entries: Vec<_> = entries_by_id.into_values().collect();
    entries.sort_by(|left, right| left.identity.launcher_id.cmp(&right.identity.launcher_id));
    (entries, failure)
}

pub(super) fn application_dirs() -> Vec<PathBuf> {
    let mut directories = Vec::new();
    let mut seen = HashSet::new();
    let user_data = std::env::var_os("XDG_DATA_HOME")
        .filter(|value| !value.is_empty())
        .map(PathBuf::from)
        .or_else(|| {
            std::env::var_os("HOME")
                .filter(|value| !value.is_empty())
                .map(|home| PathBuf::from(home).join(".local/share"))
        });
    if let Some(directory) = user_data.filter(|directory| seen.insert(directory.clone())) {
        directories.push(directory);
    }

    let system_data = std::env::var_os("XDG_DATA_DIRS")
        .filter(|dirs| !dirs.is_empty())
        .map(|dirs| dirs.to_string_lossy().into_owned())
        .unwrap_or_else(|| "/usr/local/share:/usr/share".to_owned());
    for directory in system_data
        .split(':')
        .filter(|directory| !directory.is_empty())
    {
        let directory = PathBuf::from(directory);
        if seen.insert(directory.clone()) {
            directories.push(directory);
        }
    }
    directories
}

pub(super) fn parse_desktop_entry(text: &str, desktop_id: &str) -> Option<ParsedDesktopEntry> {
    let mut in_desktop_entry = false;
    let mut kind = None;
    let mut hidden = false;
    let mut no_display = false;
    let mut name = None;
    let mut exec = None;
    let mut icon = None;
    for line in text.lines() {
        let line = line.trim();
        if let Some(group) = line
            .strip_prefix('[')
            .and_then(|line| line.strip_suffix(']'))
        {
            in_desktop_entry = group == "Desktop Entry";
            continue;
        }
        if !in_desktop_entry || line.is_empty() || line.starts_with('#') {
            continue;
        }
        let Some((key, value)) = line.split_once('=') else {
            continue;
        };
        let value = value.trim();
        match key.trim() {
            "Type" => kind = Some(value),
            "Hidden" => hidden = parse_desktop_bool(value),
            "NoDisplay" => no_display = parse_desktop_bool(value),
            "Name" => name = (!value.is_empty()).then(|| value.to_owned()),
            "Exec" => exec = (!value.is_empty()).then(|| value.to_owned()),
            "Icon" => icon = (!value.is_empty()).then(|| value.to_owned()),
            _ => {}
        }
    }
    if kind.is_some_and(|kind| !kind.eq_ignore_ascii_case("Application")) || hidden || no_display {
        return None;
    }
    let name = name?;
    let (executable, exec_args, snap_package) = parse_exec(&exec?)?;
    let launcher_id = desktop_entry_id(desktop_id)?;
    let identity = ProcessApplicationIdentity::new(&launcher_id, name, icon)?;
    Some(ParsedDesktopEntry {
        executable,
        exec_args,
        snap_package,
        identity,
    })
}

pub(super) fn desktop_entry_id(file_name: &str) -> Option<String> {
    let id = file_name
        .strip_suffix(".desktop")
        .unwrap_or(file_name)
        .trim();
    (!id.is_empty()).then(|| id.to_owned())
}

pub(super) fn parse_desktop_bool(value: &str) -> bool {
    value.eq_ignore_ascii_case("true") || value == "1"
}

/// Parse the freedesktop `Exec=` quoting subset as an argv template.
pub(super) fn tokenize_exec(value: &str) -> Option<Vec<String>> {
    let mut tokens = Vec::new();
    let mut token = String::new();
    let mut quoted = None;
    let mut escaped = false;
    let mut has_token = false;

    for character in value.chars() {
        if escaped {
            token.push(character);
            has_token = true;
            escaped = false;
            continue;
        }
        match quoted {
            Some(quote) if character == quote => quoted = None,
            Some('\'') if character == '"' => token.push(character),
            Some('"') if character == '\'' => token.push(character),
            Some(_) => {
                if character == '\\' {
                    escaped = true;
                } else {
                    token.push(character);
                    has_token = true;
                }
            }
            None if character == '\'' || character == '"' => quoted = Some(character),
            None if character == '\\' => escaped = true,
            None if character.is_whitespace() => {
                if has_token {
                    tokens.push(std::mem::take(&mut token));
                    has_token = false;
                }
            }
            None => {
                token.push(character);
                has_token = true;
            }
        }
    }
    if quoted.is_some() || escaped {
        return None;
    }
    if has_token {
        tokens.push(token);
    }
    (!tokens.is_empty()).then_some(tokens)
}

pub(super) fn parse_exec(exec: &str) -> Option<(String, Vec<String>, Option<String>)> {
    let tokens = tokenize_exec(exec)?;
    let mut index = 0;
    while index < tokens.len() {
        let token = &tokens[index];
        let basename = Path::new(token)
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or_default()
            .to_ascii_lowercase();

        if basename == "env" {
            index += 1;
            while let Some(option) = tokens.get(index) {
                if option.starts_with('-') || (option.contains('=') && !option.starts_with('/')) {
                    index += 1;
                } else {
                    break;
                }
            }
            continue;
        }
        if basename == "command" || basename == "exec" {
            index += 1;
            continue;
        }
        if (basename == "snap" || basename == "flatpak")
            && tokens.get(index + 1).is_some_and(|arg| arg == "run")
        {
            if basename == "flatpak" {
                // The host launcher does not identify the sandbox executable.
                return None;
            }
            let app_index = tokens
                .iter()
                .enumerate()
                .skip(index + 2)
                .find(|(_, arg)| !arg.starts_with('-'))
                .map(|(index, _)| index)?;
            let app = tokens.get(app_index)?.clone();
            let args = tokens.get(app_index + 1..).unwrap_or_default().to_vec();
            let package = snap_package_key(&app)?;
            return Some((app, args, Some(package)));
        }
        if token.starts_with('-') || (token.contains('=') && !token.starts_with('/')) {
            index += 1;
            continue;
        }
        return Some((
            token.clone(),
            tokens.get(index + 1..).unwrap_or_default().to_vec(),
            None,
        ));
    }
    None
}

pub(super) fn executable_selector(
    executable: &str,
    snap_package: Option<&str>,
) -> Option<ExecutableSelector> {
    let basename = executable_key_from_path(Path::new(executable))?;
    let path = normalize_executable_path(Path::new(executable));
    let snap_package = snap_package
        .map(str::to_owned)
        .or_else(|| path.as_deref().and_then(snap_package_from_path));
    Some(ExecutableSelector {
        path,
        basename,
        snap_package,
    })
}

pub(super) fn executable_key_from_path(path: &Path) -> Option<String> {
    let filename = path.file_name()?.to_str()?;
    let value = filename.strip_suffix(" (deleted)").unwrap_or(filename);
    (!value.is_empty()).then(|| value.to_ascii_lowercase())
}

pub(super) fn normalize_executable_path(path: &Path) -> Option<String> {
    let value = path.to_str()?.trim();
    let value = value.strip_suffix(" (deleted)").unwrap_or(value);
    value.starts_with('/').then(|| value.to_owned())
}

pub(super) fn executable_match_score(
    selector: &ExecutableSelector,
    process_path: Option<&str>,
    process_basename: &str,
    snap_package: Option<&str>,
) -> Option<u8> {
    if selector.path.as_deref() == process_path {
        return Some(4);
    }
    if let Some(mount_label) = process_path.and_then(appimage_mount_label) {
        return selector
            .path
            .as_deref()
            .and_then(appimage_stem)
            .filter(|stem| appimage_mount_matches(stem, mount_label))
            .map(|_| 3);
    }
    if let Some(package) = snap_package {
        return selector
            .snap_package
            .as_deref()
            .filter(|expected| *expected == package)
            .map(|_| 3);
    }
    if selector.snap_package.is_some() || selector.path.as_deref().and_then(appimage_stem).is_some()
    {
        return None;
    }
    same_executable_name(&selector.basename, process_basename).then_some(2)
}

pub(super) fn appimage_stem(path: &str) -> Option<String> {
    let name = Path::new(path).file_name()?.to_str()?;
    let lower = name.to_ascii_lowercase();
    let stem = lower.strip_suffix(".appimage")?;
    (!stem.is_empty()).then(|| stem.to_owned())
}

pub(super) fn appimage_mount_label(path: &str) -> Option<&str> {
    Path::new(path).components().find_map(|component| {
        let value = component.as_os_str().to_str()?;
        value
            .strip_prefix(".mount-")
            .or_else(|| value.strip_prefix(".mount_"))
            .filter(|label| !label.is_empty())
    })
}

pub(super) fn appimage_mount_matches(stem: &str, label: &str) -> bool {
    let label = label.to_ascii_lowercase();
    if label == stem {
        return true;
    }
    let Some(suffix) = label.strip_prefix(stem) else {
        return false;
    };
    suffix.starts_with('-')
        || suffix.starts_with('_')
        || (suffix.len() >= 6 && suffix.bytes().all(|byte| byte.is_ascii_alphanumeric()))
}

pub(super) fn snap_package_key(value: &str) -> Option<String> {
    let value = value.trim();
    (!value.is_empty() && !value.starts_with('-')).then(|| value.to_ascii_lowercase())
}

pub(super) fn snap_package_from_path(path: &str) -> Option<String> {
    let mut parts = path.strip_prefix("/snap/")?.split('/');
    let first = parts.next()?;
    if first == "bin" {
        return snap_package_key(parts.next()?);
    }
    let revision = parts.next()?;
    let valid_revision = revision == "current"
        || (!revision.is_empty() && revision.bytes().all(|byte| byte.is_ascii_digit()));
    valid_revision.then(|| first.to_ascii_lowercase())
}

pub(super) fn snap_package_from_argv(argv: &[std::borrow::Cow<'_, str>]) -> Option<String> {
    argv.windows(3).find_map(|window| {
        let command = Path::new(window[0].as_ref()).file_name()?.to_str()?;
        (command.eq_ignore_ascii_case("snap") && window[1].as_ref() == "run")
            .then(|| snap_package_key(window[2].as_ref()))
            .flatten()
    })
}

pub(super) fn same_executable_name(left: &str, right: &str) -> bool {
    left == right
        || executable_aliases(left).contains(&right)
        || executable_aliases(right).contains(&left)
        || chromium_family(left).is_some_and(|family| chromium_family(right) == Some(family))
}

pub(super) fn chromium_family(name: &str) -> Option<&'static str> {
    match name {
        "chrome"
        | "google-chrome"
        | "google-chrome-stable"
        | "google-chrome-beta"
        | "google-chrome-unstable" => Some("chrome"),
        "chromium" | "chromium-browser" => Some("chromium"),
        _ => None,
    }
}

/// Narrow aliases for known wrapper names; this is not a generic name fallback.
pub(super) fn executable_aliases(name: &str) -> &'static [&'static str] {
    match name {
        "firefox-bin" => &["firefox"],
        "oosplash" | "soffice.bin" => &["libreoffice"],
        "resources-processes" => &["resources"],
        "gnome-terminal-server" => &["gnome-terminal"],
        _ => &[],
    }
}
