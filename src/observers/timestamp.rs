use crate::event::{NoteEvent, NoteObserver, ObserverResult};
use chrono::Local;
use std::any::Any;
use std::collections::HashMap;
use std::future::Future;
use std::pin::Pin;
use tracing::{debug, info};

pub struct TimestampObserver;

impl NoteObserver for TimestampObserver {
    fn on_event_boxed(
        &self,
        event: NoteEvent,
    ) -> Pin<Box<dyn Future<Output = std::io::Result<Option<ObserverResult>>> + Send + '_>> {
        Box::pin(async move {
            let mut metadata = HashMap::new();
            match event {
                NoteEvent::Created {
                    title, frontmatter, ..
                } => {
                    debug!("Processing creation timestamp for '{}'", title);

                    if !frontmatter.contains_key("created_at") {
                        let created_at = Local::now().format("%Y-%m-%d %H:%M:%S %z").to_string();
                        debug!("Setting initial created_at: {}", created_at);
                        metadata.insert("created_at".to_string(), created_at);
                    }

                    let updated_at = Local::now().format("%Y-%m-%d %H:%M:%S.%f %z").to_string();
                    debug!("Setting updated_at: {}", updated_at);
                    metadata.insert("updated_at".to_string(), updated_at);

                    info!("✨ Timestamps initialized for new note '{}'", title);
                }
                NoteEvent::Updated {
                    title, frontmatter, ..
                }
                | NoteEvent::Synced {
                    title, frontmatter, ..
                } => {
                    debug!("Processing update timestamp for '{}'", title);

                    if let Some(created) = frontmatter.get("created_at") {
                        debug!("Preserving existing created_at: {}", created);
                        metadata.insert("created_at".to_string(), created.clone());
                    }

                    let updated_at = Local::now().format("%Y-%m-%d %H:%M:%S.%f %z").to_string();
                    debug!("Setting updated_at: {}", updated_at);
                    metadata.insert("updated_at".to_string(), updated_at);

                    info!("✨ Updated timestamp for '{}'", title);
                }
            }

            Ok(Some(ObserverResult {
                metadata: Some(metadata),
                content: None,
            }))
        })
    }

    fn name(&self) -> String {
        "timestamp".to_string()
    }

    fn as_any(&self) -> &dyn Any {
        self
    }

    fn priority(&self) -> i32 {
        0 // Run after metadata generation but before storage
    }
}
