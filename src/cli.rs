use clap::{arg, Parser, Subcommand};
use std::sync::atomic::AtomicBool;
use std::sync::Arc;
use tracing::{debug, error};

#[derive(Parser, Debug, Clone)]
#[clap(author, version, about)]
pub struct Cli {
    #[clap(subcommand)]
    pub command: Command,

    /// Enable debug logging
    #[arg(long, global = true, help = "Enable verbose debug output")]
    pub debug: bool,
}

#[derive(Debug, Clone, Subcommand)]
pub enum Command {
    /// List all notes
    List {
        /// Filter notes from this date (YYYY-MM-DD)
        #[arg(long)]
        from: Option<String>,
        /// Filter notes until this date (YYYY-MM-DD)
        #[arg(long)]
        to: Option<String>,
        /// Filter notes by frontmatter key-value pairs
        #[arg(short, long, value_parser = parse_key_val, help = "Filter by key:value (e.g. tags:rust)")]
        filter: Vec<(String, String)>,
    },
    /// Add a new note
    Add {
        /// Title of the note
        #[arg(short, long)]
        title: String,
        /// Content of the note (optional, will open editor if not provided)
        #[arg(short, long)]
        body: Option<String>,
        /// Frontmatter key-value pairs
        #[arg(short, long, value_parser = parse_key_val, help = "Add frontmatter key:value (e.g. tags:rust)")]
        frontmatter: Vec<(String, String)>,
    },
    /// Delete a note
    Delete {
        /// Title of the note to delete
        #[arg(short, long)]
        title: String,
    },
    #[clap(name = "observers")]
    ListObservers,
    /// Sync all notes with observers
    #[clap(name = "sync")]
    Sync,
    /// Query notes using natural language or SQL
    Query {
        /// Query string (natural language or SQL)
        #[arg(short, long)]
        query: String,
        /// Force SQL mode (skip natural language processing)
        #[arg(short, long)]
        sql: bool,
    },
    Watch,
}

/// Helper function to parse key-value pairs.
pub fn parse_key_val(s: &str) -> Result<(String, String), String> {
    debug!("Parsing key-value pair: {}", s);

    let parts: Vec<&str> = s.splitn(2, ':').collect();
    if parts.len() == 2 {
        let key = parts[0].trim();
        let value = parts[1].trim();

        if key.is_empty() {
            error!("Empty key in key-value pair: {}", s);
            return Err("Key cannot be empty".to_string());
        }

        if value.is_empty() {
            error!("Empty value in key-value pair: {}", s);
            return Err("Value cannot be empty".to_string());
        }

        debug!("Successfully parsed key-value pair: {}:{}", key, value);
        Ok((key.to_string(), value.to_string()))
    } else {
        error!("Invalid key-value format: {}", s);
        Err(format!(
            "'{}' is not a valid key:value pair (use 'key:value' format)",
            s
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_key_val() {
        // Valid cases
        assert_eq!(
            parse_key_val("tags:rust").unwrap(),
            ("tags".to_string(), "rust".to_string())
        );
        assert_eq!(
            parse_key_val("title:My Note").unwrap(),
            ("title".to_string(), "My Note".to_string())
        );

        // Invalid cases
        assert!(parse_key_val("invalid").is_err());
        assert!(parse_key_val(":empty_key").is_err());
        assert!(parse_key_val("empty_value:").is_err());
        assert!(parse_key_val(":").is_err());
    }
}
