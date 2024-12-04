"""
import json
from datetime import datetime

def process_event(event_json):
    try:
        event = json.loads(event_json)
        print(f"Debug - Received event: {event}")  # Debug print
        
        # Check if event is a dict and has the expected structure
        if isinstance(event, dict):
            event_type = event.get("Created") or event.get("Updated") or event.get("Synced")
            if event_type:
                metadata = {
                    "created_at": datetime.now().isoformat(),
                }
                print(f"Debug - Returning metadata: {metadata}")  # Debug print
                return json.dumps(metadata)
            
        print(f"Debug - No metadata generated")
        return None
            
    except Exception as e:
        print(f"Debug - Error processing event: {e}")
        return None
"""