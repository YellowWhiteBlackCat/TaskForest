//! Bounded freedesktop icon-theme resolution for desktop application entries.

use std::collections::HashSet;
use std::fs;
use std::path::{Component, Path, PathBuf};

use taskmanager_core::{
    ApplicationIconAsset, ApplicationIconFormat, MAX_APPLICATION_ICON_BYTES, ProcessMetadataFailure,
};

use super::failures::{classify_io, record_failure};

const MAX_ICON_THEME_FILE_BYTES: u64 = 64 * 1024;
const MAX_ICON_THEME_DEPTH: usize = 16;
const ICON_THEME_EXTENSIONS: [&str; 6] = ["svg", "png", "jpg", "jpeg", "webp", "bmp"];

/// Resolve one desktop `Icon=` token to validated bytes without exposing its
/// Linux path or theme directory to the shared model.
pub(super) fn resolve_icon_asset_from_dirs(
    data_dirs: &[PathBuf],
    token: Option<&str>,
) -> (Option<ApplicationIconAsset>, Option<ProcessMetadataFailure>) {
    let themes = icon_theme_names();
    resolve_icon_asset_from_dirs_with_themes(data_dirs, token, &themes)
}

fn resolve_icon_asset_from_dirs_with_themes(
    data_dirs: &[PathBuf],
    token: Option<&str>,
    themes: &[String],
) -> (Option<ApplicationIconAsset>, Option<ProcessMetadataFailure>) {
    let Some(token) = token.map(str::trim).filter(|token| !token.is_empty()) else {
        return (None, Some(ProcessMetadataFailure::NotFound));
    };
    let token_path = Path::new(token);
    if token_path.is_absolute() {
        return resolve_candidates(&candidate_paths(token_path));
    }
    if token_path
        .components()
        .any(|component| matches!(component, Component::ParentDir))
    {
        return (None, Some(ProcessMetadataFailure::Unsupported));
    }

    let mut failures = None;
    for icon_root in icon_roots(data_dirs) {
        for theme in themes {
            let (asset, failure) = resolve_theme(&icon_root, theme, token);
            if let Some(asset) = asset {
                return (Some(asset), None);
            }
            if let Some(failure) = failure {
                record_failure(&mut failures, failure);
            }
        }
    }

    for data_dir in data_dirs {
        let base = data_dir.join("pixmaps").join(token);
        let (asset, failure) = resolve_candidates(&candidate_paths(&base));
        if let Some(asset) = asset {
            return (Some(asset), None);
        }
        if let Some(failure) = failure {
            record_failure(&mut failures, failure);
        }
    }

    (None, failures.or(Some(ProcessMetadataFailure::NotFound)))
}

fn icon_roots(data_dirs: &[PathBuf]) -> Vec<PathBuf> {
    let mut roots = Vec::new();
    let mut seen = HashSet::new();
    if let Some(home) = std::env::var_os("HOME").filter(|value| !value.is_empty()) {
        let root = PathBuf::from(home).join(".icons");
        if seen.insert(root.clone()) {
            roots.push(root);
        }
    }
    for data_dir in data_dirs {
        let root = data_dir.join("icons");
        if seen.insert(root.clone()) {
            roots.push(root);
        }
    }
    roots
}

fn icon_theme_names() -> Vec<String> {
    let mut themes = Vec::new();
    for key in ["XDG_ICON_THEME", "GTK_ICON_THEME"] {
        if let Some(theme) = std::env::var_os(key)
            .filter(|value| !value.is_empty())
            .and_then(|value| value.into_string().ok())
        {
            push_theme(&mut themes, theme);
        }
    }
    if let Some(config_home) = config_home() {
        for relative in [
            "gtk-4.0/settings.ini",
            "gtk-3.0/settings.ini",
            "gtk-2.0/settings.ini",
        ] {
            let path = config_home.join(relative);
            let Ok(text) = fs::read_to_string(&path) else {
                continue;
            };
            for line in text.lines() {
                let Some((key, value)) = line.split_once('=') else {
                    continue;
                };
                if key.trim() == "gtk-icon-theme-name" {
                    push_theme(&mut themes, value.trim().to_owned());
                }
            }
        }
    }
    push_theme(&mut themes, "hicolor".to_owned());
    themes
}

fn config_home() -> Option<PathBuf> {
    std::env::var_os("XDG_CONFIG_HOME")
        .filter(|value| !value.is_empty())
        .map(PathBuf::from)
        .or_else(|| {
            std::env::var_os("HOME")
                .filter(|value| !value.is_empty())
                .map(|home| PathBuf::from(home).join(".config"))
        })
}

fn push_theme(themes: &mut Vec<String>, theme: String) {
    if !theme.is_empty() && !themes.iter().any(|current| current == &theme) {
        themes.push(theme);
    }
}

fn resolve_theme(
    icon_root: &Path,
    theme: &str,
    token: &str,
) -> (Option<ApplicationIconAsset>, Option<ProcessMetadataFailure>) {
    let mut pending = vec![(icon_root.join(theme), 0usize)];
    let mut visited = HashSet::new();
    let mut failures = None;

    while let Some((theme_dir, depth)) = pending.pop() {
        if depth > MAX_ICON_THEME_DEPTH || !visited.insert(theme_dir.clone()) {
            continue;
        }
        let (directories, inherits, metadata_failure) = theme_metadata(&theme_dir);
        if let Some(failure) = metadata_failure {
            record_failure(&mut failures, failure);
        }
        for directory in directories {
            let base = if directory.is_empty() {
                theme_dir.join(token)
            } else {
                theme_dir.join(directory).join(token)
            };
            let (asset, failure) = resolve_candidates(&candidate_paths(&base));
            if let Some(asset) = asset {
                return (Some(asset), None);
            }
            if let Some(failure) = failure {
                record_failure(&mut failures, failure);
            }
        }
        for inherited in inherits {
            pending.push((icon_root.join(inherited), depth + 1));
        }
    }

    (None, failures)
}

fn theme_metadata(theme_dir: &Path) -> (Vec<String>, Vec<String>, Option<ProcessMetadataFailure>) {
    let default_directories = || {
        vec![
            String::new(),
            "scalable/apps".to_owned(),
            "scalable".to_owned(),
            "48x48/apps".to_owned(),
            "32x32/apps".to_owned(),
            "24x24/apps".to_owned(),
            "22x22/apps".to_owned(),
            "16x16/apps".to_owned(),
        ]
    };
    let index = theme_dir.join("index.theme");
    let bytes = match read_bounded(&index, MAX_ICON_THEME_FILE_BYTES) {
        Ok(Some(bytes)) => bytes,
        Ok(None) => return (default_directories(), Vec::new(), None),
        Err(failure) => return (default_directories(), Vec::new(), Some(failure)),
    };
    let Ok(text) = String::from_utf8(bytes) else {
        return (
            default_directories(),
            Vec::new(),
            Some(ProcessMetadataFailure::Unsupported),
        );
    };
    let mut in_icon_theme = false;
    let mut directories = Vec::new();
    let mut inherits = Vec::new();
    for line in text.lines() {
        let line = line.trim();
        if let Some(section) = line
            .strip_prefix('[')
            .and_then(|value| value.strip_suffix(']'))
        {
            in_icon_theme = section == "Icon Theme";
            continue;
        }
        if !in_icon_theme || line.is_empty() || line.starts_with('#') {
            continue;
        }
        let Some((key, value)) = line.split_once('=') else {
            continue;
        };
        match key.trim() {
            "Directories" => directories.extend(
                value
                    .split(',')
                    .map(str::trim)
                    .filter(|directory| !directory.is_empty())
                    .map(ToOwned::to_owned),
            ),
            "Inherits" => inherits.extend(
                value
                    .split(',')
                    .map(str::trim)
                    .filter(|theme| !theme.is_empty())
                    .map(ToOwned::to_owned),
            ),
            _ => {}
        }
    }
    if directories.is_empty() {
        directories = default_directories();
    }
    (directories, inherits, None)
}

fn candidate_paths(base: &Path) -> Vec<PathBuf> {
    if base.extension().is_some() {
        return vec![base.to_path_buf()];
    }
    ICON_THEME_EXTENSIONS
        .iter()
        .map(|extension| base.with_extension(extension))
        .collect()
}

fn resolve_candidates(
    candidates: &[PathBuf],
) -> (Option<ApplicationIconAsset>, Option<ProcessMetadataFailure>) {
    let mut failure = None;
    for candidate in candidates {
        match read_icon(candidate) {
            Ok(Some(asset)) => return (Some(asset), None),
            Ok(None) => {}
            Err(error) => record_failure(&mut failure, error),
        }
    }
    (None, failure)
}

fn read_icon(path: &Path) -> Result<Option<ApplicationIconAsset>, ProcessMetadataFailure> {
    let metadata = match fs::metadata(path) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(classify_io(&error)),
    };
    if !metadata.is_file() {
        return Ok(None);
    }
    if metadata.len() == 0 || metadata.len() > MAX_APPLICATION_ICON_BYTES as u64 {
        return Err(ProcessMetadataFailure::Unsupported);
    }
    let bytes = fs::read(path).map_err(|error| classify_io(&error))?;
    let format = detect_format(path, &bytes).ok_or(ProcessMetadataFailure::Unsupported)?;
    ApplicationIconAsset::from_bytes(format, bytes)
        .ok_or(ProcessMetadataFailure::ProviderFault)
        .map(Some)
}

fn read_bounded(path: &Path, max_bytes: u64) -> Result<Option<Vec<u8>>, ProcessMetadataFailure> {
    let metadata = match fs::metadata(path) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(classify_io(&error)),
    };
    if !metadata.is_file() {
        return Ok(None);
    }
    if metadata.len() > max_bytes {
        return Err(ProcessMetadataFailure::ProviderFault);
    }
    fs::read(path)
        .map(Some)
        .map_err(|error| classify_io(&error))
}

fn detect_format(path: &Path, bytes: &[u8]) -> Option<ApplicationIconFormat> {
    let extension = path
        .extension()
        .and_then(|extension| extension.to_str())
        .map(|extension| extension.to_ascii_lowercase());
    match extension.as_deref() {
        Some("svg") if is_svg(bytes) => Some(ApplicationIconFormat::Svg),
        Some("png") if bytes.starts_with(b"\x89PNG\r\n\x1a\n") => Some(ApplicationIconFormat::Png),
        Some("jpg" | "jpeg") if bytes.starts_with(&[0xff, 0xd8, 0xff]) => {
            Some(ApplicationIconFormat::Jpeg)
        }
        Some("webp") if bytes.starts_with(b"RIFF") && bytes.get(8..12) == Some(b"WEBP") => {
            Some(ApplicationIconFormat::Webp)
        }
        Some("bmp") if bytes.starts_with(b"BM") => Some(ApplicationIconFormat::Bmp),
        Some(_) => None,
        None if is_svg(bytes) => Some(ApplicationIconFormat::Svg),
        None if bytes.starts_with(b"\x89PNG\r\n\x1a\n") => Some(ApplicationIconFormat::Png),
        None if bytes.starts_with(&[0xff, 0xd8, 0xff]) => Some(ApplicationIconFormat::Jpeg),
        None if bytes.starts_with(b"RIFF") && bytes.get(8..12) == Some(b"WEBP") => {
            Some(ApplicationIconFormat::Webp)
        }
        None if bytes.starts_with(b"BM") => Some(ApplicationIconFormat::Bmp),
        _ => None,
    }
}

fn is_svg(bytes: &[u8]) -> bool {
    String::from_utf8_lossy(&bytes[..bytes.len().min(4096)])
        .to_ascii_lowercase()
        .contains("<svg")
}

#[cfg(test)]
#[path = "../../../../tests/headless/linux_engine_process_application_icons_tests.rs"]
mod tests;
