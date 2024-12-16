use crate::event::NoteObserver;
use crate::settings::Settings;
use std::collections::HashMap;
use std::sync::Arc;

pub mod sqlite_store;
pub mod tag_index;
pub mod timestamp;
mod toc;
use toc::TocObserver;

// Update the type to include Settings
type ObserverConstructor = fn(Arc<Settings>) -> Box<dyn NoteObserver>;

// Function to create TimestampObserver
fn create_timestamp_observer(_settings: Arc<Settings>) -> Box<dyn NoteObserver> {
    Box::new(timestamp::TimestampObserver)
}

// Function to create SqliteObserver
fn create_sqlite_observer(settings: Arc<Settings>) -> Box<dyn NoteObserver> {
    Box::new(sqlite_store::SqliteObserver::new(settings).unwrap())
}

// Function to create TagIndexObserver
fn create_tag_index_observer(settings: Arc<Settings>) -> Box<dyn NoteObserver> {
    Box::new(tag_index::TagIndexObserver::new(settings).unwrap())
}

// Function to create TocObserver
fn create_toc_observer(_settings: Arc<Settings>) -> Box<dyn NoteObserver> {
    Box::new(TocObserver::new())
}

// Static registry of available Rust observers
lazy_static::lazy_static! {
    static ref OBSERVER_REGISTRY: HashMap<&'static str, ObserverConstructor> = {
        let mut m = HashMap::new();
        m.insert("timestamp", create_timestamp_observer as ObserverConstructor);
        // m.insert("llm_metadata", create_llm_metadata_observer as ObserverConstructor);
        // m.insert("similar_notes", create_similar_notes_observer as ObserverConstructor);
        m.insert("sqlite", create_sqlite_observer as ObserverConstructor);
        m.insert("tag_index", create_tag_index_observer as ObserverConstructor);
        m.insert("toc", create_toc_observer as ObserverConstructor);
        m
    };
}

pub fn get_available_observers() -> Vec<&'static str> {
    OBSERVER_REGISTRY.keys().cloned().collect()
}

// Update create_observer to take settings
pub fn create_observer(name: &str, settings: Arc<Settings>) -> Option<Box<dyn NoteObserver>> {
    OBSERVER_REGISTRY
        .get(name)
        .map(|constructor| constructor(settings))
}

pub fn create_observers(settings: Settings) -> Vec<Box<dyn NoteObserver>> {
    let settings = Arc::new(settings);
    let mut observers: Vec<Box<dyn NoteObserver>> = Vec::new();

    // ... other observers ...

    if settings.enabled_observers.contains(&"toc".to_string()) {
        observers.push(create_toc_observer(settings.clone()));
    }

    observers
}
