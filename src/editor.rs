use percent_encoding::{utf8_percent_encode, NON_ALPHANUMERIC};
use std::env;
use std::fs;
use std::io;
use std::path::PathBuf;
use std::process::Command;
use std::thread;
use std::time::Duration;
use tempfile::NamedTempFile;
use tracing::{debug, error, info, warn};

use crate::settings::Settings;

pub fn open_editor(initial_content: &str, settings: &Settings) -> io::Result<String> {
    debug!(
        "Opening editor with {} bytes of initial content",
        initial_content.len()
    );

    let editor = env::var("EDITOR").unwrap_or_else(|_| {
        debug!("No EDITOR environment variable found, checking common editors");
        for editor in ["nvim", "vim", "nano"] {
            if command_exists(editor) {
                debug!("Found editor: {}", editor);
                return editor.to_string();
            }
        }
        warn!("No common editors found, defaulting to vim");
        "vim".to_string()
    });

    if editor.to_lowercase() == "obsidian" {
        debug!("Using Obsidian as editor");
        return open_in_obsidian(initial_content, settings);
    }

    debug!("Creating temporary file for editing");
    let temp_file = NamedTempFile::new().map_err(|e| {
        error!("Failed to create temporary file: {}", e);
        io::Error::new(io::ErrorKind::Other, e)
    })?;

    if !initial_content.is_empty() {
        debug!("Writing initial content to temporary file");
        fs::write(&temp_file, initial_content).map_err(|e| {
            error!("Failed to write initial content: {}", e);
            e
        })?;
    }

    info!("🖊️ Opening {} editor", editor);
    let result = Command::new(&editor)
        .arg(temp_file.path())
        .status()
        .map_err(|e| {
            error!("Failed to open editor '{}': {}", editor, e);
            io::Error::new(
                io::ErrorKind::NotFound,
                format!("Failed to open editor '{}': {}. Please ensure it's installed or set a different editor using the EDITOR environment variable.", editor, e)
            )
        })?;

    if !result.success() {
        error!("Editor '{}' returned non-zero status", editor);
        return Err(io::Error::new(
            io::ErrorKind::Other,
            format!("Editor '{}' returned non-zero status", editor),
        ));
    }

    debug!("Reading edited content from temporary file");
    let content = fs::read_to_string(temp_file.path()).map_err(|e| {
        error!("Failed to read edited content: {}", e);
        e
    })?;

    info!("✨ Editor closed successfully");
    Ok(content)
}

fn open_in_obsidian(initial_content: &str, settings: &Settings) -> io::Result<String> {
    debug!("Opening note in Obsidian");
    let notes_dir = settings.obsidian_vault_path.clone().unwrap_or_else(|| {
        warn!("No Obsidian vault path found in config, using default path");
        "./notes".to_string()
    });

    info!("📂 Using Obsidian vault path: {}", notes_dir);
    let notes_path = PathBuf::from(&notes_dir);
    if !notes_path.exists() {
        error!("Obsidian vault directory not found: {}", notes_dir);
        return Err(io::Error::new(
            io::ErrorKind::NotFound,
            format!("Obsidian vault directory not found: {}", notes_dir),
        ));
    }

    debug!("Creating temporary directory for Obsidian");
    let temp_dir = notes_path.join("_temp");
    fs::create_dir_all(&temp_dir).map_err(|e| {
        error!("Failed to create temp directory: {}", e);
        e
    })?;

    let temp_filename = format!("temp_{}.md", chrono::Utc::now().timestamp());
    let temp_path = temp_dir.join(&temp_filename);
    debug!("Created temporary file at: {}", temp_path.display());

    if !initial_content.is_empty() {
        debug!("Writing initial content to temporary file");
        fs::write(&temp_path, initial_content).map_err(|e| {
            error!("Failed to write initial content: {}", e);
            e
        })?;
    }

    info!("🚀 Launching Obsidian...");
    let launch_status = if cfg!(target_os = "macos") {
        Command::new("open").arg("obsidian://open").status()
    } else if cfg!(target_os = "windows") {
        Command::new("cmd")
            .args(["/C", "start", "obsidian://open"])
            .status()
    } else {
        Command::new("xdg-open").arg("obsidian://open").status()
    }
    .map_err(|e| {
        error!("Failed to launch Obsidian: {}", e);
        e
    })?;

    if !launch_status.success() {
        error!("Failed to launch Obsidian");
        return Err(io::Error::new(
            io::ErrorKind::Other,
            "Failed to launch Obsidian",
        ));
    }

    debug!("Waiting for Obsidian to start...");
    thread::sleep(Duration::from_secs(1));

    let absolute_path = temp_path.canonicalize().map_err(|e| {
        error!("Failed to get absolute path: {}", e);
        e
    })?;
    let path_str = absolute_path.to_string_lossy();
    let encoded_path = utf8_percent_encode(&path_str, NON_ALPHANUMERIC).to_string();
    let obsidian_url = format!("obsidian://open?path={}", encoded_path);

    debug!("Opening note with URL: {}", obsidian_url);

    let status = if cfg!(target_os = "macos") {
        Command::new("open").arg(&obsidian_url).status()
    } else if cfg!(target_os = "windows") {
        Command::new("cmd")
            .args(["/C", "start", "", &obsidian_url])
            .status()
    } else {
        Command::new("xdg-open").arg(&obsidian_url).status()
    }
    .map_err(|e| {
        error!("Failed to open note in Obsidian: {}", e);
        e
    })?;

    if !status.success() {
        error!("Failed to open note in Obsidian");
        return Err(io::Error::new(
            io::ErrorKind::Other,
            "Failed to open note in Obsidian",
        ));
    }

    info!("📝 Note opened in Obsidian. Press Enter when you're done editing...");
    let mut input = String::new();
    io::stdin().read_line(&mut input)?;

    debug!("Reading edited content");
    let content = fs::read_to_string(&temp_path).map_err(|e| {
        error!("Failed to read edited content: {}", e);
        e
    })?;

    debug!("Cleaning up temporary files");
    if let Err(e) = fs::remove_file(&temp_path) {
        warn!("Failed to remove temporary file: {}", e);
    }
    if let Err(e) = fs::remove_dir(&temp_dir) {
        warn!("Failed to remove temporary directory: {}", e);
    }

    info!("✨ Successfully saved changes from Obsidian");
    Ok(content)
}

fn command_exists(command: &str) -> bool {
    debug!("Checking if command exists: {}", command);
    let exists = if cfg!(target_os = "windows") {
        Command::new("where")
            .arg(command)
            .output()
            .map(|output| output.status.success())
            .unwrap_or(false)
    } else {
        Command::new("which")
            .arg(command)
            .output()
            .map(|output| output.status.success())
            .unwrap_or(false)
    };

    if exists {
        debug!("Command '{}' found", command);
    } else {
        debug!("Command '{}' not found", command);
    }
    exists
}
