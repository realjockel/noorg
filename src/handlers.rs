use std::collections::HashMap;
use std::io;
use std::sync::atomic::AtomicBool;
use std::sync::Arc;
use tracing::{debug, error, info, warn};

use crate::cli::Command;
use crate::editor::open_editor;
use crate::note::NoteManager;
use crate::observers::sqlite_store::SqliteObserver;
use crate::settings::Settings;
use crate::watcher::watch_directory;
use crate::{observer_registry::ObserverRegistry, observers};

pub async fn handle_command(
    command: Command,
    settings: Settings,
    observer_registry: Arc<ObserverRegistry>,
    stop_signal: Option<Arc<AtomicBool>>,
) -> io::Result<()> {
    debug!("Initializing note manager");
    let note_manager = NoteManager::new(settings.clone(), observer_registry.clone()).await?;

    match command {
        Command::List { from, to, filter } => {
            debug!("Handling list command with filters: {:?}", filter);

            let from_date = from
                .map(|d| NoteManager::parse_date_string(&d))
                .transpose()
                .map_err(|e| {
                    error!("Invalid 'from' date format: {}", e);
                    io::Error::new(io::ErrorKind::InvalidInput, e)
                })?;

            let to_date = to
                .map(|d| NoteManager::parse_date_string(&d))
                .transpose()
                .map_err(|e| {
                    error!("Invalid 'to' date format: {}", e);
                    io::Error::new(io::ErrorKind::InvalidInput, e)
                })?;

            let filters: HashMap<String, String> = filter.into_iter().collect();
            debug!(
                "Listing notes with date range {:?} to {:?}",
                from_date, to_date
            );
            note_manager.list_notes_with_filter(from_date, to_date, filters)?;
        }
        Command::Add {
            title,
            body,
            frontmatter,
        } => {
            debug!("Handling add command for note '{}'", title);

            let content = match body {
                Some(text) => {
                    debug!("Using provided content for note");
                    text
                }
                None => {
                    info!("Opening editor for note content...");
                    open_editor("", &settings)?
                }
            };

            if content.trim().is_empty() {
                warn!("Note creation cancelled - empty content");
                return Ok(());
            }

            let mut frontmatter_data = HashMap::new();
            for (key, value) in frontmatter {
                match key.as_str() {
                    "tags" => {
                        let tags: Vec<String> = value
                            .split(',')
                            .map(|s| s.trim().to_string())
                            .filter(|s| !s.is_empty())
                            .collect();
                        debug!("Processing tags: {:?}", tags);
                        frontmatter_data.insert(key, tags.join(", "));
                    }
                    _ => {
                        debug!("Adding frontmatter: {} = {}", key, value);
                        frontmatter_data.insert(key, value);
                    }
                }
            }

            note_manager
                .add_note(title.clone(), content, frontmatter_data)
                .await?;
            info!("✨ Note '{}' added successfully", title);
        }
        Command::Delete { title } => {
            debug!("Handling delete command for note '{}'", title);
            note_manager.delete_note(&title)?;
            info!("🗑️ Note '{}' deleted successfully", title);
        }
        Command::Sync => {
            info!("🔄 Syncing all notes with observers...");
            note_manager.sync_notes().await?;
            info!("✨ Sync completed successfully");
        }
        Command::ListObservers => {
            info!("📋 Available Rust observers:");
            for observer in observers::get_available_observers() {
                info!("- {}", observer);
            }
        }
        Command::Query { query } => {
            debug!("Handling query command: {}", query);
            let observers = observer_registry.get_observers().await;
            let sqlite_observer = observers
                .iter()
                .find(|o| o.name() == "sqlite")
                .and_then(|o| o.as_any().downcast_ref::<SqliteObserver>())
                .ok_or_else(|| {
                    error!("SQLite observer not found in registry");
                    io::Error::new(
                        io::ErrorKind::NotFound,
                        "SQLite observer not found in registry",
                    )
                })?;

            debug!("Executing {} query", &query);
            let results = sqlite_observer.query(&query).await?;

            if results.rows.is_empty() {
                info!("No matching notes found");
            } else {
                debug!("Found {} matching notes", results.rows.len());
                print_query_results(&results);
                info!("📊 Found {} notes", results.rows.len());
            }
        }
        Command::Watch => {
            let stop = stop_signal.unwrap_or_else(|| Arc::new(AtomicBool::new(false)));
            watch_directory(settings, observer_registry, stop).await?
        }
    }

    Ok(())
}

fn print_query_results(results: &crate::observers::sqlite_store::QueryResult) {
    // Print column headers
    println!("| {} |", results.columns.join(" | "));
    let separator = results
        .columns
        .iter()
        .map(|_| "---")
        .collect::<Vec<_>>()
        .join("|");
    println!("|{}|", separator);

    // Print rows
    for row in &results.rows {
        let values: Vec<String> = results
            .columns
            .iter()
            .map(|col| {
                let empty = String::new();
                let value = row.get(col).unwrap_or(&empty);
                if col == "title" {
                    let path = row.get("path").unwrap_or(&empty);
                    format!("[{}]({})", value, path)
                } else if col != "path" {
                    value.to_string()
                } else {
                    empty
                }
            })
            .filter(|s| !s.is_empty())
            .collect();
        println!("| {} |", values.join(" | "));
    }
}
