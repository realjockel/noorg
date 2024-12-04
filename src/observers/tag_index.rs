use tracing::{debug, info};

use crate::event::{NoteEvent, NoteObserver, ObserverResult};
use crate::settings::Settings;
use std::any::Any;
use std::collections::{BTreeMap, HashMap};
use std::fs::File;
use std::future::Future;
use std::io::{self, Read, Write};
use std::path::Path;
use std::pin::Pin;
use std::sync::Arc;

pub struct TagIndexObserver {
    index_path: String,
    settings: Arc<Settings>,
}

impl TagIndexObserver {
    pub fn new(settings: Arc<Settings>) -> io::Result<Self> {
        // Use the configured notes directory
        let index_path = Path::new(&settings.note_dir).join("_tag_index.md");

        // Create empty index file if it doesn't exist
        if !index_path.exists() {
            let mut file = File::create(&index_path)?;
            writeln!(file, "# Tag Index\n")?;
        }

        Ok(Self {
            index_path: index_path.to_str().unwrap_or("_tag_index.md").to_string(),
            settings,
        })
    }

    fn parse_index(&self) -> io::Result<BTreeMap<String, Vec<(String, String)>>> {
        let mut content = String::new();
        File::open(&self.index_path)?.read_to_string(&mut content)?;

        let mut index: BTreeMap<String, Vec<(String, String)>> = BTreeMap::new();
        let mut current_tag = String::new();

        for line in content.lines() {
            if line.starts_with("## ") {
                current_tag = line[3..].trim().to_string();
            } else if line.starts_with("- ") && !current_tag.is_empty() {
                if let Some(link_start) = line.find('[') {
                    if let Some(link_end) = line.find(']') {
                        if let Some(path_start) = line.find('(') {
                            if let Some(path_end) = line.find(')') {
                                let title = line[link_start + 1..link_end].to_string();
                                let path = line[path_start + 1..path_end].to_string();
                                index
                                    .entry(current_tag.clone())
                                    .or_default()
                                    .push((title, path));
                            }
                        }
                    }
                }
            }
        }

        Ok(index)
    }

    fn write_index(&self, index: &BTreeMap<String, Vec<(String, String)>>) -> io::Result<()> {
        // First read existing content to preserve frontmatter
        let existing_content = if let Ok(mut content) =
            File::open(&self.index_path).and_then(|mut f| {
                let mut content = String::new();
                f.read_to_string(&mut content)?;
                Ok(content)
            }) {
            content
        } else {
            String::new()
        };

        // Extract frontmatter if it exists
        let frontmatter = if existing_content.starts_with("---") {
            if let Some(end) = existing_content.find("---\n") {
                if let Some(second) = existing_content[end + 4..].find("---") {
                    Some(existing_content[..end + second + 7].to_string())
                } else {
                    None
                }
            } else {
                None
            }
        } else {
            None
        };

        let mut file = File::create(&self.index_path)?;

        // Write frontmatter if it exists
        if let Some(fm) = frontmatter {
            writeln!(file, "{}\n", fm)?;
        }

        writeln!(file, "# _tag_index\n")?;

        for (tag, entries) in index {
            writeln!(file, "## {}\n", tag)?;

            for (title, path) in entries {
                writeln!(file, "- [{}]({})", title, path)?;
            }
            writeln!(file)?;
        }

        Ok(())
    }

    fn update_index(&self, title: &str, _file_path: &str, tags: &[String]) -> io::Result<()> {
        let mut index = self.parse_index()?;

        // Remove existing entries for this note
        for entries in index.values_mut() {
            entries.retain(|(t, _)| t != title);
        }

        // Add new entries with relative paths
        for tag in tags {
            let tag = tag.trim();
            if !tag.is_empty() && !tag.starts_with("tags:") {
                // Use relative path from the notes directory
                let file_path = format!("./{}.{}", title, self.settings.file_type);
                index
                    .entry(tag.to_string())
                    .or_default()
                    .push((title.to_string(), file_path));
            }
        }

        // Sort entries within each tag
        for entries in index.values_mut() {
            entries.sort_by(|a, b| a.0.cmp(&b.0));
        }

        // Remove empty tags
        index.retain(|_, entries| !entries.is_empty());

        self.write_index(&index)
    }

    fn parse_frontmatter_tags(&self, frontmatter: &str) -> Vec<String> {
        frontmatter
            .lines()
            .find(|line| line.trim().starts_with("tags:"))
            .map(|tags_line| {
                tags_line
                    .trim_start_matches("tags:")
                    .split(',')
                    .map(|s| s.trim().to_string())
                    .filter(|s| !s.is_empty() && !s.starts_with("tags:"))
                    .collect()
            })
            .unwrap_or_default()
    }
}

impl NoteObserver for TagIndexObserver {
    fn on_event_boxed(
        &self,
        event: NoteEvent,
    ) -> Pin<Box<dyn Future<Output = io::Result<Option<ObserverResult>>> + Send + '_>> {
        Box::pin(async move {
            match event {
                NoteEvent::Created {
                    title,
                    file_path,
                    frontmatter,
                    ..
                }
                | NoteEvent::Updated {
                    title,
                    file_path,
                    frontmatter,
                    ..
                }
                | NoteEvent::Synced {
                    title,
                    file_path,
                    frontmatter,
                    ..
                } => {
                    // Extract tags from frontmatter directly
                    if let Some(tags) = frontmatter.get("tags") {
                        let tags: Vec<String> = tags
                            .split(',')
                            .map(|s| s.trim().to_string())
                            .filter(|s| !s.is_empty() && !s.starts_with("tags:"))
                            .collect();

                        debug!(
                            "🏷️ Updating tag index for '{}' with tags: {:?}",
                            title, tags
                        );
                        self.update_index(&title, &file_path, &tags)?;
                        info!("✅ Tag index updated successfully");

                        // Return the tags in the metadata
                        let mut metadata = HashMap::new();
                        metadata.insert("tags".to_string(), tags.join(", "));

                        Ok(Some(ObserverResult {
                            metadata: Some(metadata),
                            content: None,
                        }))
                    } else {
                        Ok(None)
                    }
                }
            }
        })
    }

    fn name(&self) -> String {
        "tag_index".to_string()
    }

    fn as_any(&self) -> &dyn Any {
        self
    }

    fn priority(&self) -> i32 {
        -99 // Run after metadata generation but before storage
    }
}
