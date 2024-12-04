use std::collections::HashMap;
use tracing::{debug, trace};

pub fn merge_metadata(existing: &mut HashMap<String, String>, new: HashMap<String, String>) {
    debug!("Merging metadata with {} new fields", new.len());
    trace!("Existing metadata: {:?}", existing);
    trace!("New metadata: {:?}", new);

    for (key, value) in new {
        match key.as_str() {
            "tags" => {
                debug!("Merging tags field");
                let existing_tags: Vec<String> = existing
                    .get("tags")
                    .map(|t| {
                        trace!("Existing tags: {}", t);
                        t.split(',').map(|s| s.trim().to_string()).collect()
                    })
                    .unwrap_or_default();

                let new_tags: Vec<String> =
                    value.split(',').map(|s| s.trim().to_string()).collect();
                trace!("New tags: {:?}", new_tags);

                let mut combined_tags: Vec<String> =
                    existing_tags.into_iter().chain(new_tags).collect();

                combined_tags.sort();
                combined_tags.dedup();
                trace!("Combined and deduplicated tags: {:?}", combined_tags);

                existing.insert(key, combined_tags.join(", "));
            }
            "topics" => {
                debug!("Merging topics field");
                let existing_items: Vec<String> = existing
                    .get("topics")
                    .map(|t| {
                        trace!("Existing topics: {}", t);
                        t.split(',').map(|s| s.trim().to_string()).collect()
                    })
                    .unwrap_or_default();

                let new_items: Vec<String> =
                    value.split(',').map(|s| s.trim().to_string()).collect();
                trace!("New topics: {:?}", new_items);

                let mut combined: Vec<String> =
                    existing_items.into_iter().chain(new_items).collect();

                combined.sort();
                combined.dedup();
                trace!("Combined and deduplicated topics: {:?}", combined);

                existing.insert(key, combined.join(", "));
            }
            "created_at" => {
                debug!("Processing created_at field");
                if !existing.contains_key(&key) {
                    trace!("Setting initial created_at: {}", value);
                    existing.insert(key, value);
                } else {
                    trace!("Keeping existing created_at timestamp");
                }
            }
            "updated_at" => {
                debug!("Updating updated_at field to: {}", value);
                existing.insert(key, value);
            }
            "timestamp" => {
                debug!("Skipping redundant timestamp field");
            }
            _ => {
                debug!("Setting field '{}' to '{}'", key, value);
                existing.insert(key, value);
            }
        }
    }

    debug!("Metadata merge completed");
    trace!("Final metadata state: {:?}", existing);
}
