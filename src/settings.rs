use crate::embedded::DefaultScripts;
use config::{Config, ConfigError};
use directories::ProjectDirs;
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;
use tracing::{debug, error, info};

#[derive(Debug, Deserialize, Serialize, Default, Clone)]
pub struct SimilarNotesConfig {
    pub excluded_notes: Option<Vec<String>>,
    pub excluded_from_references: Option<Vec<String>>,
}

#[derive(Debug, Deserialize, Serialize, Default, Clone)]
#[allow(dead_code)]
pub struct Settings {
    pub file_type: String,
    pub timestamps: bool,
    pub note_dir: String,
    pub scripts_dir: String,
    pub obsidian_vault_path: Option<String>,
    pub enabled_observers: Vec<String>,
    pub similar_notes: SimilarNotesConfig,
}

impl Settings {
    pub fn new() -> Self {
        debug!("Loading application settings");

        let config_path = match Self::ensure_config_exists() {
            Ok(path) => {
                debug!("Using config file at: {:?}", path);
                path
            }
            Err(e) => {
                error!("Failed to initialize config: {}", e);
                panic!("Failed to initialize config: {}", e);
            }
        };

        let config_result = Config::builder()
            .add_source(config::File::from(config_path).required(true))
            .add_source(config::Environment::with_prefix("NOTE_CLI"))
            .build();

        let settings = match config_result {
            Ok(config) => {
                debug!("Configuration sources loaded successfully");
                match config.try_deserialize::<Settings>() {
                    Ok(settings) => {
                        debug!("Settings deserialized successfully");
                        trace_settings(&settings);
                        settings
                    }
                    Err(e) => {
                        error!("Failed to deserialize configuration: {}", e);
                        panic!("Failed to deserialize configuration: {}", e);
                    }
                }
            }
            Err(e) => {
                error!("Failed to load configuration: {}", e);
                panic!("Failed to load configuration: {}", e);
            }
        };

        Self::ensure_directories_exist(&settings);

        info!("✨ Settings loaded successfully");
        settings
    }

    fn ensure_config_exists() -> Result<PathBuf, ConfigError> {
        let proj_dirs = ProjectDirs::from("", "norg", "norg")
            .ok_or_else(|| ConfigError::NotFound("Could not determine config directory".into()))?;

        let config_dir = proj_dirs.config_dir();
        debug!("Config directory: {:?}", config_dir);

        // Create base directories
        let norg_base_dir = dirs::document_dir()
            .map(|d| d.join("norg"))
            .unwrap_or_else(|| PathBuf::from("./norg"));

        let scripts_dir = norg_base_dir.join("scripts");

        // Copy default scripts before creating config
        Self::copy_default_scripts(&scripts_dir)?;

        let config_path = config_dir.join("config.toml");
        debug!("Config file path: {:?}", config_path);

        if !config_path.exists() {
            debug!("Creating default config file");
            let norg_base_dir = dirs::document_dir()
                .map(|d| d.join("norg"))
                .unwrap_or_else(|| PathBuf::from("./norg"));

            let default_settings = Settings {
                file_type: "md".to_string(),
                timestamps: true,
                note_dir: norg_base_dir.join("notes").to_string_lossy().into_owned(),
                scripts_dir: norg_base_dir.join("scripts").to_string_lossy().into_owned(),
                obsidian_vault_path: Some(
                    dirs::home_dir()
                        .map(|h| {
                            h.join("Library/Mobile Documents/iCloud~md~obsidian/Documents/Obsidian")
                        })
                        .unwrap_or_default()
                        .to_string_lossy()
                        .into_owned(),
                ),
                enabled_observers: vec![
                    "timestamp".to_string(),
                    "sqlite".to_string(),
                    "tag_index".to_string(),
                    "similar_notes".to_string(),
                    "toc".to_string(),
                ],
                similar_notes: SimilarNotesConfig {
                    excluded_notes: Some(vec![
                        "_kanban".to_string(),
                        "_tag_index".to_string(),
                        "project".to_string(),
                    ]),
                    excluded_from_references: Some(vec![
                        "_tag_index".to_string(),
                        "_kanban".to_string(),
                    ]),
                },
            };

            let config_str = toml::to_string_pretty(&default_settings).map_err(|e| {
                ConfigError::NotFound(format!("Failed to serialize default config: {}", e))
            })?;

            fs::write(&config_path, config_str).map_err(|e| {
                ConfigError::NotFound(format!("Failed to write default config: {}", e))
            })?;

            debug!("Created default config at {:?}", config_path);
        }

        Ok(config_path)
    }

    fn ensure_directories_exist(settings: &Settings) {
        if let Err(e) = fs::create_dir_all(&settings.note_dir) {
            error!("Failed to create note directory: {}", e);
            panic!("Failed to create note directory: {}", e);
        }

        if let Err(e) = fs::create_dir_all(&settings.scripts_dir) {
            error!("Failed to create scripts directory: {}", e);
            panic!("Failed to create scripts directory: {}", e);
        }
    }

    fn copy_default_scripts(target_dir: &PathBuf) -> Result<(), ConfigError> {
        fs::create_dir_all(target_dir).map_err(|e| {
            ConfigError::NotFound(format!("Failed to create scripts directory: {}", e))
        })?;

        for file in DefaultScripts::iter() {
            let file_path = PathBuf::from(file.as_ref());
            let script_path = target_dir.join(&file_path);

            // Create parent directories if they don't exist
            if let Some(parent) = script_path.parent() {
                fs::create_dir_all(parent).map_err(|e| {
                    ConfigError::NotFound(format!(
                        "Failed to create directory {}: {}",
                        parent.display(),
                        e
                    ))
                })?;
            }

            if !script_path.exists() {
                if let Some(content) = DefaultScripts::get(&file) {
                    fs::write(&script_path, content.data).map_err(|e| {
                        ConfigError::NotFound(format!("Failed to write script {}: {}", file, e))
                    })?;

                    #[cfg(unix)]
                    {
                        use std::os::unix::fs::PermissionsExt;
                        fs::set_permissions(&script_path, fs::Permissions::from_mode(0o755))
                            .map_err(|e| {
                                ConfigError::NotFound(format!(
                                    "Failed to make script {} executable: {}",
                                    file, e
                                ))
                            })?;
                    }

                    debug!("Created script {} at {:?}", file, script_path);
                }
            }
        }

        Ok(())
    }

    pub fn get_data_dir() -> PathBuf {
        ProjectDirs::from("", "norg", "norg")
            .map(|proj_dirs| proj_dirs.data_dir().to_path_buf())
            .unwrap_or_else(|| PathBuf::from("./data"))
    }
}

fn trace_settings(settings: &Settings) {
    debug!("Loaded settings:");
    debug!("  File type: {}", settings.file_type);
    debug!("  Timestamps enabled: {}", settings.timestamps);
    debug!("  Note directory: {}", settings.note_dir);
    debug!("  Scripts directory: {}", settings.scripts_dir);

    if let Some(ref vault_path) = settings.obsidian_vault_path {
        debug!("  Obsidian vault path: {}", vault_path);
    } else {
        debug!("  No Obsidian vault path configured");
    }

    debug!("  Enabled observers: {:?}", settings.enabled_observers);

    if let Some(ref excluded) = settings.similar_notes.excluded_notes {
        debug!("  Excluded notes: {:?}", excluded);
    }

    if let Some(ref excluded_refs) = settings.similar_notes.excluded_from_references {
        debug!("  Excluded from references: {:?}", excluded_refs);
    }
}
