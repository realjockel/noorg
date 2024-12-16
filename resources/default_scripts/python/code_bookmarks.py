# code_bookmarks.py

import json
import re
from pathlib import Path
from typing import Dict, List, Tuple
from logging_utils import log_debug, log_info, log_error

def extract_code_blocks(content: str) -> List[Tuple[str, str, str]]:
    """Extract code blocks with language and optional title."""
    pattern = r'```(\w+)(?:\s+([^`\n]*)?)?\n(.*?)```'
    blocks = []
    
    try:
        # Skip frontmatter
        content_without_frontmatter = content
        if content.startswith('---'):
            end_marker = content.find('---', 3)
            if end_marker != -1:
                content_without_frontmatter = content[end_marker + 3:]

        for match in re.finditer(pattern, content_without_frontmatter, re.DOTALL):
            language = match.group(1).lower()
            title = match.group(2).strip() if match.group(2) else ''
            code = match.group(3).strip()
            
            # Skip empty blocks or bookmarks header content
            if not code or "Code blocks extracted from notes" in code:
                continue
            
            # Skip if code looks like frontmatter or bookmarks header
            if code.startswith('---') and 'code_bookmarks:' in code:
                continue
                
            blocks.append((language, title, code))
            
        log_debug(f"Extracted {len(blocks)} code blocks")
        return blocks
        
    except Exception as e:
        log_error(f"Error extracting code blocks: {e}")
        return []

def get_bookmarks_file(note_dir: str) -> Path:
    """Get path to code bookmarks file."""
    return Path(note_dir) / '_code_bookmarks.md'

def get_json_file(note_dir: str) -> Path:
    """Get path to JSON file for storing code blocks."""
    return Path(note_dir) / '_code_bookmarks.json'

def load_json(json_file: Path) -> Dict[str, List[Dict[str, str]]]:
    """Load code blocks from JSON file."""
    if not json_file.exists():
        return {}
    
    try:
        with open(json_file, 'r') as f:
            return json.load(f)
    except Exception as e:
        log_error(f"Error loading JSON file: {e}")
        return {}

def save_json(json_file: Path, data: Dict[str, List[Dict[str, str]]]) -> None:
    """Save code blocks to JSON file."""
    try:
        with open(json_file, 'w') as f:
            json.dump(data, f, indent=4)
    except Exception as e:
        log_error(f"Error saving JSON file: {e}")

def generate_header() -> List[str]:
    """Generate the standard header for code bookmarks file."""
    return [
        "---",
        "code_bookmarks: true",
        "skip_observers: all", 
        "---",
        "",
        "# 📚 Code Bookmarks",
        "",
        "Code blocks extracted from notes, organized by language.",
        ""
    ]

def generate_bookmarks_content(data: Dict[str, List[Dict[str, str]]]) -> str:
    """Generate markdown content organizing code blocks by language."""
    lines = []

    for lang in sorted(data.keys()):
        if not data[lang]:  # Skip empty language sections
            continue
        lines.extend([
            f"## {lang.upper()}",
            ""
        ])
        
        for block in data[lang]:
            title = block['title']
            code = block['code']
            source = block['source']
            if title.strip():
                lines.append(f"### {title}")
            lines.append(f"From: [{source}](./{source}.md)")
            lines.extend([
                "```" + lang,
                code,
                "```",
                ""
            ])

    return "\n".join(lines)  # Return only content, no header

def process_event(event_json: str) -> str:
    try:
        event = json.loads(event_json)
        event_type = event.get("Created") or event.get("Updated") or event.get("Synced")
        if not event_type:
            return json.dumps({"metadata": {}})
            
        content = event_type.get("content", "")
        title = event_type.get("title", "")
        file_path = event_type.get("file_path", "")
        
        if title == "_code_bookmarks":
            return json.dumps({"metadata": {}})
        
        # Get new blocks from current file (skip SQL)    
        new_blocks = [
            block for block in extract_code_blocks(content)
            if block[0].lower() != 'sql' and block[2].strip()
        ]
        
        if not new_blocks:
            return json.dumps({"metadata": {}})
        
        # Setup JSON file
        note_dir = str(Path(file_path).parent)
        json_file = get_json_file(note_dir)
        
        # Load existing data from JSON file
        data = load_json(json_file)
        
        # Update JSON data with new blocks
        for lang, title, code in new_blocks:
            if lang not in data:
                data[lang] = []
            data[lang].append({
                'title': title,
                'code': code,
                'source': file_path  # Correctly set the source file
            })
        
        # Save updated data to JSON file
        save_json(json_file, data)
        
        # Generate content using JSON data
        content = generate_bookmarks_content(data)
        
        # Write file with header if it doesn't exist
        bookmarks_file = get_bookmarks_file(note_dir)
        if not bookmarks_file.exists():
            with open(bookmarks_file, 'w') as f:
                f.write("\n".join(generate_header()) + "\n" + content)  # Fix concatenation
        else:
            # Read existing header
            with open(bookmarks_file, 'r') as f:
                existing_content = f.read()
                header_end = existing_content.find("# 📚 Code Bookmarks") + len("# 📚 Code Bookmarks")
                header = existing_content[:header_end]
            
            # Write updated content with preserved header
            with open(bookmarks_file, 'w') as f:
                f.write(header + "\n" + content)
            
        return json.dumps({
            "metadata": {
                "code_blocks_added": str(len(new_blocks)),
                "total_code_blocks": str(sum(len(blocks) for blocks in data.values())),
                "languages": ", ".join(sorted(data.keys())),
                "bookmarks_updated": "true"
            }
        })
            
    except Exception as e:
        log_error(f"Failed to process code bookmarks: {e}")
        return json.dumps({"metadata": {}, "error": str(e)})