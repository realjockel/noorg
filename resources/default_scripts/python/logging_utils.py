from typing import Any
import json
import sys

def log_error(message: str, *args: Any) -> None:
    """Log an error message that will be parsed by Rust."""
    print(json.dumps({"level": "ERROR", "message": message.format(*args)}), file=sys.stderr)

def log_warn(message: str, *args: Any) -> None:
    """Log a warning message that will be parsed by Rust."""
    print(json.dumps({"level": "WARN", "message": message.format(*args)}))

def log_info(message: str, *args: Any) -> None:
    """Log an info message that will be parsed by Rust."""
    print(json.dumps({"level": "INFO", "message": message.format(*args)}))

def log_debug(message: str, *args: Any) -> None:
    """Log a debug message that will be parsed by Rust."""
    print(json.dumps({"level": "DEBUG", "message": message.format(*args)}))

def log_trace(message: str, *args: Any) -> None:
    """Log a trace message that will be parsed by Rust."""
    print(json.dumps({"level": "TRACE", "message": message.format(*args)})) 