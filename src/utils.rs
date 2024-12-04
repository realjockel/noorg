use crate::settings::Settings;
use std::path::{Path, PathBuf};
use tracing::{debug, error, warn};

pub fn get_absolute_note_path(title: &str, settings: &Settings) -> String {
    debug!("Getting absolute note path for title: {}", title);
    let path = PathBuf::from(&settings.note_dir).join(format!(
        "{}.{}",
        title.replace(" ", "%20"),
        settings.file_type
    ));

    match path.to_str() {
        Some(p) => {
            debug!("Generated absolute path: {}", p);
            p.to_string()
        }
        None => {
            error!("Failed to convert path to string for title: {}", title);
            format!(
                "{}/{}.{}",
                settings.note_dir,
                title.replace(" ", "%20"),
                settings.file_type
            )
        }
    }
}

pub fn get_fs_path(title: &str, settings: &Settings) -> PathBuf {
    debug!("Getting filesystem path for title: {}", title);
    let path = PathBuf::from(&settings.note_dir).join(format!(
        "{}.{}",
        title.replace(" ", "%20"),
        settings.file_type
    ));
    debug!("Generated filesystem path: {}", path.display());
    path
}

pub fn get_note_title_from_path(path: &str) -> String {
    debug!("Extracting note title from path: {}", path);
    let title = Path::new(path)
        .file_stem()
        .and_then(|s| s.to_str())
        .map(|s| s.replace("%20", " "))
        .unwrap_or_default();

    if title.is_empty() {
        warn!("Could not extract title from path: {}", path);
    } else {
        debug!("Extracted title: {}", title);
    }

    title
}
