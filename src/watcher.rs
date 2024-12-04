use notify::{Config, Event, RecommendedWatcher, RecursiveMode, Watcher};
use percent_encoding::percent_decode_str;
use std::collections::HashSet;
use std::io;
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use tokio::runtime::Handle;
use tokio::sync::mpsc;
use tracing::{debug, error, info, trace, warn};
use std::fs;

use crate::note::{Note, NoteManager};
use crate::observer_registry::ObserverRegistry;
use crate::settings::Settings;

fn convert_notify_error(e: notify::Error) -> io::Error {
    error!("Notify error: {}", e);
    io::Error::new(io::ErrorKind::Other, e)
}

pub async fn watch_directory(
    settings: Settings,
    observer_registry: Arc<ObserverRegistry>,
    stop_signal: Arc<AtomicBool>,
) -> io::Result<()> {
    debug!("Initializing directory watcher");
    
    // Test write permissions
    let note_dir = Path::new(&settings.note_dir);
    if !note_dir.exists() {
        debug!("Creating notes directory: {}", settings.note_dir);
        fs::create_dir_all(note_dir).map_err(|e| {
            error!("Failed to create notes directory. This might be a permissions issue: {}", e);
            io::Error::new(
                io::ErrorKind::PermissionDenied,
                format!("Cannot create or access notes directory: {}. Please check app permissions in System Settings > Privacy & Security > Files and Folders.", e)
            )
        })?;
    }

    // Test write permissions with a temporary file
    let test_file = note_dir.join(".permissions_test");
    match fs::write(&test_file, "") {
        Ok(_) => {
            fs::remove_file(test_file).ok(); // Clean up test file
            debug!("Successfully verified write permissions");
        }
        Err(e) => {
            error!("Failed to write test file. This might be a permissions issue: {}", e);
            return Err(io::Error::new(
                io::ErrorKind::PermissionDenied,
                "Cannot write to notes directory. Please check app permissions in System Settings > Privacy & Security > Files and Folders."
            ));
        }
    }

    let (tx, mut rx) = mpsc::channel(100);
    let note_manager = NoteManager::new(settings.clone(), observer_registry.clone()).await?;

    // Track recently processed files to avoid loops
    let processing_files = Arc::new(Mutex::new(HashSet::new()));
    let debounce_duration = Duration::from_millis(100);
    debug!("Using debounce duration: {:?}", debounce_duration);

    let runtime_handle = Handle::current();
    let processing_files_clone = processing_files.clone();

    let mut watcher = RecommendedWatcher::new(
        move |res: Result<Event, notify::Error>| {
            let tx = tx.clone();
            let _processing_files = processing_files_clone.clone();

            if let Ok(event) = res {
                trace!("Received file system event: {:?}", event);
                let handle = runtime_handle.clone();
                std::thread::spawn(move || {
                    handle.block_on(async {
                        if let Err(e) = tx.send(event).await {
                            error!("Failed to send event: {}", e);
                        }
                    });
                });
            }
        },
        Config::default(),
    )
    .map_err(convert_notify_error)?;

    watcher
        .watch(Path::new(&settings.note_dir), RecursiveMode::Recursive)
        .map_err(convert_notify_error)?;

    info!("🔍 Watching directory: {}", settings.note_dir);

    let _watcher = watcher;
    let mut last_events = std::collections::HashMap::new();

    while let Some(event) = rx.recv().await {
        // Check if we should stop
        if stop_signal.load(Ordering::SeqCst) {
            info!("Stop signal received, shutting down watcher");
            break;
        }

        match event.kind {
            notify::EventKind::Modify(_) | notify::EventKind::Create(_) => {
                for path in event.paths {
                    if path.extension().and_then(|s| s.to_str()) == Some(&settings.file_type) {
                        if let Some(title) = path.file_stem().and_then(|s| s.to_str()) {
                            // Decode any percent-encoded characters in the title
                            let decoded_title = percent_decode_str(title)
                                .decode_utf8()
                                .unwrap_or_else(|_| title.into())
                                .into_owned();

                            let path_str = path.to_string_lossy().to_string();
                            debug!("Processing change for note: {}", decoded_title);

                            // Check if we recently processed this file
                            let mut processing = processing_files.lock().unwrap();
                            if !processing.contains(&path_str) {
                                // Check if we need to debounce
                                let now = Instant::now();
                                if let Some(last_time) = last_events.get(&path_str) {
                                    if now.duration_since(*last_time) < debounce_duration {
                                        trace!("Debouncing change for: {}", decoded_title);
                                        continue;
                                    }
                                }

                                // Mark file as being processed
                                processing.insert(path_str.clone());
                                last_events.insert(path_str.clone(), now);

                                info!("📝 Change detected in note: {}", decoded_title);

                                // Read the file content first to check if it really changed
                                match Note::from_file(&path) {
                                    Ok(Some((content, _frontmatter))) => {
                                        debug!("Successfully read note content");
                                        // Only sync if content changed
                                        if note_manager
                                            .should_process_note(&decoded_title, &content)
                                            .await
                                        {
                                            debug!("Content changed, syncing note");
                                            if let Err(e) = note_manager
                                                .sync_single_note(&decoded_title, true)
                                                .await
                                            {
                                                error!(
                                                    "Failed to sync note '{}': {}",
                                                    decoded_title, e
                                                );
                                            }
                                        } else {
                                            info!(
                                                "⏭️ Content unchanged for '{}', skipping sync",
                                                decoded_title
                                            );
                                        }
                                    }
                                    Ok(None) => warn!("Could not parse note: {}", decoded_title),
                                    Err(e) => {
                                        error!("Error reading note '{}': {}", decoded_title, e)
                                    }
                                }

                                // Remove from processing after a delay
                                let processing = processing_files.clone();
                                let path_str = path_str.clone();
                                tokio::spawn(async move {
                                    trace!("Starting debounce timer for: {}", path_str);
                                    tokio::time::sleep(debounce_duration).await;
                                    processing.lock().unwrap().remove(&path_str);
                                    trace!("Removed {} from processing list", path_str);
                                });
                            } else {
                                trace!("Note already being processed: {}", decoded_title);
                            }
                        }
                    }
                }
            }
            _ => {
                trace!("Ignoring non-modify/create event: {:?}", event.kind);
            }
        }
    }

    info!("Watcher stopped");
    Ok(())
}
