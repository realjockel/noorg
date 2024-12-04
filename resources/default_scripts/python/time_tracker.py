import json
import re
from datetime import datetime, timedelta
from pathlib import Path
from typing import Dict, List, Optional, Tuple, Any
from logging_utils import log_debug, log_info, log_error
import os
import sys

DEFAULT_CONFIG = {
    "expected_hours_per_week": 40,
    "workdays": ["Monday", "Tuesday", "Wednesday", "Thursday", "Friday"],
    "vacation_days_per_year": 30
}

class TimeBlock:
    def __init__(self, start: str, end: str):
        self.start = start
        self.end = end
    
    def duration_minutes(self) -> float:
        start_time = parse_time(self.start)
        end_time = parse_time(self.end)
        if not (start_time and end_time):
            log_error(f"Invalid time format: start={self.start}, end={self.end}")
            return 0.0
        
        duration = (end_time - start_time).total_seconds() / 60
        log_debug(f"Duration calculation: {self.start} to {self.end} = {duration} minutes")
        return duration
    
    @staticmethod
    def parse(time_str: str) -> List['TimeBlock']:
        """Parse time blocks from string format '09:00-12:00,13:00-17:00'"""
        blocks = []
        if not time_str or time_str.strip() in ['-', 'N/A']:
            return blocks
        
        parts = time_str.split(',')
        for part in parts:
            if '-' in part:
                start, end = part.strip().split('-')
                blocks.append(TimeBlock(start.strip(), end.strip()))
        return blocks

def get_app_data_dir(event_json: str) -> Path:
    """Get the notes directory from the event filepath."""
    try:
        event = json.loads(event_json)
        if isinstance(event, dict):
            event_type = event.get("Created") or event.get("Updated") or event.get("Synced")
            if event_type and "file_path" in event_type:
                filepath = Path(event_type["file_path"])
                log_debug("Event filepath: {}", filepath)
                return filepath.parent
            else:
                log_error("No file_path found in event: {}", event)
    except Exception as e:
        log_error("Failed to get app data directory: {}", str(e))
    
    log_error("Falling back to default location")
    return Path.home() / ".local" / "share" / "note_cli"

def get_tracker_file(event_json: str) -> Path:
    """Get the path to the time tracker file."""
    try:
        event = json.loads(event_json)
        if isinstance(event, dict):
            event_type = event.get("Created") or event.get("Updated") or event.get("Synced")
            if event_type and "file_path" in event_type:
                # Get the parent directory of the note's file path
                note_path = Path(event_type["file_path"])
                tracker_path = note_path.parent / "_time_tracker.md"
                log_info(f"Creating time tracker at: {tracker_path}")
                return tracker_path
            else:
                log_error("No file_path found in event: {}", event)
    except Exception as e:
        log_error("Failed to get app data directory: {}", str(e))
    
    # Fallback to default location
    fallback_path = Path.home() / ".local" / "share" / "note_cli" / "notes" / "_time_tracker.md"
    log_error(f"Falling back to default location: {fallback_path}")
    return fallback_path

def parse_time(time_str: str) -> Optional[datetime]:
    """Parse time string in format HH:MM."""
    try:
        return datetime.strptime(time_str.strip(), "%H:%M")
    except ValueError:
        return None

def calculate_day_hours(work_blocks: List[TimeBlock], break_blocks: List[TimeBlock]) -> float:
    """Calculate working hours for a day considering multiple time blocks."""
    total_work_minutes = sum(block.duration_minutes() for block in work_blocks)
    total_break_minutes = sum(block.duration_minutes() for block in break_blocks)
    
    working_minutes = total_work_minutes - total_break_minutes
    return round(working_minutes / 60, 2)

def parse_config(content: str) -> Dict:
    """Parse configuration from the document."""
    config = DEFAULT_CONFIG.copy()
    
    config_section = re.search(r'## Configuration\n(.*?)\n\n', content, re.DOTALL)
    if config_section:
        config_text = config_section.group(1)
        for line in config_text.splitlines():
            line = line.strip()
            if ':' in line:
                key, value = [part.strip() for part in line.split(':', 1)]
                key = key.lower().replace(' ', '_')
                
                if key == 'expected_hours_per_week':
                    config[key] = float(value)
                elif key == 'workdays':
                    config[key] = [day.strip() for day in value.split(',')]
                elif key == 'vacation_days_per_year':
                    config[key] = int(value)
    
    return config

def parse_entries(content: str) -> List[Dict[str, Any]]:
    """Parse time entries from the content."""
    entries = []
    lines = content.split('\n')
    in_entries = False
    header_seen = False
    
    for line in lines:
        if line.startswith('## Time Entries'):
            in_entries = True
            continue
        if in_entries and line.startswith('|'):
            if not header_seen:
                header_seen = True  # Skip the header row
                continue
            if line.startswith('|--'):  # Skip the separator row
                continue
            if '|' not in line:  # Skip empty or malformed lines
                continue
            
            parts = [p.strip() for p in line.split('|')]
            if len(parts) >= 6 and parts[1]:  # Ensure we have enough parts and date is not empty
                entries.append({
                    'date': parts[1],
                    'type': parts[2],
                    'work_times': TimeBlock.parse(parts[3]),  # Convert to TimeBlock objects
                    'break_times': TimeBlock.parse(parts[4]),  # Convert to TimeBlock objects
                    'notes': parts[5]
                })
    
    return entries

def calculate_balance(entries: List[Dict], config: Dict) -> Tuple[float, str]:
    total_worked = 0.0
    expected_per_week = config['expected_hours_per_week']
    total_expected = 0.0
    
    # Group entries by month and week for summaries
    monthly_stats = {}
    weekly_stats = {}
    
    for entry in entries:
        try:
            if entry['date'] == 'Date':
                continue
                
            entry_date = datetime.strptime(entry['date'], "%Y-%m-%d")
            month_key = entry_date.strftime("%Y-%m")
            week_key = entry_date.strftime("%Y-W%W")  # ISO week number
            
            # Initialize month stats
            if month_key not in monthly_stats:
                monthly_stats[month_key] = {
                    'worked': 0.0,
                    'expected': expected_per_week * 4,
                    'vacation_days': 0,
                    'sick_days': 0
                }
                total_expected += expected_per_week * 4
            
            # Initialize week stats
            if week_key not in weekly_stats:
                weekly_stats[week_key] = {
                    'worked': 0.0,
                    'expected': expected_per_week,
                    'start_date': entry_date - timedelta(days=entry_date.weekday()),
                    'vacation_days': 0,
                    'sick_days': 0
                }
            
            entry_type = entry['type'].lower()
            if entry_type == 'workday':
                hours = calculate_day_hours(
                    entry['work_times'],
                    entry['break_times']
                )
                if hours > 0:
                    total_worked += hours
                    monthly_stats[month_key]['worked'] += hours
                    weekly_stats[week_key]['worked'] += hours
            elif entry_type in ['vacation', 'sick']:
                if entry_type == 'vacation':
                    monthly_stats[month_key]['vacation_days'] += 1
                    weekly_stats[week_key]['vacation_days'] += 1
                else:
                    monthly_stats[month_key]['sick_days'] += 1
                    weekly_stats[week_key]['sick_days'] += 1
        
        except Exception as e:
            log_error(f"Error processing entry {entry}: {str(e)}")
            continue
    
    balance = total_worked - total_expected
    
    # Build the summary text
    summary_parts = [
        "### Overall Summary",
        f"Total hours worked: {total_worked:.2f}h",
        f"Expected hours: {total_expected:.2f}h",
        f"Balance: {balance:+.2f}h\n",
        f"Status: {'✅ On track' if balance >= 0 else '⚠️ Behind schedule'}\n",
        "### Weekly Summary\n",
        "| Week | Dates | Hours Worked | Expected Hours | Balance | Cumulative Balance |",
        "|------|-------|--------------|----------------|---------|-------------------|"
    ]

    # Add weekly summaries
    cumulative_balance = 0.0
    for week in sorted(weekly_stats.keys(), reverse=True):
        stats = weekly_stats[week]
        week_balance = stats['worked'] - stats['expected']
        cumulative_balance += week_balance
        start_date = stats['start_date']
        end_date = start_date + timedelta(days=6)
        
        summary_parts.append(
            f"| {week} | {start_date.strftime('%Y-%m-%d')} to {end_date.strftime('%Y-%m-%d')} | "
            f"{stats['worked']:.2f}h | {stats['expected']:.2f}h | {week_balance:+.2f}h | {cumulative_balance:+.2f}h |"
        )
    
    summary_parts.extend([
        "\n### Monthly Summary\n"
    ])

    # Add monthly summaries
    for month in sorted(monthly_stats.keys()):
        stats = monthly_stats[month]
        month_balance = stats['worked'] - stats['expected']
        
        summary_parts.extend([
            f"#### {month}",
            f"Hours worked: {stats['worked']:.2f}h",
            f"Expected hours: {stats['expected']:.2f}h",
            f"Balance: {month_balance:+.2f}h",
            f"Vacation days: {stats['vacation_days']}",
            f"Sick days: {stats['sick_days']}\n"
        ])

    return balance, "\n".join(summary_parts)

def generate_tracker_content(original_content: str, config: Dict[str, Any], entries: List[Dict[str, str]]) -> str:
    """Generate the full content for the time tracker."""
    try:
        # Keep existing entries if they exist
        existing_entries = parse_entries(original_content) if original_content else []
        
        # Merge existing entries with any new ones, avoiding duplicates
        all_entries = existing_entries
        for new_entry in entries:
            if not any(e['date'] == new_entry['date'] for e in existing_entries):
                all_entries.append(new_entry)
        
        # Sort entries by date in reverse order
        all_entries.sort(key=lambda x: x['date'], reverse=True)
        
        # Calculate balances with all entries
        balance, summary = calculate_balance(all_entries, config)
        
        # Generate the content with all entries preserved
        content_lines = [
            "---",
            "time_tracker: true",
            "---",
            "",
            "# ⏱️ Time Tracker",
            "",
            "## Configuration",
            f"Expected Hours per Week: {config['expected_hours_per_week']}",
            f"Workdays: {', '.join(config['workdays'])}",
            f"Vacation Days per Year: {config['vacation_days_per_year']}",
            "",
            summary,  # This now includes both Overall and Monthly summaries
            "",
            "## Time Entries",
            "| Date | Type | Work Times | Break Times | Notes |",
            "|------|------|------------|-------------|--------|"
        ]
        
        # Add all entries to the table
        for entry in all_entries:
            work_times_str = ','.join(f"{b.start}-{b.end}" for b in entry['work_times']) or '-'
            break_times_str = ','.join(f"{b.start}-{b.end}" for b in entry['break_times']) or '-'
            content_lines.append(
                f"| {entry['date']} | {entry['type']} | {work_times_str} | {break_times_str} | {entry['notes']} |"
            )
        
        content_lines.append("<!-- Format: Work Times: 09:00-12:00,13:00-17:00 | Break Times: 12:00-13:00 -->")
        
        return "\n".join(content_lines)
        
    except Exception as e:
        log_error(f"Failed to generate tracker content: {str(e)}")
        return original_content

def process_event(event_json: str) -> Optional[str]:
    try:
        event = json.loads(event_json)
        
        # Get the tracker file path and check if it exists
        tracker_file = get_tracker_file(event_json)
        file_exists = tracker_file.exists()
               # Create default content if file doesn't exist
        default_content = "\n".join([
            "---",
            "time_tracker: true",
            "---",
            "",
            "# ⏱️ Time Tracker",
            "",
            "## Configuration",
            f"Expected Hours per Week: {DEFAULT_CONFIG['expected_hours_per_week']}",
            f"Workdays: {', '.join(DEFAULT_CONFIG['workdays'])}",
            f"Vacation Days per Year: {DEFAULT_CONFIG['vacation_days_per_year']}",
            "",
            "## Summary",
            "### Overall Summary",
            "Total hours worked: 0.00h",
            "Expected hours: 0.00h",
            "Balance: +0.00h",
            "",
            "Status: ✅ On track",
            "",
            "### Monthly Summary",
            "",
            "## Time Entries",
            "| Date | Type | Work Times | Break Times | Notes |",
            "|------|------|------------|-------------|--------|",
            "<!-- Format: Work Times: 09:00-12:00,13:00-17:00 | Break Times: 12:00-13:00 -->"
        ]) 
        # If file exists, we can do early return for non-time tracker files
        if file_exists:
            if isinstance(event, dict):
                event_type = event.get("Created") or event.get("Updated") or event.get("Synced")
                if event_type and "title" in event_type:
                    title = event_type["title"]
                    if title != "_time_tracker":
                        log_debug("Skipping non-time tracker file: {}", title)
                        return None
        
        # Get content from event or use default
        content = None
        for event_type in ["Created", "Updated", "Synced"]:
            if event_type in event:
                event_data = event[event_type]
                event_file = Path(event_data.get("file_path", ""))
                
                # If this is the time tracker file, use its content
                if event_file == tracker_file:
                    content = event_data.get("content")
                    break
        
        # If no content exists, create the file with default template
        if not content:
            log_info("Creating new time tracker with default template")
            tracker_file.parent.mkdir(parents=True, exist_ok=True)
            tracker_file.write_text(default_content)
            log_info(f"Created time tracker file at: {tracker_file}")
            content = default_content
            
            return json.dumps({
                "metadata": {
                    "time_tracker": "true",
                    "hour_balance": "+0.00",
                    "time_entries": "0",
                    "tracker_updated": datetime.now().strftime("%Y-%m-%d %H:%M:%S")
                },
                "content": content
            })

        # Early return if this isn't the time tracker file
        if isinstance(event, dict):
            event_type = event.get("Created") or event.get("Updated") or event.get("Synced")
            if event_type and "title" in event_type:
                title = event_type["title"]
                if title != "_time_tracker":
                    log_debug("Skipping non-time tracker file: {}", title)
                    return None
            
        # Process existing content
        log_info("⏱️ Processing event for time tracker")
        entries = parse_entries(content)
        config = parse_config(content) or DEFAULT_CONFIG
        
        # Generate new content with updated calculations
        updated_content = generate_tracker_content(content, config, entries)
        balance, _ = calculate_balance(entries, config)
        
        # Only return new content if it's different from the original
        should_update = updated_content != content
        
        return json.dumps({
            "metadata": {
                "time_entries": str(len(entries)),
                "hour_balance": f"{balance:+.2f}",
                "tracker_updated": datetime.now().strftime("%Y-%m-%d %H:%M:%S")
            },
            "content": updated_content if should_update else content
        })
            
    except Exception as e:
        log_error("Failed to process time tracker: {}", str(e))
        return None 