use crate::event::ObserverResult;
use crate::event::{NoteEvent, NoteObserver};
use crate::settings::Settings;
use kalosm::language::*;
use std::any::Any;
use std::future::Future;
use std::io;
use std::pin::Pin;
use std::sync::Arc;
use surrealdb::engine::local::Db;
use surrealdb::{engine::local::RocksDb, Surreal};
use tokio::sync::Mutex;
use tracing::{debug, error, info};

#[derive(Clone)]
pub struct SimilarNotesObserver {
    db: Arc<Mutex<Surreal<Db>>>,
    document_table: Arc<Mutex<DocumentTable<Db, Document, Bert, SemanticChunker>>>,
    bert: Arc<Mutex<Bert>>,
    settings: Arc<Settings>,
}

impl SimilarNotesObserver {
    pub fn new(settings: Arc<Settings>) -> io::Result<Self> {
        let db_dir = std::path::PathBuf::from("db");
        std::fs::create_dir_all(&db_dir)?;

        debug!("Initializing SimilarNotesObserver");
        let (db, document_table, bert) = tokio::task::block_in_place(|| {
            tokio::runtime::Handle::current().block_on(async {
                debug!("Creating BERT model");
                let bert = Bert::new().await.map_err(|e| {
                    error!("Failed to initialize BERT model: {}", e);
                    io::Error::new(io::ErrorKind::Other, e)
                })?;

                debug!("Connecting to embeddings database");
                let db = Surreal::new::<RocksDb>("./db/embeddings.db")
                    .await
                    .map_err(|e| {
                        error!("Failed to connect to embeddings database: {}", e);
                        io::Error::new(io::ErrorKind::Other, e)
                    })?;

                db.use_ns("test").use_db("test").await.map_err(|e| {
                    error!("Failed to select database namespace: {}", e);
                    io::Error::new(io::ErrorKind::Other, e)
                })?;

                debug!("Creating document table");
                let chunker = SemanticChunker::new();
                let document_table = db
                    .document_table_builder("documents")
                    .with_chunker(chunker)
                    .at("./db/embeddings.db")
                    .build()
                    .await
                    .map_err(|e| {
                        error!("Failed to create document table: {}", e);
                        io::Error::new(io::ErrorKind::Other, e)
                    })?;

                Ok::<_, io::Error>((
                    Arc::new(Mutex::new(db)),
                    Arc::new(Mutex::new(document_table)),
                    Arc::new(Mutex::new(bert)),
                ))
            })
        })?;

        info!("✨ SimilarNotesObserver initialized successfully");
        Ok(Self {
            db,
            document_table,
            bert,
            settings,
        })
    }

    async fn find_similar_notes(
        &self,
        content: &str,
        current_path: &str,
    ) -> io::Result<Vec<(String, String, f32)>> {
        debug!("Starting similarity search for note at {}", current_path);
        let clean_content = Self::extract_content(content);

        let user_question_embedding =
            self.bert
                .lock()
                .await
                .embed(&clean_content)
                .await
                .map_err(|e| {
                    error!("Failed to create embedding: {}", e);
                    io::Error::new(io::ErrorKind::Other, e)
                })?;

        debug!("Finding nearest neighbors in embedding space");
        let similar_docs = self
            .document_table
            .lock()
            .await
            .select_nearest(user_question_embedding, 10)
            .await
            .map_err(|e| {
                error!("Failed to find similar documents: {}", e);
                io::Error::new(io::ErrorKind::Other, e)
            })?;

        let current_title = current_path
            .split('/')
            .last()
            .and_then(|s| s.strip_suffix(".md"))
            .unwrap_or("");

        let excluded = self
            .settings
            .similar_notes
            .excluded_from_references
            .as_ref()
            .map(|v| v.as_slice())
            .unwrap_or(&[]);

        let mut similar_notes: Vec<_> = similar_docs
            .into_iter()
            .filter(|doc| {
                let title = doc.record.title();
                !excluded.contains(&title.to_string()) && title != current_title
            })
            .map(|doc| {
                let title = doc.record.title().to_string();
                let path = format!("./{}.{}", title, self.settings.file_type);
                let score = doc.distance;
                debug!("Similarity score for {}: {:.2}%", title, (score * 100.0));
                (title, path, score)
            })
            .filter(|(_, _, score)| *score > 0.1)
            .collect();

        similar_notes.sort_by(|a, b| b.2.partial_cmp(&a.2).unwrap_or(std::cmp::Ordering::Equal));
        similar_notes.truncate(5);

        info!("Found {} similar notes", similar_notes.len());
        Ok(similar_notes)
    }

    async fn update_embeddings(&self, title: String, content: String) -> io::Result<()> {
        debug!("Updating embeddings for note: {}", title);
        let clean_content = Self::extract_content(&content);

        if clean_content.trim().is_empty() {
            debug!("Skipping empty content for {}", title);
            return Ok(());
        }

        let document = Document::from_parts(title.clone(), clean_content);
        debug!("Created document for {}, performing upsert", title);

        let table = self.document_table.lock().await;
        let safe_id = title
            .replace(|c: char| !c.is_alphanumeric(), "_")
            .to_lowercase();
        let id = surrealdb::sql::Id::from(safe_id);

        table.update(id, document).await.map_err(|e| {
            error!("Failed to upsert document for {}: {}", title, e);
            io::Error::new(io::ErrorKind::Other, e)
        })?;

        info!("✨ Successfully updated embeddings for {}", title);
        Ok(())
    }

    fn extract_content(text: &str) -> String {
        // Remove frontmatter and get clean content
        if let Some(start) = text.find("---\n") {
            if let Some(end) = text[start + 4..].find("\n---\n") {
                return text[start + 4 + end + 5..].trim().to_string();
            }
        }
        text.trim().to_string()
    }

    fn append_references(&self, content: &str, similar_notes: &[(String, String, f32)]) -> String {
        let mut new_content = content.to_string();

        // Remove existing references section if it exists
        if let Some(idx) = new_content.rfind("\n## Similar Notes\n") {
            new_content.truncate(idx);
        }

        // Add new references section
        if !similar_notes.is_empty() {
            new_content.push_str("\n## Similar Notes\n\n");

            for (title, _path, similarity) in similar_notes {
                let similarity_percent = (similarity * 100.0).round();
                let file_path = format!("./{}.{}", title, self.settings.file_type);
                new_content.push_str(&format!(
                    "- [{}]({}) ({}% similar)\n",
                    title, file_path, similarity_percent
                ));
            }
        }

        new_content
    }
}

impl NoteObserver for SimilarNotesObserver {
    fn on_event_boxed(
        &self,
        event: NoteEvent,
    ) -> Pin<Box<dyn Future<Output = io::Result<Option<ObserverResult>>> + Send + '_>> {
        Box::pin(async move {
            match event {
                NoteEvent::Created {
                    title,
                    content,
                    file_path,
                    ..
                }
                | NoteEvent::Updated {
                    title,
                    content,
                    file_path,
                    ..
                }
                | NoteEvent::Synced {
                    title,
                    content,
                    file_path,
                    ..
                } => {
                    if let Some(excluded_notes) = &self.settings.similar_notes.excluded_notes {
                        if excluded_notes.contains(&title) {
                            debug!("Similar notes disabled for: {}", title);
                            return Ok(None);
                        }
                    }

                    debug!("Processing note '{}' for similar notes", title);

                    match self.update_embeddings(title.clone(), content.clone()).await {
                        Ok(_) => debug!("Embeddings updated for '{}'", title),
                        Err(e) => error!("Failed to update embeddings for '{}': {}", title, e),
                    };

                    debug!("Finding similar notes for '{}'", title);
                    let similar_notes = self.find_similar_notes(&content, &file_path).await?;

                    if !similar_notes.is_empty() {
                        let new_content = self.append_references(&content, &similar_notes);
                        info!("✨ Added {} references to '{}'", similar_notes.len(), title);
                        Ok(Some(ObserverResult {
                            metadata: None,
                            content: Some(new_content),
                        }))
                    } else {
                        debug!("No similar notes found for '{}'", title);
                        Ok(None)
                    }
                }
            }
        })
    }

    fn name(&self) -> String {
        "similar_notes".to_string()
    }

    fn as_any(&self) -> &dyn Any {
        self
    }

    fn priority(&self) -> i32 {
        0
    }
}
