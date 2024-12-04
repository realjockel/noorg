use chrono::{DateTime, FixedOffset, Local};
use directories::ProjectDirs;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::HashMap;
use std::fs::{self, File};
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use tracing::{debug, error, info, warn};

use crate::event::NoteEvent;
use crate::metadata::merge_metadata;
use crate::observer_registry::ObserverRegistry;
use crate::settings::Settings;
use crate::utils::get_absolute_note_path;

#[derive(Debug, Deserialize, Serialize)]
struct Frontmatter {
    #[serde(flatten)]
    fields: HashMap<String, String>,
}

pub struct Note {
    title: String,
    content: String,
    frontmatter: Frontmatter,
}

impl Note {
    pub async fn new(
        title: String,
        content: String,
        mut frontmatter_fields: HashMap<String, String>,
    ) -> Self {
        debug!("Creating new note: {}", title);

        if let Some(tags) = frontmatter_fields.get("tags") {
            debug!("Processing tags: {}", tags);
            let formatted_tags: Vec<String> = tags
                .split(',')
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty())
                .collect();
            frontmatter_fields.insert("tags".to_string(), formatted_tags.join(", "));
        }

        Note {
            title,
            content,
            frontmatter: Frontmatter {
                fields: frontmatter_fields,
            },
        }
    }

    pub fn to_string(&self, _settings: &Settings) -> String {
        debug!("Converting note to string format: {}", self.title);
        let frontmatter_str = match serde_yaml::to_string(&self.frontmatter.fields) {
            Ok(s) => s,
            Err(e) => {
                error!("Failed to serialize frontmatter: {}", e);
                String::new()
            }
        };

        // First split on References section to preserve it and any Similar Notes
        let content_parts: Vec<&str> = self.content.split("\n## References\n").collect();
        let main_content = content_parts[0];

        // Process main content
        let lines: Vec<&str> = main_content.lines().collect();
        let mut output = Vec::new();
        let mut skip_until_content = true;
        let mut first_h1_seen = false;
        let mut in_sql_block = false;
        let mut in_toc = false;

        for line in lines {
            // Always preserve SQL blocks
            if line.starts_with("```sql") {
                in_sql_block = true;
                output.push(line);
                continue;
            } else if in_sql_block {
                output.push(line);
                if line.starts_with("```") {
                    in_sql_block = false;
                }
                continue;
            }

            // Handle TOC section
            if line.starts_with("## Contents") {
                in_toc = true;
                output.push(line);
                continue;
            } else if in_toc {
                if line.starts_with("## ") {
                    in_toc = false;
                } else {
                    output.push(line);
                    continue;
                }
            }

            // Skip empty lines and frontmatter at start
            if skip_until_content {
                if line.trim().is_empty() || line.trim() == "---" || line.contains(": ") {
                    continue;
                }
                skip_until_content = false;
            }

            // Handle H1 headers (skip duplicates)
            if line.starts_with("# ") {
                if !first_h1_seen {
                    first_h1_seen = true;
                    output.push(line);
                }
                continue;
            }

            output.push(line);
        }

        // Combine processed content with References and Similar Notes if they exist
        let final_content = if content_parts.len() > 1 {
            let references_section = content_parts[1];
            // Split references section to preserve Similar Notes
            let references_parts: Vec<&str> =
                references_section.split("\n\nSimilar notes:\n").collect();

            if references_parts.len() > 1 {
                // We have both References and Similar Notes
                format!(
                    "{}\n\n## References\n{}\n\nSimilar notes:\n{}",
                    output.join("\n"),
                    references_parts[0],
                    references_parts[1]
                )
            } else {
                // We only have References
                format!(
                    "{}\n\n## References\n{}",
                    output.join("\n"),
                    references_section
                )
            }
        } else {
            output.join("\n")
        };

        // Format the final note with frontmatter
        format!("---\n{}---\n\n{}", frontmatter_str, final_content)
    }

    pub fn from_file(path: &Path) -> io::Result<Option<(String, HashMap<String, String>)>> {
        debug!("Reading note from file: {}", path.display());
        let content = fs::read_to_string(path).map_err(|e| {
            error!("Failed to read file {}: {}", path.display(), e);
            e
        })?;

        let metadata = fs::metadata(path)?;
        let created_time = metadata
            .created()
            .map(|time| {
                let datetime: DateTime<Local> = time.into();
                datetime.format("%Y-%m-%d %H:%M:%S %z").to_string()
            })
            .unwrap_or_else(|_| {
                warn!("Could not get file creation time, using current time");
                Local::now().format("%Y-%m-%d %H:%M:%S %z").to_string()
            });

        // Try to find frontmatter
        if let Some(start) = content.find("---\n") {
            if let Some(end) = content[start + 4..].find("\n---\n") {
                let frontmatter_str = &content[start + 4..start + 4 + end];
                let content_start = start + 4 + end + 5;
                let actual_content = content[content_start..].trim().to_string();

                match serde_yaml::from_str::<HashMap<String, String>>(frontmatter_str) {
                    Ok(mut fields) => {
                        debug!("Successfully parsed frontmatter for {}", path.display());
                        if !fields.contains_key("created_at") {
                            debug!("Adding created_at timestamp");
                            fields.insert("created_at".to_string(), created_time);
                        }
                        return Ok(Some((actual_content, fields)));
                    }
                    Err(e) => error!("Failed to parse frontmatter: {}", e),
                }
            }
        }

        debug!("No frontmatter found, creating default");
        let mut default_frontmatter = HashMap::new();
        default_frontmatter.insert("created_at".to_string(), created_time);

        Ok(Some((content, default_frontmatter)))
    }

    pub async fn save(&self, settings: &Settings) -> io::Result<String> {
        let filename = get_absolute_note_path(&self.title, settings);
        debug!("Saving note to: {}", filename);

        // Get the properly formatted content including frontmatter and TOC
        let content = self.to_string(settings);
        debug!("Content to be saved:\n{}", content);

        let path = Path::new(&filename);
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).map_err(|e| {
                error!("Failed to create directory {}: {}", parent.display(), e);
                e
            })?;
        }

        let mut file = File::create(&filename).map_err(|e| {
            error!("Failed to create file {}: {}", filename, e);
            e
        })?;

        file.write_all(content.as_bytes()).map_err(|e| {
            error!("Failed to write content to {}: {}", filename, e);
            e
        })?;

        info!("✨ Saved note: {}", filename);
        Ok(filename)
    }
}

pub struct NoteManager {
    settings: Settings,
    notes_dir: String,
    observer_registry: Arc<ObserverRegistry>,
}

impl NoteManager {
    pub async fn new(
        settings: Settings,
        observer_registry: Arc<ObserverRegistry>,
    ) -> io::Result<Self> {
        debug!("Initializing NoteManager");
        let notes_dir = settings.note_dir.clone();
        fs::create_dir_all(&notes_dir).map_err(|e| {
            error!("Failed to create notes directory: {}", e);
            e
        })?;

        Ok(NoteManager {
            settings,
            notes_dir,
            observer_registry,
        })
    }
    pub fn title_to_filename(&self, title: &str) -> String {
        debug!("Converting title to filename: {}", title);
        title.to_lowercase().replace(" ", "_")
    }

    pub async fn add_note(
        &self,
        title: String,
        content: String,
        frontmatter_fields: HashMap<String, String>,
    ) -> io::Result<()> {
        info!(
            "📝 Adding note '{}' with {} frontmatter fields",
            title,
            frontmatter_fields.len()
        );
        debug!("Initial frontmatter: {:?}", frontmatter_fields);

        let mut note = Note::new(title.clone(), content.clone(), frontmatter_fields.clone()).await;

        let filename = note.save(&self.settings).await?;
        info!("💾 Saved initial note to: {}", &filename);

        let observers = self.observer_registry.get_observers().await;
        let skip_observers: Vec<String> = if frontmatter_fields
            .get("skip_observers")
            .map_or(false, |s| s.trim() == "all")
        {
            debug!("Skipping all observers");
            observers.iter().map(|o| o.name()).collect()
        } else {
            frontmatter_fields
                .get("skip_observers")
                .map(|s| {
                    debug!("Processing skip_observers: {}", s);
                    s.split(',')
                        .map(|s| s.trim().to_string())
                        .filter(|s| !s.is_empty())
                        .collect()
                })
                .unwrap_or_default()
        };

        let active_observers: Vec<_> = observers
            .iter()
            .filter(|o| !skip_observers.contains(&o.name()))
            .collect();

        info!(
            "🔄 Processing {} active observers...",
            active_observers.len()
        );
        if !skip_observers.is_empty() {
            info!("ℹ️ Skipping observers: {}", skip_observers.join(", "));
        }

        let event = NoteEvent::Created {
            title: title.clone(),
            content: content.clone(),
            file_path: get_absolute_note_path(&title, &self.settings),
            frontmatter: frontmatter_fields.clone(),
        };

        let mut combined_metadata = frontmatter_fields;
        let mut final_content = content;

        for observer in &observers {
            if observer.name() == "sqlite" {
                debug!("Skipping SQLite observer for now (will run last)");
                continue;
            }

            info!("🔵 Running observer: {}", observer.name());
            match observer.on_event_boxed(event.clone()).await {
                Ok(Some(result)) => {
                    if let Some(metadata) = result.metadata {
                        debug!("Observer returned metadata: {:?}", metadata);
                        merge_metadata(&mut combined_metadata, metadata);
                    }
                    if let Some(new_content) = result.content {
                        debug!("Observer modified content");
                        final_content = new_content;
                    }
                }
                Ok(None) => debug!("No changes from observer: {}", observer.name()),
                Err(e) => error!("Error from observer {}: {}", observer.name(), e),
            }
        }

        note.content = final_content;
        note.frontmatter.fields = combined_metadata.clone();
        note.save(&self.settings).await?;

        if let Some(sqlite_observer) = observers.iter().find(|o| o.name() == "sqlite") {
            debug!("Running SQLite observer");
            match sqlite_observer.on_event_boxed(event).await {
                Ok(Some(result)) => {
                    if let Some(new_content) = result.content {
                        info!("✨ SQLite observer modified content");
                        let updated_note =
                            Note::new(title.clone(), new_content, combined_metadata.clone()).await;
                        updated_note.save(&self.settings).await?;
                    }
                }
                Ok(None) => debug!("No changes from SQLite observer"),
                Err(e) => {
                    error!("SQLite observer error: {}", e);
                    return Err(e);
                }
            }
        }

        info!("✨ Note added successfully: {}", title);
        Ok(())
    }

    pub fn delete_note(&self, title: &str) -> io::Result<()> {
        let filename = format!(
            "{}/{}.{}",
            self.notes_dir,
            self.title_to_filename(&title),
            self.settings.file_type
        );
        debug!("Attempting to delete note: {}", filename);

        if Path::new(&filename).exists() {
            fs::remove_file(&filename).map_err(|e| {
                error!("Failed to delete note '{}': {}", title, e);
                e
            })?;
            info!("🗑️ Note '{}' deleted successfully", title);
        } else {
            warn!("Note '{}' not found", title);
        }
        Ok(())
    }

    pub fn list_notes_with_filter(
        &self,
        from: Option<DateTime<FixedOffset>>,
        to: Option<DateTime<FixedOffset>>,
        filters: HashMap<String, String>,
    ) -> io::Result<()> {
        debug!("Listing notes with filters: {:?}", filters);
        let entries = fs::read_dir(&self.notes_dir)?;
        let mut notes = Vec::new();
        let mut found_notes = false;

        for entry in entries {
            if let Ok(entry) = entry {
                let path = entry.path();

                if path
                    .extension()
                    .and_then(|ext| ext.to_str())
                    .map_or(false, |ext| ext == self.settings.file_type)
                {
                    match Note::from_file(&path) {
                        Ok(Some((title, frontmatter_fields))) => {
                            let timestamp_str = frontmatter_fields.get("timestamp").unwrap();
                            let timestamp =
                                DateTime::parse_from_str(timestamp_str, "%Y-%m-%d %H:%M:%S %z")
                                    .unwrap()
                                    .with_timezone(&Local);

                            let include = match (from, to) {
                                (Some(from), Some(to)) => timestamp >= from && timestamp <= to,
                                (Some(from), None) => timestamp >= from,
                                (None, Some(to)) => timestamp <= to,
                                (None, None) => true,
                            };

                            let matches_filters = filters.iter().all(|(key, value)| {
                                frontmatter_fields.get(key).map_or(false, |v| v == value)
                            });

                            if include && matches_filters {
                                notes.push((title, timestamp));
                                found_notes = true;
                            }
                        }
                        Ok(None) => {}
                        Err(_) => {}
                    }
                }
            }
        }

        notes.sort_by(|a, b| b.1.cmp(&a.1));

        info!("Notes:");
        for (title, timestamp) in notes {
            info!("- {} ({})", title, timestamp.format("%Y-%m-%d %H:%M:%S"));
        }

        if !found_notes {
            info!("No notes found in the specified time range");
        }
        Ok(())
    }

    pub fn parse_date_string(date_str: &str) -> Result<DateTime<FixedOffset>, String> {
        let formats = ["%Y-%m-%d", "%Y-%m-%d %H:%M:%S", "%Y-%m-%d %H:%M:%S %z"];
        for format in formats {
            if let Ok(dt) = DateTime::parse_from_str(date_str, format) {
                return Ok(dt);
            }
        }
        Err(format!("Invalid dateformat '{}'", date_str))
    }

    pub async fn sync_notes(&self) -> io::Result<()> {
        debug!("Starting note sync process");
        let observers = self.observer_registry.get_observers().await;

        let entries = fs::read_dir(&self.notes_dir).map_err(|e| {
            error!("Failed to read notes directory: {}", e);
            e
        })?;

        for entry in entries {
            let entry = entry?;
            let path = entry.path();

            if path
                .extension()
                .and_then(|ext| ext.to_str())
                .map_or(false, |ext| ext == self.settings.file_type)
            {
                let title = path
                    .file_stem()
                    .and_then(|s| s.to_str())
                    .unwrap_or_default();

                debug!("Processing note: {}", title);

                match Note::from_file(&path) {
                    Ok(Some((content, current_frontmatter))) => {
                        if !self.should_process_note(title, &content).await {
                            info!("️ Content unchanged for '{}', skipping observers", title);
                            continue;
                        }

                        let skip_observers: Vec<String> = if current_frontmatter
                            .get("skip_observers")
                            .map_or(false, |s| s.trim() == "all")
                        {
                            debug!("Skipping all observers");
                            observers.iter().map(|o| o.name()).collect()
                        } else {
                            current_frontmatter
                                .get("skip_observers")
                                .map(|s| {
                                    debug!("Processing skip_observers: {}", s);
                                    s.split(',')
                                        .map(|s| s.trim().to_string())
                                        .filter(|s| !s.is_empty())
                                        .collect()
                                })
                                .unwrap_or_default()
                        };

                        let active_observers: Vec<_> = observers
                            .iter()
                            .filter(|o| !skip_observers.contains(&o.name()))
                            .collect();

                        info!(
                            "🔄 Processing {} active observers...",
                            active_observers.len()
                        );
                        if !skip_observers.is_empty() {
                            info!("ℹ️ Skipping observers: {}", skip_observers.join(", "));
                        }

                        let event = NoteEvent::Synced {
                            title: title.to_string(),
                            content: content.clone(),
                            file_path: get_absolute_note_path(title, &self.settings),
                            frontmatter: current_frontmatter.clone(),
                        };

                        let mut combined_metadata = current_frontmatter;
                        let mut final_content = content.clone();

                        // Process all non-SQLite observers first
                        for observer in &active_observers {
                            if observer.name() == "sqlite" {
                                continue;
                            }

                            info!("🔵 Running observer: {}", observer.name());
                            match observer.on_event_boxed(event.clone()).await {
                                Ok(Some(result)) => {
                                    if let Some(metadata) = result.metadata {
                                        debug!("Observer returned metadata: {:?}", metadata);
                                        merge_metadata(&mut combined_metadata, metadata);
                                    }
                                    if let Some(new_content) = result.content {
                                        debug!("Observer modified content");
                                        final_content = new_content;
                                    }
                                }
                                Ok(None) => debug!("No changes from observer: {}", observer.name()),
                                Err(e) => error!("Error from observer {}: {}", observer.name(), e),
                            }
                        }

                        let updated_note =
                            Note::new(title.to_string(), final_content, combined_metadata.clone())
                                .await;
                        updated_note.save(&self.settings).await?;

                        // Run SQLite observer last
                        if let Some(sqlite_observer) =
                            observers.iter().find(|o| o.name() == "sqlite")
                        {
                            info!("🔵 Running SQLite observer");
                            match sqlite_observer.on_event_boxed(event).await {
                                Ok(Some(result)) => {
                                    if let Some(new_content) = result.content {
                                        info!("✨ SQLite observer modified content");
                                        let updated_note = Note::new(
                                            title.to_string(),
                                            new_content,
                                            combined_metadata.clone(),
                                        )
                                        .await;
                                        updated_note.save(&self.settings).await?;
                                    }
                                }
                                Ok(None) => debug!("No changes from SQLite observer"),
                                Err(e) => {
                                    error!("SQLite observer error: {}", e);
                                    return Err(e);
                                }
                            }
                        }

                        info!("✨ Note sync completed for: {}", title);
                    }
                    Ok(None) => {
                        warn!("Could not parse note: {}", title);
                    }
                    Err(e) => {
                        error!("Error reading note {}: {}", title, e);
                        return Err(e);
                    }
                }
            }
        }

        info!("🎉 All notes synced successfully");
        Ok(())
    }

    pub async fn sync_single_note(&self, title: &str, skip_hash_check: bool) -> io::Result<()> {
        let path = get_absolute_note_path(title, &self.settings);
        debug!("Syncing note at path: {}", path);
        if Path::new(&path).exists() {
            // Get observers
            let observers = self.observer_registry.get_observers().await;

            match Note::from_file(Path::new(&path)) {
                Ok(Some((content, current_frontmatter))) => {
                    info!("✅ Successfully read note: {}", title);
                    if !skip_hash_check && !self.should_process_note(&title, &content).await {
                        info!("⏭️ Content unchanged for '{}', skipping observers", title);
                        return Ok(());
                    }

                    let skip_observers: Vec<String> = if current_frontmatter
                        .get("skip_observers")
                        .map_or(false, |s| s.trim() == "all")
                    {
                        debug!("Skipping all observers");
                        observers.iter().map(|o| o.name()).collect()
                    } else {
                        current_frontmatter
                            .get("skip_observers")
                            .map(|s| {
                                debug!("Processing skip_observers: {}", s);
                                s.split(',')
                                    .map(|s| s.trim().to_string())
                                    .filter(|s| !s.is_empty())
                                    .collect()
                            })
                            .unwrap_or_default()
                    };

                    // Filter out skipped observers first
                    let active_observers: Vec<_> = observers
                        .iter()
                        .filter(|o| !skip_observers.contains(&o.name().to_string()))
                        .collect();

                    info!(
                        "🔄 Processing {} active observers...",
                        active_observers.len()
                    );
                    if !skip_observers.is_empty() {
                        info!("ℹ️ Skipping observers: {}", skip_observers.join(", "));
                    }

                    // Create a sync event for observers
                    let event = NoteEvent::Synced {
                        title: title.to_string(),
                        content: content.clone(),
                        file_path: get_absolute_note_path(title, &self.settings),
                        frontmatter: current_frontmatter.clone(),
                    };

                    let mut combined_metadata = current_frontmatter;
                    let mut final_content = content.clone();

                    // Process all non-SQLite observers first
                    for observer in &active_observers {
                        if observer.name() == "sqlite" {
                            continue;
                        }

                        info!("🔵 Running observer: {}", observer.name());
                        match observer.on_event_boxed(event.clone()).await {
                            Ok(Some(result)) => {
                                if let Some(metadata) = result.metadata {
                                    info!("✅ Observer returned metadata: {:?}", metadata);
                                    merge_metadata(&mut combined_metadata, metadata);
                                }
                                if let Some(new_content) = result.content {
                                    info!("✅ Observer modified content");
                                    final_content = new_content;
                                }
                            }
                            Ok(None) => info!("ℹ️ No changes from observer: {}", observer.name()),
                            Err(e) => error!("Error from observer {}: {}", observer.name(), e),
                        }
                    }
                    debug!("Final content: {}", final_content);

                    let updated_note =
                        Note::new(title.to_string(), final_content, combined_metadata.clone())
                            .await;
                    updated_note.save(&self.settings).await?;

                    // Run SQLite observer last
                    if let Some(sqlite_observer) = observers.iter().find(|o| o.name() == "sqlite") {
                        info!("🔵 Running SQLite observer");
                        match sqlite_observer.on_event_boxed(event).await {
                            Ok(Some(result)) => {
                                if let Some(new_content) = result.content {
                                    info!("✨ SQLite observer modified content");
                                    let updated_note = Note::new(
                                        title.to_string(),
                                        new_content,
                                        combined_metadata.clone(),
                                    )
                                    .await;
                                    updated_note.save(&self.settings).await?;
                                }
                            }
                            Ok(None) => debug!("No changes from SQLite observer"),
                            Err(e) => {
                                error!("SQLite observer error: {}", e);
                                return Err(e);
                            }
                        }
                    }

                    info!("✨ Note sync completed for: {}", title);
                }
                Ok(None) => {
                    warn!("Could not parse note: {}", title);
                }
                Err(e) => {
                    error!("Error reading note {}: {}", title, e);
                    return Err(e);
                }
            }
        }
        Ok(())
    }

    fn calculate_content_hash(content: &str) -> String {
        debug!("Calculating content hash");
        let mut hasher = Sha256::new();
        hasher.update(content.as_bytes());
        format!("{:x}", hasher.finalize())
    }

    fn get_hash_cache(&self) -> HashMap<String, String> {
        let cache_path = ProjectDirs::from("", "norg", "norg")
            .map(|proj_dirs| proj_dirs.data_dir().join("content_hashes.json"))
            .unwrap_or_else(|| PathBuf::from("./data/content_hashes.json"));

        debug!("Reading hash cache from: {}", cache_path.display());

        // Create parent directory if it doesn't exist
        if let Some(parent) = cache_path.parent() {
            if let Err(e) = fs::create_dir_all(parent) {
                error!("Failed to create hash cache directory: {}", e);
                return HashMap::new();
            }
        }

        if cache_path.exists() {
            match fs::read_to_string(&cache_path) {
                Ok(content) => match serde_json::from_str(&content) {
                    Ok(cache) => {
                        debug!("Successfully loaded hash cache");
                        cache
                    }
                    Err(e) => {
                        error!("Failed to parse hash cache: {}", e);
                        HashMap::new()
                    }
                },
                Err(e) => {
                    error!("Failed to read hash cache file: {}", e);
                    HashMap::new()
                }
            }
        } else {
            debug!("No existing hash cache found");
            HashMap::new()
        }
    }

    fn save_hash_cache(&self, cache: &HashMap<String, String>) -> io::Result<()> {
        let cache_path = ProjectDirs::from("", "norg", "norg")
            .map(|proj_dirs| proj_dirs.data_dir().join("content_hashes.json"))
            .unwrap_or_else(|| PathBuf::from("./data/content_hashes.json"));

        debug!("Saving hash cache to: {}", cache_path.display());

        // Create parent directory if it doesn't exist
        if let Some(parent) = cache_path.parent() {
            fs::create_dir_all(parent)?;
        }

        let json = serde_json::to_string_pretty(cache).map_err(|e| {
            error!("Failed to serialize hash cache: {}", e);
            io::Error::new(io::ErrorKind::Other, e)
        })?;

        fs::write(&cache_path, json).map_err(|e| {
            error!("Failed to write hash cache: {}", e);
            e
        })?;

        debug!("Hash cache saved successfully");
        Ok(())
    }

    pub async fn should_process_note(&self, title: &str, content: &str) -> bool {
        debug!("Checking if note needs processing: {}", title);
        let mut hash_cache = self.get_hash_cache();
        let new_hash = Self::calculate_content_hash(content);

        let should_process = match hash_cache.get(title) {
            Some(old_hash) => {
                debug!(
                    "Comparing hashes for '{}': old={}, new={}",
                    title, old_hash, new_hash
                );
                old_hash != &new_hash
            }
            None => {
                debug!("No previous hash found for '{}'", title);
                true
            }
        };

        if should_process {
            debug!("Content changed, updating hash cache");
            hash_cache.insert(title.to_string(), new_hash);
            if let Err(e) = self.save_hash_cache(&hash_cache) {
                error!("Failed to save hash cache: {}", e);
            }
        } else {
            debug!("Content unchanged for '{}'", title);
        }

        should_process
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_frontmatter_serialization() {
        let mut fields = HashMap::new();
        fields.insert("tags".to_string(), "test, example".to_string());
        fields.insert("timestamp".to_string(), "2024-03-14".to_string());

        let frontmatter = Frontmatter { fields };
        let yaml = serde_yaml::to_string(&frontmatter).unwrap();

        assert!(!yaml.contains("fields:"));
        assert!(yaml.contains("tags: test, example"));
        assert!(yaml.contains("timestamp: 2024-03-14"));
    }
}
