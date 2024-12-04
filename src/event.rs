use std::any::Any;
use std::collections::HashMap;
use std::future::Future;
use std::io;
use std::pin::Pin;

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum NoteEvent {
    Created {
        title: String,
        content: String,
        file_path: String,
        frontmatter: HashMap<String, String>,
    },
    Updated {
        title: String,
        content: String,
        file_path: String,
        frontmatter: HashMap<String, String>,
    },
    Synced {
        title: String,
        content: String,
        file_path: String,
        frontmatter: HashMap<String, String>,
    },
}

#[derive(Debug, Clone)]
pub struct ObserverResult {
    pub metadata: Option<HashMap<String, String>>,
    pub content: Option<String>,
}

pub trait NoteObserver: Send + Sync + 'static {
    fn on_event_boxed(
        &self,
        event: NoteEvent,
    ) -> Pin<Box<dyn Future<Output = io::Result<Option<ObserverResult>>> + Send + '_>>;

    fn name(&self) -> String;
    fn priority(&self) -> i32 {
        0
    }
    fn as_any(&self) -> &dyn Any;
}
