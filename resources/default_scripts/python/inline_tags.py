import json
import re
import os
import sys
import traceback
from typing import Set
from logging_utils import log_debug, log_info, log_error, log_warn

def get_environment_info():
    """Collect environment information for debugging."""
    info = {
        "Python Version": sys.version,
        "Python Path": sys.path,
        "Working Directory": os.getcwd(),
        "Environment Variables": {
            k: v for k, v in os.environ.items() 
            if k.startswith(("PYTHON", "PATH", "HOME", "USER"))
        },
        "Module Search Paths": [str(p) for p in sys.path],
    }
    log_debug("Environment Info: {}", json.dumps(info, indent=2))
    return info

def extract_inline_tags(content: str) -> Set[str]:
    """Extract all inline hashtags from the content, excluding heading anchors."""
    try:
        log_debug("Starting inline tag extraction")
        env_info = get_environment_info()
        
        # First, remove all heading anchor links
        content = re.sub(r'\* \[.*?\]\(#.*?\)', '', content)
        
        # Match hashtags
        tags = re.findall(r'(?<![\w]\]\()#([\w-]+)(?!\))', content)
        
        # Filter out common heading-related words
        heading_words = {'contents', 'references', 'table-of-contents'}
        tags = {tag for tag in tags if tag.lower() not in heading_words}
        
        log_debug("Found {} tags: {}", len(tags), tags)
        return tags
    except Exception as e:
        log_error("Error in extract_inline_tags: {}\n{}", str(e), traceback.format_exc())
        raise

def merge_tags(existing_tags: str, new_tags: Set[str]) -> str:
    """Merge existing tags with new tags, avoiding duplicates."""
    try:
        # Convert existing tags string to set
        if existing_tags:
            current_tags = {tag.strip() for tag in existing_tags.split(',')}
        else:
            current_tags = set()
        
        # Merge with new tags
        all_tags = current_tags.union(new_tags)
        
        # Sort and join tags
        return ', '.join(sorted(all_tags))
    except Exception as e:
        log_error("Error in merge_tags: {}\n{}", str(e), traceback.format_exc())
        raise

def process_event(event_json):
    try:
        log_info(" Starting Python observer execution")
        log_debug("Received event: {}", event_json)
        
        # Verify Python environment
        env_info = get_environment_info()
        
        # Parse event
        event = json.loads(event_json)
        log_info("📝 Processing event for tag extraction")
        
        if not isinstance(event, dict):
            log_error("Event is not a dictionary: {}", type(event))
            return json.dumps({"metadata": {}, "error": "Invalid event format"})
        
        # Extract event type
        event_type = event.get("Created") or event.get("Updated") or event.get("Synced")
        if not event_type:
            log_error("No valid event type found in: {}", event.keys())
            return json.dumps({"metadata": {}, "error": "Invalid event type"})
        
        # Process content
        content = event_type.get("content")
        if not content:
            log_warn("No content found in event")
            return json.dumps({"metadata": {}, "error": "No content found"})
        
        # Extract tags
        inline_tags = extract_inline_tags(content)
        if inline_tags:
            log_info("🏷️ Found {} inline tags", len(inline_tags))
            log_debug("Tags: {}", inline_tags)
        else:
            log_info("ℹ️ No inline tags found")
        
        # Process frontmatter
        existing_tags = ""
        if "frontmatter" in event_type:
            existing_tags = event_type["frontmatter"].get("tags", "")
            log_debug("Existing tags: {}", existing_tags)
        
        # Merge tags
        combined_tags = merge_tags(existing_tags, inline_tags)
        
        # Prepare response
        metadata = {
            "metadata": {
                "tags": combined_tags,
                "inline_tags_found": str(len(inline_tags)),
                "python_version": sys.version.split()[0],
                "script_path": __file__,
            }
        }
        
        log_debug("Generated metadata: {}", metadata)
        return json.dumps(metadata)
            
    except Exception as e:
        error_info = {
            "error": str(e),
            "traceback": traceback.format_exc(),
            "python_version": sys.version,
            "python_path": sys.path,
            "working_dir": os.getcwd(),
            "env_vars": {k: v for k, v in os.environ.items() if k.startswith(("PYTHON", "PATH"))}
        }
        log_error("Error processing event: {}\nFull error info: {}", 
                 str(e), json.dumps(error_info, indent=2))
        return json.dumps({"metadata": {}, "error": error_info})
