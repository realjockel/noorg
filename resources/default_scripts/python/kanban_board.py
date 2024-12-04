import json
import re
from datetime import datetime
from pathlib import Path
import os
import sys
from typing import Dict, List, Any, Optional
from logging_utils import log_debug, log_info, log_error, log_warn, log_trace

KANBAN_STATES = ['planned', 'todo', 'doing', 'done']

def get_app_data_dir(event_json: str) -> Path:
    """Get the platform-specific application data directory."""
    try:
        event = json.loads(event_json)
        if isinstance(event, dict):
            event_type = event.get("Created") or event.get("Updated") or event.get("Synced")
            if event_type and "data_dir" in event_type:
                return Path(event_type["data_dir"])
    except Exception as e:
        log_error("Failed to get data dir from event: {}", str(e))
    
    # Fallback to default paths
    if sys.platform == 'darwin':  # macOS
        return Path.home() / 'Library' / 'Application Support' / 'norg' / 'norg'
    elif sys.platform == 'win32':  # Windows
        app_data = os.getenv('APPDATA')
        if app_data:
            return Path(app_data) / 'norg' / 'norg'
    else:  # Linux and others
        return Path.home() / '.config' / 'norg' / 'norg'
    return Path('./data')  # Final fallback

def get_cache_file(event_json: str) -> Path:
    """Get the path to the kanban cache file."""
    cache_dir = get_app_data_dir(event_json) / 'cache'
    cache_dir.mkdir(parents=True, exist_ok=True)
    return cache_dir / 'kanban_cache.json'

def get_kanban_file(note_dir: str) -> Path:
    """Get the path to the kanban board file."""
    return Path(note_dir) / '_kanban.md'

def load_tasks_cache(event_json: str) -> Dict[str, Any]:
    cache_file = get_cache_file(event_json)
    log_debug("Loading tasks cache from: {}", cache_file)
    if cache_file.exists():
        try:
            with open(cache_file, 'r') as f:
                cache = json.load(f)
                log_debug("Loaded {} notes from cache", len(cache))
                return cache
        except Exception as e:
            log_error("Failed to load tasks cache: {}", str(e))
            return {}
    log_debug("No existing cache found")
    return {}

def save_tasks_cache(cache: Dict[str, Any], event_json: str) -> None:
    cache_file = get_cache_file(event_json)
    log_debug("Saving tasks cache with {} notes", len(cache))
    try:
        with open(cache_file, 'w') as f:
            json.dump(cache, f, indent=2)
        log_debug("Cache saved successfully")
    except Exception as e:
        log_error("Failed to save tasks cache: {}", str(e))

def get_context_for_tag(content: str, tag_position: int) -> str:
    """Extract the context in parentheses after the tag."""
    text_after = content[tag_position:tag_position + 200]
    match = re.search(r'#\w+\s*\((.*?)\)', text_after)
    if match:
        context = match.group(1).strip()
        log_trace("Found task context: {}", context)
        return context
    
    log_trace("No context found for tag at position {}", tag_position)
    return ""

def extract_tasks(content: str, note_title: str, note_path: str) -> Dict[str, List[Dict[str, Any]]]:
    """Extract tasks and their context based on kanban tags."""
    log_debug("Extracting tasks from note: {}", note_title)
    tasks = {state: [] for state in KANBAN_STATES}
    
    if note_title == "_kanban" or "📋 Kanban Board" in content:
        log_debug("Skipping kanban board itself")
        return tasks
    
    content = re.sub(r'\n## References\n.*$', '', content, flags=re.DOTALL)
    relative_path = f"./{note_title}.md"
    
    total_tasks = 0
    for state in KANBAN_STATES:
        tag = f'#{state}'
        for match in re.finditer(rf'{tag}\b', content, re.IGNORECASE):
            tag_pos = match.start()
            context = get_context_for_tag(content, tag_pos)
            
            if not context:
                continue
            
            surrounding_text = content[max(0, tag_pos-200):min(len(content), tag_pos+200)]
            paragraph_end = surrounding_text.find('\n\n')
            if paragraph_end != -1:
                surrounding_text = surrounding_text[:paragraph_end]
                
            links = re.findall(r'\[([^\]]+)\]\(([^\)]+)\)', surrounding_text)
            
            seen_links = set()
            processed_links = []
            for link_title, link_path in links:
                if ('_kanban' not in link_title.lower() and 
                    '_kanban' not in link_path.lower() and 
                    'kanban board' not in link_title.lower() and
                    'References' not in link_title):
                    link_title = link_title.strip()
                    link_path = f"./{Path(link_path).stem}.md"
                    link_key = (link_title, link_path)
                    if link_key not in seen_links:
                        processed_links.append(link_key)
                        seen_links.add(link_key)
            
            tasks[state].append({
                'context': context,
                'links': processed_links,
                'source': {
                    'title': note_title,
                    'path': relative_path
                }
            })
            total_tasks += 1
            log_trace("Added task in state '{}': {}", state, context)
    
    log_debug("Found {} tasks in note '{}'", total_tasks, note_title)
    return tasks

def generate_kanban_board(tasks: Dict[str, List[Dict[str, Any]]]) -> str:
    """Generate a markdown kanban board."""
    log_debug("Generating kanban board")
    current_time = datetime.now().strftime("%Y-%m-%d %H:%M:%S")
    
    board = [
        "---",
        "kanban: true",
        f"last_updated: {current_time}",
        "---",
        "",
        "# 📋 Kanban Board",
        f"\nLast updated: {current_time}\n",
        "| 📅 Planned | ✅ Todo | 🏃 Doing | ✨ Done |",
        "|------------|---------|----------|---------|",
        "| | | | |"  # Add empty row for empty board
    ]
    
    total_tasks = 0
    for state in KANBAN_STATES:
        for task in tasks[state]:
            row = [" "] * len(KANBAN_STATES)
            
            cell_parts = []
            cell_parts.append(task['context'])
            cell_parts.append(f"📎 [View in {task['source']['title']}]({task['source']['path']})")
            
            if task['links']:
                cell_parts.append("🔗 Related:")
                for title, url in task['links']:
                    cell_parts.append(f"- [{title}]({url})")
            
            row[KANBAN_STATES.index(state)] = "<br>".join(cell_parts)
            board.append(f"| {' | '.join(row)} |")
            total_tasks += 1
    
    log_debug("Generated board with {} total tasks", total_tasks)
    return "\n".join(board)

def process_event(event_json: str) -> Optional[str]:
    try:
        event = json.loads(event_json)
        log_info("📋 Processing event for kanban board")
        
        if isinstance(event, dict):
            event_type = event.get("Created") or event.get("Updated") or event.get("Synced")
            if event_type and "content" in event_type:
                content = event_type["content"]
                title = event_type["title"]
                file_path = event_type.get("file_path", f"/notes/{title}.md")
                note_dir = str(Path(file_path).parent)
                
                log_debug("Processing note: {}", title)
                tasks_cache = load_tasks_cache(event_json)
                new_tasks = extract_tasks(content, title, file_path)
                
                task_count = sum(len(items) for items in new_tasks.values())
                if task_count > 0:
                    log_debug("Updating cache with {} tasks from '{}'", task_count, title)
                    tasks_cache[title] = new_tasks
                else:
                    if title in tasks_cache:
                        log_debug("Removing '{}' from cache (no tasks)", title)
                        tasks_cache.pop(title, None)
                
                save_tasks_cache(tasks_cache, event_json)
                
                combined_tasks = {state: [] for state in KANBAN_STATES}
                for note_tasks in tasks_cache.values():
                    for state in KANBAN_STATES:
                        combined_tasks[state].extend(note_tasks[state])
                
                kanban_file = get_kanban_file(note_dir)
                board_content = generate_kanban_board(combined_tasks)
                with open(kanban_file, 'w', encoding='utf-8') as f:
                    f.write(board_content)
                
                total_tasks = sum(len(items) for items in combined_tasks.values())
                log_info("✨ Updated kanban board with {} tasks from {} notes", 
                        total_tasks, len(tasks_cache))
                
                return json.dumps({
                    "metadata": {
                        "kanban_tasks": str(total_tasks),
                        "kanban_updated": datetime.now().strftime("%Y-%m-%d %H:%M:%S")
                    }
                })
                
        return None
            
    except Exception as e:
        log_error("Failed to process kanban board: {}", str(e))
        return None 