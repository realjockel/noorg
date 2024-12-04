use crate::event::{NoteEvent, NoteObserver};
use crate::metadata::merge_metadata;
use std::collections::HashMap;
use std::io;
use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::{debug, error, info, trace};

pub struct ObserverRegistry {
    observers: RwLock<Vec<Arc<Box<dyn NoteObserver>>>>,
}

impl ObserverRegistry {
    pub fn new() -> Self {
        debug!("Creating new ObserverRegistry");
        Self {
            observers: RwLock::new(Vec::new()),
        }
    }

    pub async fn register(&self, observer: Box<dyn NoteObserver>) {
        let name = observer.name();
        debug!("Registering new observer: {}", name);
        let mut observers = self.observers.write().await;
        observers.push(Arc::new(observer));
        info!("✅ Observer '{}' registered successfully", name);
    }

    pub async fn notify(&self, event: NoteEvent) -> io::Result<HashMap<String, String>> {
        debug!("Starting notification process for event");
        trace!("Event details: {:?}", event);

        let observers = self.observers.read().await;
        let mut sorted_observers = observers.iter().collect::<Vec<_>>();

        debug!("Sorting observers by priority");
        sorted_observers.sort_by_key(|o| -o.priority());

        // Move special observers to end
        if let Some(pos) = sorted_observers
            .iter()
            .position(|o| o.name() == "tag_index")
        {
            debug!("Moving tag_index observer to end");
            let tag_index = sorted_observers.remove(pos);
            sorted_observers.push(tag_index);
        }
        if let Some(pos) = sorted_observers.iter().position(|o| o.name() == "sqlite") {
            debug!("Moving sqlite observer to end");
            let sqlite = sorted_observers.remove(pos);
            sorted_observers.push(sqlite);
        }

        let mut combined_metadata = HashMap::new();

        for observer in sorted_observers {
            info!("🔵 Starting observer: {}", observer.name());
            debug!("Processing event for observer: {}", observer.name());
            trace!("Event details for {}: {:?}", observer.name(), event);

            match observer.on_event_boxed(event.clone()).await {
                Ok(Some(result)) => {
                    if let Some(metadata) = result.metadata {
                        debug!("Observer '{}' returned metadata", observer.name());
                        trace!("Metadata from {}: {:?}", observer.name(), metadata);
                        merge_metadata(&mut combined_metadata, metadata);
                    }
                    // Content changes are handled at the note level
                }
                Ok(None) => debug!("Observer '{}' returned no changes", observer.name()),
                Err(e) => error!("Observer '{}' error: {}", observer.name(), e),
            }

            debug!("Completed processing for observer: {}", observer.name());
        }

        debug!("Notification process completed");
        trace!("Final combined metadata: {:?}", combined_metadata);
        Ok(combined_metadata)
    }

    pub async fn get_observers(&self) -> Vec<Arc<Box<dyn NoteObserver>>> {
        debug!("Retrieving observer list");
        let observers = self.observers.read().await;
        let result = observers.iter().cloned().collect();
        trace!("Retrieved {} observers", observers.len());
        result
    }
}
