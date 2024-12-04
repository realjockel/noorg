use crate::event::{NoteEvent, NoteObserver, ObserverResult};
use kalosm::language::*;
use rusqlite::{Connection, Error as SqliteError};
use std::any::Any;
use std::collections::HashMap;
use std::future::Future;
use std::io;
use std::pin::Pin;
use std::sync::Arc;
use tokio::sync::Mutex;
use tracing::{debug, error};

#[derive(Clone, Debug, Parse, Schema)]
struct NoteMetadata {
    tags: Vec<String>,
    summary: String,
}

pub struct LlmMetadataObserver {
    conn: Arc<Mutex<Connection>>,
}

impl LlmMetadataObserver {
    pub fn new() -> io::Result<Self> {
        let conn = Connection::open("data/frontmatter.db").map_err(|e| {
            error!("Failed to open frontmatter database: {}", e);
            io::Error::new(io::ErrorKind::Other, e)
        })?;
        debug!("LlmMetadataObserver initialized successfully");
        Ok(Self {
            conn: Arc::new(Mutex::new(conn)),
        })
    }

    async fn get_existing_tags(&self) -> io::Result<Vec<String>> {
        let conn = self.conn.lock().await;
        debug!("Fetching existing tags from database");
        let mut stmt = conn
            .prepare("SELECT DISTINCT value FROM frontmatter WHERE key = 'tags'")
            .map_err(|e| {
                error!("Failed to prepare tags query: {}", e);
                io::Error::new(io::ErrorKind::Other, e)
            })?;

        let tags: Result<Vec<String>, SqliteError> = stmt
            .query_map([], |row| row.get(0))
            .map_err(|e| io::Error::new(io::ErrorKind::Other, e))?
            .collect();

        let all_tags: Vec<String> = tags
            .map_err(|e| io::Error::new(io::ErrorKind::Other, e))?
            .into_iter()
            .flat_map(|t| {
                t.split(',')
                    .map(|s| s.trim().to_string())
                    .collect::<Vec<_>>()
            })
            .collect();

        debug!("Found {} existing tags", all_tags.len());
        Ok(all_tags)
    }
}

impl NoteObserver for LlmMetadataObserver {
    fn on_event_boxed(
        &self,
        event: NoteEvent,
    ) -> Pin<Box<dyn Future<Output = io::Result<Option<ObserverResult>>> + Send + '_>> {
        let conn = self.conn.clone();

        Box::pin(async move {
            match event {
                NoteEvent::Created { content, title, .. }
                | NoteEvent::Updated { content, title, .. }
                | NoteEvent::Synced { content, title, .. } => {
                    debug!("Processing note '{}' with LLM metadata observer", title);
                    let llm = Llama::new_chat().await.map_err(|e| {
                        error!("Failed to initialize LLM: {}", e);
                        io::Error::new(
                            io::ErrorKind::Other,
                            format!("Failed to initialize LLM: {}", e),
                        )
                    })?;

                    // Get existing tags
                    let observer = LlmMetadataObserver { conn };
                    let existing_tags = observer.get_existing_tags().await?;
                    let tags_list = existing_tags.join(", ");
                    debug!("Using existing tags list: {}", tags_list);

                    let task = Task::builder_for::<NoteMetadata>(format!(
                        "Analyze the text content and extract key metadata:
                         - Select 0-5 tags ONLY from this list: {}
                         - Write a brief summary (EXACTLY 3 sentences)
                         
                         The tags MUST be selected from the provided list only.
                         The summary MUST be exactly 3 sentences.",
                        tags_list
                    ))
                    .build();

                    debug!("Running LLM analysis for note '{}'", title);
                    let metadata = task.run(&content, &llm).await.map_err(|e| {
                        error!("Failed to generate metadata for '{}': {}", title, e);
                        io::Error::new(
                            io::ErrorKind::Other,
                            format!("Failed to generate metadata: {}", e),
                        )
                    })?;

                    // Clean up and validate metadata
                    let mut fields = HashMap::new();

                    // Filter and limit tags
                    let clean_tags: Vec<String> = metadata
                        .tags
                        .into_iter()
                        .filter(|t| existing_tags.contains(t))
                        .take(5)
                        .collect();
                    fields.insert("tags".to_string(), clean_tags.join(", "));

                    // Ensure exactly 3 sentences
                    let summary = metadata
                        .summary
                        .split('.')
                        .filter(|s| !s.trim().is_empty())
                        .take(3)
                        .map(|s| s.trim().to_string())
                        .collect::<Vec<_>>()
                        .join(". ")
                        + ".";
                    fields.insert("summary".to_string(), summary);

                    Ok(Some(ObserverResult {
                        metadata: Some(fields),
                        content: None,
                    }))
                }
            }
        })
    }

    fn name(&self) -> String {
        "llm_metadata".to_string()
    }

    fn as_any(&self) -> &dyn Any {
        self
    }

    fn priority(&self) -> i32 {
        0
    }
}
