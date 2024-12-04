import json
import re
from collections import Counter
from datetime import datetime
from logging_utils import log_debug, log_info, log_error, log_trace

def calculate_metrics(content):
    log_debug("Calculating content metrics")
    
    # Basic counts
    word_count = len(content.split())
    char_count = len(content)
    log_trace("Basic counts - words: {}, chars: {}", word_count, char_count)
    
    # Count sentences (basic approximation)
    sentences = re.split(r'[.!?]+', content)
    sentence_count = len([s for s in sentences if s.strip()])
    
    # Average words per sentence
    avg_words_per_sentence = round(word_count / sentence_count if sentence_count > 0 else 0, 2)
    log_debug("Sentence analysis - count: {}, avg words: {}", 
             sentence_count, avg_words_per_sentence)
    
    # Count links
    markdown_links = len(re.findall(r'\[([^\]]+)\]\(([^\)]+)\)', content))
    log_trace("Found {} markdown links", markdown_links)
    
    # Count headers (excluding frontmatter)
    headers = len(re.findall(r'^#{1,6}\s+.+$', content, re.MULTILINE))
    log_trace("Found {} headers", headers)
    
    # Count bullet points
    bullet_points = len(re.findall(r'^\s*[-*+]\s+', content, re.MULTILINE))
    log_trace("Found {} bullet points", bullet_points)
    
    # Find most common words (excluding common stop words)
    stop_words = {'the', 'a', 'an', 'and', 'or', 'but', 'in', 'on', 
                 'at', 'to', 'for', 'of', 'with', 'by'}
    words = [word.lower() for word in re.findall(r'\b\w+\b', content)]
    word_freq = Counter(w for w in words if w not in stop_words)
    top_words = ', '.join([word for word, _ in word_freq.most_common(5)])
    log_debug("Top words found: {}", top_words)
    
    metrics = {
        "word_count": str(word_count),
        "char_count": str(char_count),
        "sentence_count": str(sentence_count),
        "avg_words_per_sentence": str(avg_words_per_sentence),
        "link_count": str(markdown_links),
        "header_count": str(headers),
        "bullet_point_count": str(bullet_points),
        "top_words": top_words,
        "last_analyzed": datetime.now().strftime("%Y-%m-%d %H:%M:%S %z")
    }
    
    log_debug("Generated metrics: {}", metrics)
    return metrics

def process_event(event_json):
    try:
        event = json.loads(event_json)
        log_info("📊 Processing content metrics")
        
        if isinstance(event, dict):
            event_type = event.get("Created") or event.get("Updated") or event.get("Synced")
            if event_type and "content" in event_type:
                title = event_type.get("title", "unknown")
                log_debug("Analyzing content for note: {}", title)
                
                metrics = calculate_metrics(event_type["content"])
                log_info("✨ Generated metrics for '{}' - {} words, {} sentences", 
                        title, metrics["word_count"], metrics["sentence_count"])
                
                # Wrap metrics in the expected metadata structure
                result = {
                    "metadata": metrics,
                    "content": None
                }
                
                return json.dumps(result)
        
        log_debug("No suitable content found for metrics calculation")
        return None
            
    except Exception as e:
        log_error("Failed to process content metrics: {}", str(e))
        return None 