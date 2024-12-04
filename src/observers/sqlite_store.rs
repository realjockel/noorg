use crate::event::*;
use crate::settings::Settings;
use kalosm::language::*;
use rusqlite::{Connection, Result as SqlResult};
use std::any::Any;
use std::collections::HashMap;
use std::future::Future;
use std::io;
use std::path::Path;
use std::pin::Pin;
use std::sync::Arc;
use tokio::sync::Mutex;
use tracing::{debug, error, info};

pub struct SqliteObserver {
    conn: Arc<Mutex<Connection>>,
    model: Llama,
    settings: Arc<Settings>,
}
#[derive(Debug)]
pub struct NoteResult {
    pub id: i64,
    pub title: String,
    pub filepath: String, // Changed from Option<String> to String since it's required
}

#[derive(Debug)]
pub struct QueryResult {
    pub columns: Vec<String>,
    pub rows: Vec<HashMap<String, String>>,
}

#[derive(Debug)]
struct SqlBlock {
    sql: String,
    range: (usize, usize),
}

impl SqliteObserver {
    pub fn new(settings: Arc<Settings>) -> io::Result<Self> {
        let data_dir = Settings::get_data_dir();
        let sqlite_dir = data_dir.join("sqlite");
        let db_path = sqlite_dir.join("frontmatter.db");

        debug!("Creating SQLite directory at {:?}", sqlite_dir);
        std::fs::create_dir_all(&sqlite_dir)?;

        debug!("Initializing SQLite database at {:?}", db_path);
        let conn = Connection::open(&db_path).map_err(|e| {
            error!("Failed to open SQLite database: {}", e);
            io::Error::new(io::ErrorKind::Other, e)
        })?;

        debug!("Creating database schema");
        conn.execute(
            "CREATE TABLE IF NOT EXISTS notes (
                id INTEGER PRIMARY KEY,
                title TEXT UNIQUE NOT NULL,
                path TEXT NOT NULL
            )",
            [],
        )
        .map_err(|e| {
            error!("Failed to create notes table: {}", e);
            io::Error::new(io::ErrorKind::Other, e)
        })?;

        conn.execute(
            "CREATE TABLE IF NOT EXISTS frontmatter (
                file_id INTEGER,
                key TEXT,
                value TEXT,
                PRIMARY KEY (file_id, key),
                FOREIGN KEY (file_id) REFERENCES notes(id)
            )",
            [],
        )
        .map_err(|e| {
            error!("Failed to create frontmatter table: {}", e);
            io::Error::new(io::ErrorKind::Other, e)
        })?;

        debug!("Initializing LLM model");
        let model = tokio::task::block_in_place(|| {
            tokio::runtime::Handle::current().block_on(async {
                Llama::phi_3()
                    .await
                    .map_err(|e| io::Error::new(io::ErrorKind::Other, e))
            })
        })?;

        info!("✨ SQLite observer initialized successfully");
        Ok(Self {
            conn: Arc::new(Mutex::new(conn)),
            model,
            settings,
        })
    }

    async fn store_frontmatter(
        &self,
        title: &str,
        frontmatter: &HashMap<String, String>,
        file_path: String,
    ) -> SqlResult<()> {
        debug!("Storing frontmatter for note: {}", title);
        let conn = self.conn.lock().await;

        conn.execute(
            "INSERT OR REPLACE INTO notes (title, path) VALUES (?1, ?2)",
            [title, &file_path],
        )?;

        let file_id: i64 =
            conn.query_row("SELECT id FROM notes WHERE title = ?1", [title], |row| {
                row.get(0)
            })?;

        debug!("Updating frontmatter for note ID: {}", file_id);
        conn.execute("DELETE FROM frontmatter WHERE file_id = ?1", [file_id])?;

        let mut stmt =
            conn.prepare("INSERT INTO frontmatter (file_id, key, value) VALUES (?1, ?2, ?3)")?;

        for (key, value) in frontmatter {
            stmt.execute(rusqlite::params![file_id, key.as_str(), value.as_str()])?;
        }

        debug!("Successfully stored frontmatter for '{}'", title);
        Ok(())
    }

    pub async fn natural_query(&self, query: &str) -> io::Result<QueryResult> {
        debug!("Processing natural language query: {}", query);

        let task = Task::new(
            "Convert natural language to SQLite queries.
             You are a SQL generator. Return only the raw SQL query, no explanations or formatting. AGAIN FOR REAL.
             Do not add any explanations.
             
             Database tables:
             notes (id INTEGER PRIMARY KEY, title TEXT, path TEXT)
             frontmatter (file_id INTEGER, key TEXT, value TEXT)
             
             Rules:
             1. Use proper table aliases (n for notes, f for frontmatter)
             2. Join using: ON n.id = f.file_id
             3. Always use proper spacing around operators
             4. Use single quotes for string values
             5. Always include n.id, n.title, n.path in SELECT clause
             6. Return only the raw SQL query, nothing else
             
             Examples:
             Q: show notes tagged with rust
             A: SELECT n.id, n.title, n.path FROM notes n JOIN frontmatter f ON n.id = f.file_id WHERE f.key = 'tags' AND f.value LIKE '%rust%'
             
             Q: find all notes
             A: SELECT n.id, n.title, n.path FROM notes n"
        );

        let mut response = String::new();
        let mut stream = task.run(query, &self.model);
        while let Some(token) = stream.next().await {
            response.push_str(&token);
        }

        let clean_query = response
            .lines()
            .find(|line| line.trim().to_uppercase().starts_with("SELECT"))
            .unwrap_or_default()
            .replace("nodes", "notes")
            .replace(" AS ", " ")
            .replace("`", "")
            .split(';')
            .next()
            .unwrap_or_default()
            .trim()
            .to_string();

        if clean_query.is_empty() {
            error!("No valid SQL query generated from natural language input");
            return Err(io::Error::new(
                io::ErrorKind::Other,
                "No valid SQL query found in response",
            ));
        }

        debug!("Generated SQL query: {}", clean_query);
        self.query(&clean_query).await
    }

    pub async fn query(&self, sql: &str) -> io::Result<QueryResult> {
        debug!("Executing SQL query: {}", sql);
        let conn = self.conn.lock().await;
        let mut stmt = conn.prepare(sql).map_err(|e| {
            error!("Failed to prepare SQL statement: {}", e);
            io::Error::new(io::ErrorKind::Other, e)
        })?;

        let columns: Vec<String> = stmt
            .column_names()
            .into_iter()
            .map(|c| c.to_string())
            .collect();

        let rows = stmt
            .query_map([], |row| {
                let mut map = HashMap::new();
                for (i, column) in columns.iter().enumerate() {
                    let value: String = row.get(i).unwrap_or_else(|_| "".to_string());
                    map.insert(column.clone(), value);
                }
                Ok(map)
            })
            .map_err(|e| {
                error!("Failed to execute query: {}", e);
                io::Error::new(io::ErrorKind::Other, e)
            })?
            .collect::<Result<Vec<_>, _>>()
            .map_err(|e| io::Error::new(io::ErrorKind::Other, e))?;

        debug!("Query returned {} rows", rows.len());
        Ok(QueryResult { columns, rows })
    }

    pub async fn print_all_frontmatter(&self) -> io::Result<()> {
        debug!("Retrieving all frontmatter data");
        let conn = self.conn.lock().await;
        let sql = "
            SELECT n.title, f.key, f.value 
            FROM notes n 
            JOIN frontmatter f ON n.id = f.file_id 
            ORDER BY n.title, f.key";

        let mut stmt = conn.prepare(sql).unwrap();
        let rows = stmt
            .query_map([], |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                ))
            })
            .unwrap();

        info!("📊 Current Database Contents:");
        let mut current_title = String::new();
        for row in rows {
            if let Ok((title, key, value)) = row {
                if title != current_title {
                    info!("📝 {}", title);
                    current_title = title;
                }
                debug!("  {} = {}", key, value);
            }
        }

        Ok(())
    }

    pub async fn process_sql_blocks(&self, content: &str) -> io::Result<String> {
        let sql_blocks = self.extract_sql_blocks(content);

        if sql_blocks.is_empty() {
            debug!("No SQL blocks found in content");
            return Ok(content.to_string());
        }

        let mut new_content = content.to_string();

        debug!("Processing {} SQL blocks", sql_blocks.len());

        // Process blocks in reverse to maintain correct positions
        for block in sql_blocks.into_iter().rev() {
            let results = self.query(&block.sql).await?;

            // Build the replacement content
            let mut output = String::new();
            output.push_str("```sql\n");
            output.push_str(&block.sql);
            output.push_str("\n```\n");
            output.push_str("<!-- BEGIN SQL -->\n");

            // Add table header
            output.push_str("| ");
            output.push_str(&results.columns.join(" | "));
            output.push_str(" |\n|");
            output.push_str(&vec!["---"; results.columns.len()].join("|"));
            output.push_str("|\n");

            let default_string = String::new();
            // Add table rows
            for row in &results.rows {
                output.push_str("| ");
                let values: Vec<String> = results
                    .columns
                    .iter()
                    .map(|col| {
                        let val = row.get(col.as_str()).unwrap_or(&default_string);
                        if col == "path" {
                            // Extract title from the full path
                            let path = Path::new(&val);
                            let title = path.file_stem().and_then(|s| s.to_str()).unwrap_or("");

                            // Create relative path for the link
                            let relative_path = format!("./{}.{}", title, self.settings.file_type);

                            // Format link using relative path
                            format!("[{}]({})", title, relative_path)
                        } else {
                            val.trim().to_string()
                        }
                    })
                    .collect();
                output.push_str(&values.join(" | "));
                output.push_str(" |\n");
            }

            output.push_str("<!-- END SQL -->\n\n");

            // Check if the block is within the TOC section
            let is_in_toc = content[..block.range.0].contains("## Contents");

            if !is_in_toc {
                // Replace the old content with the new only if not in TOC
                new_content.replace_range(block.range.0..block.range.1, &output);
            }
        }

        Ok(new_content)
    }

    fn extract_sql_blocks(&self, content: &str) -> Vec<SqlBlock> {
        let mut blocks = Vec::new();
        let lines: Vec<&str> = content.lines().collect();
        let mut i = 0;

        while i < lines.len() {
            if lines[i].trim().starts_with("```sql") && !content[..i].contains("## Contents") {
                let start_line = i;
                let mut sql = String::new();

                // Find the exact byte position of the start of the SQL block
                let start_pos = content
                    .chars()
                    .take(
                        content
                            .lines()
                            .take(start_line)
                            .map(|l| l.chars().count() + 1) // +1 for newline
                            .sum(),
                    )
                    .map(|c| c.len_utf8())
                    .sum::<usize>();

                i += 1; // Skip the opening ```sql line

                // Collect SQL until the closing backticks
                while i < lines.len() && !lines[i].trim().starts_with("```") {
                    sql.push_str(lines[i]);
                    sql.push('\n');
                    i += 1;
                }

                // Skip the closing ``` line
                if i < lines.len() {
                    i += 1;
                }

                // Find the end of any existing results
                let mut end_line = i;
                while end_line < lines.len() {
                    let line = lines[end_line].trim();
                    if line.starts_with("```sql") {
                        // Next SQL block starts
                        break;
                    }
                    if line == "<!-- END SQL -->" {
                        // Current block results end
                        end_line += 1; // Include the END SQL marker
                        break;
                    }
                    end_line += 1;
                }

                // Calculate exact end position in bytes
                let end_pos = content
                    .chars()
                    .take(
                        content
                            .lines()
                            .take(end_line)
                            .map(|l| l.chars().count() + 1)
                            .sum(),
                    )
                    .map(|c| c.len_utf8())
                    .sum::<usize>();

                debug!(
                    "SQL block range: {} to {} (content len: {})",
                    start_pos,
                    end_pos,
                    content.len()
                );

                blocks.push(SqlBlock {
                    sql: sql.trim().to_string(),
                    range: (start_pos, end_pos),
                });

                i = end_line;
            } else {
                i += 1;
            }
        }

        blocks
    }
}

impl NoteObserver for SqliteObserver {
    fn on_event_boxed(
        &self,
        event: NoteEvent,
    ) -> Pin<Box<dyn Future<Output = io::Result<Option<ObserverResult>>> + Send + '_>> {
        Box::pin(async move {
            match event {
                NoteEvent::Synced {
                    content,
                    title,
                    file_path,
                    frontmatter,
                    ..
                } => {
                    info!("🔄 Processing note '{}' with SQLite observer", title);

                    match self
                        .store_frontmatter(&title, &frontmatter, file_path)
                        .await
                    {
                        Ok(_) => debug!("Successfully stored frontmatter for '{}'", title),
                        Err(e) => error!("Failed to store frontmatter for '{}': {}", title, e),
                    }

                    if self.extract_sql_blocks(&content).is_empty() {
                        debug!(
                            "No SQL blocks found in note '{}', skipping processing",
                            title
                        );
                        Ok(None)
                    } else {
                        match self.process_sql_blocks(&content).await {
                            Ok(processed_content) => {
                                info!("✨ Successfully processed SQL blocks for '{}'", title);
                                debug!("SQL OBSERVER: Processed content:\n{}", processed_content);
                                Ok(Some(ObserverResult {
                                    metadata: None,
                                    content: Some(processed_content),
                                }))
                            }
                            Err(e) => {
                                error!("Failed to process SQL blocks for '{}': {}", title, e);
                                Err(e)
                            }
                        }
                    }
                }
                _ => Ok(None),
            }
        })
    }

    fn name(&self) -> String {
        "sqlite".to_string()
    }

    fn as_any(&self) -> &dyn Any {
        self
    }

    fn priority(&self) -> i32 {
        100 // Make sure SQLite runs last
    }
}
