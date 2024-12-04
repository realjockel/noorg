use crate::event::{NoteEvent, NoteObserver, ObserverResult};
use pulldown_cmark::{Event as MarkdownEvent, HeadingLevel, Parser, Tag};
use std::any::Any;
use std::collections::HashMap;
use std::future::Future;
use std::io;
use std::pin::Pin;
use tracing::{debug, info};

pub struct TocObserver;

impl TocObserver {
    pub fn new() -> Self {
        debug!("Initializing TOC observer");
        TocObserver
    }

    fn generate_toc(&self, content: &str) -> Option<String> {
        let mut headings = Vec::new();
        let parser = Parser::new(content);
        let mut in_heading = false;
        let mut current_level = 0;
        let mut current_heading = String::new();
        let mut first_h1_seen = false;

        debug!("Collecting headings from content");
        for event in parser {
            match event {
                MarkdownEvent::Start(Tag::Heading(level, ..)) => {
                    in_heading = true;
                    current_level = match level {
                        HeadingLevel::H1 => 1,
                        HeadingLevel::H2 => 2,
                        HeadingLevel::H3 => 3,
                        HeadingLevel::H4 => 4,
                        HeadingLevel::H5 => 5,
                        HeadingLevel::H6 => 6,
                    };
                }
                MarkdownEvent::Text(text) | MarkdownEvent::Code(text) if in_heading => {
                    current_heading.push_str(&text);
                }
                MarkdownEvent::End(Tag::Heading(..)) => {
                    if !current_heading.is_empty() {
                        if current_level == 1 {
                            if !first_h1_seen {
                                first_h1_seen = true;
                                debug!("Skipping first H1 heading: {}", current_heading);
                            } else {
                                let anchor = self.create_anchor(&current_heading);
                                debug!("Adding H1 heading: {} ({})", current_heading, anchor);
                                headings.push((current_level, current_heading.clone(), anchor));
                            }
                        } else {
                            let anchor = self.create_anchor(&current_heading);
                            debug!(
                                "Adding H{} heading: {} ({})",
                                current_level, current_heading, anchor
                            );
                            headings.push((current_level, current_heading.clone(), anchor));
                        }
                        current_heading.clear();
                    }
                    in_heading = false;
                }
                _ => {}
            }
        }

        if headings.is_empty() {
            debug!("No headings found, skipping TOC generation");
            return None;
        }

        debug!("Generating TOC with {} headings", headings.len());
        let mut toc = String::from("## Contents\n\n");

        for (level, heading, anchor) in headings {
            let indent = "  ".repeat(level - 1);
            toc.push_str(&format!("{}* [{}](#{})\n", indent, heading, anchor));
        }

        Some(toc.to_string())
    }

    fn create_anchor(&self, heading: &str) -> String {
        heading
            .to_lowercase()
            .replace(' ', "-")
            .replace(|c: char| !c.is_alphanumeric() && c != '-', "")
    }

    fn insert_toc(&self, content: &str) -> Option<String> {
        let toc = self.generate_toc(content)?;
        debug!("Generated TOC content:\n{}", toc);
        debug!("Processing content for TOC insertion");

        let lines: Vec<&str> = content.lines().collect();
        let mut output = Vec::new();
        let mut in_frontmatter = false;
        let mut frontmatter_end = 0;
        let mut first_heading_found = false;
        let mut first_heading_pos = 0;

        // Find frontmatter end and first heading
        for (i, line) in lines.iter().enumerate() {
            if line.trim() == "---" {
                if !in_frontmatter {
                    in_frontmatter = true;
                    debug!("Found start of frontmatter at line {}", i);
                } else {
                    frontmatter_end = i;
                    debug!("Found end of frontmatter at line {}", i);
                }
            }

            if line.starts_with("# ") && !first_heading_found {
                first_heading_found = true;
                first_heading_pos = i;
                debug!("Found first heading at line {}", i);
            }
        }

        // Copy frontmatter
        for i in 0..=frontmatter_end {
            output.push(lines[i]);
        }
        output.push(""); // Add blank line after frontmatter

        // Copy content up to first heading
        for i in (frontmatter_end + 1)..first_heading_pos {
            output.push(lines[i]);
        }

        // Add first heading
        output.push(lines[first_heading_pos]);
        output.push(""); // Add blank line after heading

        // Add TOC after first heading
        output.extend(toc.lines());
        output.push(""); // Add blank line after TOC

        // Add remaining content, skipping old TOC if present
        let mut skip_old_toc = false;
        for i in (first_heading_pos + 1)..lines.len() {
            let line = lines[i];

            if line.starts_with("## Contents") || line.starts_with("## Table of Contents") {
                skip_old_toc = true;
                continue;
            }

            if skip_old_toc {
                if line.starts_with("## ") {
                    skip_old_toc = false;
                } else {
                    continue;
                }
            }

            output.push(line);
        }

        Some(output.join("\n") + "\n")
    }
}

impl NoteObserver for TocObserver {
    fn on_event_boxed(
        &self,
        event: NoteEvent,
    ) -> Pin<Box<dyn Future<Output = io::Result<Option<ObserverResult>>> + Send + '_>> {
        Box::pin(async move {
            match event {
                NoteEvent::Created { content, title, .. }
                | NoteEvent::Updated { content, title, .. }
                | NoteEvent::Synced { content, title, .. } => {
                    debug!("Processing TOC for note '{}'", title);

                    if content.len() < 50 || !content.contains('#') {
                        debug!(
                            "Skipping TOC generation for '{}' (too short or no headers)",
                            title
                        );
                        return Ok(None);
                    }

                    if let Some(updated_content) = self.insert_toc(&content) {
                        if updated_content != content {
                            info!("📚 Generated table of contents for '{}'", title);
                            debug!("Updated content:\n{}", updated_content);
                            Ok(Some(ObserverResult {
                                content: Some(updated_content),
                                metadata: Some(HashMap::from([(
                                    "toc_generated".to_string(),
                                    "true".to_string(),
                                )])),
                            }))
                        } else {
                            debug!("No changes needed for TOC in '{}'", title);
                            Ok(None)
                        }
                    } else {
                        debug!("No TOC generated for '{}'", title);
                        Ok(None)
                    }
                }
            }
        })
    }

    fn name(&self) -> String {
        "toc".to_string()
    }

    fn as_any(&self) -> &dyn Any {
        self
    }

    fn priority(&self) -> i32 {
        0
    }
}
