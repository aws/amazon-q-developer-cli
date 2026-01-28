#!/usr/bin/env python3
"""
Build doc-index.json from YAML frontmatter in all documentation files.
Used for progressive loading fallback when semantic search is unavailable.
"""

import json
import re
from datetime import datetime
from pathlib import Path

from typing import Optional

def parse_frontmatter(content: str) -> Optional[dict]:
    """Extract YAML frontmatter from markdown content."""
    match = re.match(r'^---\s*\n(.*?)\n---\s*\n', content, re.DOTALL)
    if not match:
        return None
    
    yaml_content = match.group(1)
    result = {}
    
    # Simple YAML parsing for our known structure
    current_key = None
    for line in yaml_content.split('\n'):
        if not line.strip():
            continue
        
        # Handle doc_meta: prefix
        if line.strip() == 'doc_meta:':
            continue
            
        # Handle key: value
        if ':' in line and not line.strip().startswith('-'):
            indent = len(line) - len(line.lstrip())
            key_val = line.strip().split(':', 1)
            key = key_val[0].strip()
            val = key_val[1].strip() if len(key_val) > 1 else ''
            
            if val.startswith('[') and val.endswith(']'):
                # Inline array: [a, b, c]
                val = [v.strip().strip('"\'') for v in val[1:-1].split(',') if v.strip()]
            elif val:
                val = val.strip('"\'')
            else:
                val = []  # Will be populated by following list items
            
            result[key] = val
            current_key = key if isinstance(val, list) and not val else None
        
        # Handle list items
        elif line.strip().startswith('-') and current_key:
            item = line.strip()[1:].strip().strip('"\'')
            if isinstance(result.get(current_key), list):
                result[current_key].append(item)
    
    return result

def build_index():
    """Build doc-index.json from all markdown files."""
    autodocs_path = Path(__file__).parent.parent.parent
    docs_path = autodocs_path / 'docs'
    output_path = autodocs_path / 'meta' / 'doc-index.json'
    
    if not docs_path.exists():
        print(f"Error: {docs_path} does not exist")
        return
    
    documents = []
    by_category = {}
    
    for md_file in sorted(docs_path.rglob('*.md')):
        rel_path = md_file.relative_to(docs_path)
        content = md_file.read_text(encoding='utf-8')
        
        meta = parse_frontmatter(content)
        if not meta:
            print(f"  Warning: No frontmatter in {rel_path}")
            continue
        
        doc = {
            'path': str(rel_path),
            'file': md_file.name,
            'title': meta.get('title', md_file.stem),
            'description': meta.get('description', ''),
            'category': meta.get('category', 'unknown'),
            'keywords': meta.get('keywords', []),
            'related': meta.get('related', []),
        }
        
        # Optional fields
        if 'validated' in meta:
            doc['validated'] = meta['validated']
        if 'status' in meta:
            doc['status'] = meta['status']
        
        documents.append(doc)
        
        # Group by category
        cat = doc['category']
        if cat not in by_category:
            by_category[cat] = []
        by_category[cat].append(str(rel_path))
    
    index = {
        'generated_at': datetime.now().isoformat(),
        'total_docs': len(documents),
        'by_category': by_category,
        'documents': documents,
    }
    
    with open(output_path, 'w') as f:
        json.dump(index, f, indent=2)
    
    print(f"  Built index: {len(documents)} docs")

if __name__ == '__main__':
    build_index()
