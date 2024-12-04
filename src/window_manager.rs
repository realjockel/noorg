use crate::settings::Settings;
use directories::ProjectDirs;
use std::path::PathBuf;
use std::process::Command;
use std::sync::Arc;
use tokio::sync::Mutex;
use tracing::{error, info};

pub fn open_settings(settings: Arc<Mutex<Settings>>) {
    // Get the project directory for settings
    let config_path = if let Some(proj_dirs) = ProjectDirs::from("", "norg", "norg") {
        proj_dirs.config_dir().join("config.toml")
    } else {
        error!("Could not determine config directory");
        return;
    };

    // Get settings in a separate thread to avoid blocking
    std::thread::spawn(move || {
        // Create a new runtime for this thread
        let rt = tokio::runtime::Runtime::new().unwrap();

        // Get settings
        let settings_clone = rt.block_on(async {
            let settings = settings.lock().await;
            settings.clone()
        });

        // Save to config file
        if let Ok(config_str) = toml::to_string_pretty(&settings_clone) {
            if let Err(e) = std::fs::write(&config_path, &config_str) {
                error!("Failed to save settings: {}", e);
                return;
            }

            // Get the path to the settings binary
            let settings_binary = if let Ok(exe_path) = std::env::current_exe() {
                let mut path = exe_path
                    .parent()
                    .unwrap_or(&PathBuf::from("."))
                    .to_path_buf();
                path.push("note_settings");
                #[cfg(target_os = "windows")]
                path.set_extension("exe");
                path
            } else {
                PathBuf::from("note_settings")
            };

            // Launch settings dialog
            match Command::new(&settings_binary).arg(&config_path).spawn() {
                Ok(_) => {
                    info!("Settings dialog opened with binary: {:?}", settings_binary);
                }
                Err(e) => {
                    error!(
                        "Failed to open settings using binary {:?}: {}",
                        settings_binary, e
                    );
                    rfd::MessageDialog::new()
                        .set_title("Error")
                        .set_description(&format!("Failed to open settings: {}", e))
                        .set_level(rfd::MessageLevel::Error)
                        .show();
                }
            }
        }
    });
}
