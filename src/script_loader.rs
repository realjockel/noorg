use crate::event::{NoteEvent, NoteObserver, ObserverResult};
use crate::observers;
use crate::settings::Settings;
use mlua::Lua;
use pyo3::prelude::*;
use pyo3::types::{PyDict, PyModule};
use std::any::Any;
use std::collections::HashMap;
use std::fs;
use std::future::Future;
use std::io;
use std::path::Path;
use std::pin::Pin;
use std::sync::Arc;
use tokio::task;
use tracing::{debug, error, info, trace, warn};

#[derive(Clone)]
pub struct LuaObserver {
    lua: Lua,
    name: String,
}

impl LuaObserver {
    pub fn new(script_path: &Path) -> io::Result<Self> {
        debug!("Creating new Lua observer from: {}", script_path.display());
        let lua = Lua::new();

        // First, register the json module
        lua.load(
            r#"
            json = {
                encode = function(v)
                    return require("json").encode(v)
                end,
                decode = function(v)
                    return require("json").decode(v)
                end
            }
        "#,
        )
        .exec()
        .map_err(|e| {
            error!("Failed to register json module: {}", e);
            io::Error::new(io::ErrorKind::Other, e.to_string())
        })?;

        // Register logging module directly
        let logging_utils = r#"
local M = {}

local function format_log(level, message, ...)
    local formatted = string.format(message, ...)
    io.write(string.format("  %s  %s  %s\n",
        os.date("%Y-%m-%dT%H:%M:%S.000000Z"),
        string.format("%-5s", level),
        formatted
    ))
end

function M.error(message, ...)
    format_log("ERROR", message, ...)
end

function M.warn(message, ...)
    format_log("WARN", message, ...)
end

function M.info(message, ...)
    format_log("INFO", message, ...)
end

function M.debug(message, ...)
    format_log("DEBUG", message, ...)
end

function M.trace(message, ...)
    format_log("TRACE", message, ...)
end

return M
"#;

        let package: mlua::Table = lua.globals().get("package").map_err(|e| {
            error!("Failed to get package table: {}", e);
            io::Error::new(io::ErrorKind::Other, e.to_string())
        })?;

        let loaded: mlua::Table = package.get("loaded").map_err(|e| {
            error!("Failed to get loaded table: {}", e);
            io::Error::new(io::ErrorKind::Other, e.to_string())
        })?;

        // Load and execute the logging module
        let logging_module = lua
            .load(logging_utils)
            .set_name("logging_utils")
            .eval::<mlua::Table>()
            .map_err(|e| {
                error!("Failed to load logging module: {}", e);
                io::Error::new(io::ErrorKind::Other, e.to_string())
            })?;

        // Register it in package.loaded
        loaded.set("logging_utils", logging_module).map_err(|e| {
            error!("Failed to register logging module: {}", e);
            io::Error::new(io::ErrorKind::Other, e.to_string())
        })?;

        // Load the main script
        let script_content = fs::read_to_string(script_path).map_err(|e| {
            error!("Failed to read Lua script: {}", e);
            e
        })?;

        // Load the script
        lua.load(&script_content)
            .set_name(script_path.to_str().unwrap_or("script"))
            .exec()
            .map_err(|e| {
                error!("Failed to execute Lua script: {}", e);
                io::Error::new(io::ErrorKind::Other, e.to_string())
            })?;

        let name = script_path
            .file_stem()
            .unwrap_or_default()
            .to_string_lossy()
            .to_string();

        info!("✨ Loaded Lua observer: {}", name);
        Ok(Self { lua, name })
    }
}

impl NoteObserver for LuaObserver {
    fn on_event_boxed(
        &self,
        event: NoteEvent,
    ) -> Pin<Box<dyn Future<Output = io::Result<Option<ObserverResult>>> + Send + '_>> {
        let lua = self.lua.clone();
        let observer_name = self.name.clone();

        Box::pin(async move {
            debug!("Processing event in Lua observer: {}", observer_name);
            task::spawn_blocking(move || {
                let globals = lua.globals();
                let on_event: mlua::Function = globals.get("on_event").map_err(|e| {
                    error!("Failed to get on_event function: {}", e);
                    io::Error::new(io::ErrorKind::Other, e.to_string())
                })?;

                let event_str = serde_json::to_string(&event)?;
                trace!("Sending event to Lua: {}", event_str);

                let result: mlua::Value = on_event.call(event_str).map_err(|e| {
                    error!("Failed to execute Lua on_event: {}", e);
                    io::Error::new(io::ErrorKind::Other, e.to_string())
                })?;

                match result {
                    mlua::Value::Nil => {
                        debug!("Lua observer returned no changes");
                        Ok(None)
                    }
                    mlua::Value::String(s) => {
                        debug!("Processing Lua observer result");
                        let result: serde_json::Value = serde_json::from_str(&s.to_string_lossy())?;

                        let metadata = result.get("metadata").and_then(|m| {
                            serde_json::from_value::<HashMap<String, String>>(m.clone()).ok()
                        });
                        let content = result
                            .get("content")
                            .and_then(|c| c.as_str())
                            .map(|s| s.to_string());

                        trace!(
                            "Lua observer returned - metadata: {:?}, content modified: {}",
                            metadata,
                            content.is_some()
                        );
                        Ok(Some(ObserverResult { metadata, content }))
                    }
                    _ => {
                        error!("Invalid return type from Lua script");
                        Err(io::Error::new(
                            io::ErrorKind::Other,
                            "Invalid return type from Lua script",
                        ))
                    }
                }
            })
            .await
            .map_err(|e| {
                error!("Task execution failed: {}", e);
                io::Error::new(io::ErrorKind::Other, e.to_string())
            })?
        })
    }

    fn name(&self) -> String {
        self.name.clone()
    }

    fn as_any(&self) -> &dyn Any {
        self
    }
}

pub struct PythonObserver {
    code: String,
    name: String,
}

impl PythonObserver {
    pub fn new(script_path: &Path) -> io::Result<Self> {
        debug!(
            "Creating new Python observer from: {}",
            script_path.display()
        );

        Python::with_gil(|py| {
            // Get the scripts directory
            let scripts_dir = script_path
                .parent()
                .and_then(|p| p.parent())
                .ok_or_else(|| {
                    io::Error::new(io::ErrorKind::NotFound, "Could not find scripts directory")
                })?;

            // Add both the scripts dir and the python subdir to Python path
            let sys_path = py.import("sys")?.getattr("path")?;
            sys_path.call_method1("insert", (0, scripts_dir.join("python")))?;
            sys_path.call_method1("insert", (0, scripts_dir))?;

            // Create the logging module
            let logging_utils = r#"
from typing import Any
import json
import sys
from datetime import datetime, timezone

def _format_log(level: str, message: str) -> None:
    timestamp = datetime.now(timezone.utc).strftime("%Y-%m-%dT%H:%M:%S.%fZ")
    print(f"  {timestamp}  {level:<5}  {message}")

def log_error(message: str, *args: Any) -> None:
    _format_log("ERROR", message.format(*args))

def log_warn(message: str, *args: Any) -> None:
    _format_log("WARN", message.format(*args))

def log_info(message: str, *args: Any) -> None:
    _format_log("INFO", message.format(*args))

def log_debug(message: str, *args: Any) -> None:
    _format_log("DEBUG", message.format(*args))

def log_trace(message: str, *args: Any) -> None:
    _format_log("TRACE", message.format(*args))
"#;

            // Create logging_utils module
            PyModule::from_code(py, logging_utils, "logging_utils.py", "logging_utils")?;

            // Now load the actual script
            let code = fs::read_to_string(script_path)?;
            let name = script_path
                .file_stem()
                .unwrap_or_default()
                .to_string_lossy()
                .to_string();

            info!("✨ Loaded Python observer: {}", name);
            Ok(Self { code, name })
        })
        .map_err(|e: PyErr| {
            error!("Failed to initialize Python observer: {}", e);
            io::Error::new(io::ErrorKind::Other, e.to_string())
        })
    }
}

impl NoteObserver for PythonObserver {
    fn on_event_boxed(
        &self,
        event: NoteEvent,
    ) -> Pin<Box<dyn Future<Output = io::Result<Option<ObserverResult>>> + Send + '_>> {
        let code = self.code.clone();
        let observer_name = self.name.clone();

        Box::pin(async move {
            debug!("Processing event in Python observer: {}", observer_name);
            task::spawn_blocking(move || {
                Python::with_gil(|py| {
                    let mut event_json = serde_json::to_value(&event)?;
                    if let serde_json::Value::Object(ref mut map) = event_json {
                        let event_type = match map {
                            m if m.contains_key("Created") => m.get_mut("Created"),
                            m if m.contains_key("Updated") => m.get_mut("Updated"),
                            m if m.contains_key("Synced") => m.get_mut("Synced"),
                            _ => None,
                        };

                        if let Some(serde_json::Value::Object(ref mut event_map)) = event_type {
                            event_map.insert(
                                "data_dir".to_string(),
                                serde_json::Value::String(
                                    Settings::get_data_dir().to_string_lossy().to_string(),
                                ),
                            );
                        }
                    }
                    let event_json = serde_json::to_string(&event_json)?;
                    trace!("Sending event to Python: {}", event_json);

                    let locals = PyDict::new_bound(py);
                    locals
                        .set_item("event_json", event_json.clone())
                        .map_err(|e| {
                            error!("Failed to set event_json in Python context: {}", e);
                            io::Error::new(io::ErrorKind::Other, e.to_string())
                        })?;

                    let code = PyModule::from_code_bound(py, &code, "", "").map_err(|e| {
                        error!("Failed to create Python module: {}", e);
                        io::Error::new(io::ErrorKind::Other, e.to_string())
                    })?;

                    if let Ok(func) = code.getattr("process_event") {
                        let result = func.call1((event_json,)).map_err(|e| {
                            error!("Failed to execute Python process_event: {}", e);
                            io::Error::new(io::ErrorKind::Other, e.to_string())
                        })?;

                        if let Ok(result_str) = result.extract::<String>() {
                            if let Ok(result) = serde_json::from_str(&result_str) {
                                let result: serde_json::Value = result;

                                let metadata = result.get("metadata").and_then(|m| {
                                    serde_json::from_value::<HashMap<String, String>>(m.clone())
                                        .ok()
                                });

                                // Only get content if it exists, don't fall back to original
                                let content = result
                                    .get("content")
                                    .and_then(|c| c.as_str())
                                    .map(|s| s.to_string());

                                return Ok(Some(ObserverResult { metadata, content }));
                            }
                        }
                    }

                    debug!("Python observer returned no changes");
                    Ok(None)
                })
            })
            .await
            .map_err(|e| {
                error!("Task execution failed: {}", e);
                io::Error::new(io::ErrorKind::Other, e.to_string())
            })?
        })
    }

    fn name(&self) -> String {
        self.name.clone()
    }

    fn as_any(&self) -> &dyn Any {
        self
    }
}

pub struct ScriptLoader {
    scripts_dir: String,
    settings: Settings,
}

impl ScriptLoader {
    pub fn new(scripts_dir: String, settings: Settings) -> Self {
        debug!("Creating new ScriptLoader with directory: {}", scripts_dir);
        Self {
            scripts_dir,
            settings,
        }
    }

    pub fn load_observers(
        &self,
        enabled_observers: &[String],
    ) -> io::Result<Vec<Box<dyn NoteObserver>>> {
        debug!("Loading observers. Enabled: {:?}", enabled_observers);
        let mut observers: Vec<Box<dyn NoteObserver>> = Vec::new();

        // Add enabled Rust observers
        for observer_name in enabled_observers {
            debug!("Loading Rust observer: {}", observer_name);
            if let Some(observer) =
                observers::create_observer(observer_name, Arc::new(self.settings.clone()))
            {
                info!("✨ Loaded Rust observer: {}", observer_name);
                observers.push(observer);
            } else {
                warn!("No Rust observer found for: {}", observer_name);
            }
        }

        // Load Lua scripts
        let lua_dir = Path::new(&self.scripts_dir).join("lua");
        if lua_dir.exists() {
            debug!("Loading Lua scripts from: {}", lua_dir.display());
            for entry in fs::read_dir(lua_dir)? {
                let path = entry?.path();
                if path.extension().map_or(false, |ext| ext == "lua") {
                    debug!("Loading Lua script: {}", path.display());
                    observers.push(Box::new(LuaObserver::new(&path)?));
                }
            }
        } else {
            debug!("No Lua scripts directory found");
        }

        // Load Python scripts
        let py_dir = Path::new(&self.scripts_dir).join("python");
        if py_dir.exists() {
            debug!("Loading Python scripts from: {}", py_dir.display());
            for entry in fs::read_dir(py_dir)? {
                let path = entry?.path();
                if path.extension().map_or(false, |ext| ext == "py") {
                    debug!("Loading Python script: {}", path.display());
                    observers.push(Box::new(PythonObserver::new(&path)?));
                }
            }
        } else {
            debug!("No Python scripts directory found");
        }

        info!("🔌 Loaded {} observers total", observers.len());
        Ok(observers)
    }
}
