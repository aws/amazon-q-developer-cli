#!/bin/bash
# generate-embeddings.sh - Run this when docs change to regenerate embeddings

set -e

echo "Generating embeddings for documentation..."
echo ""

# Step 1: Build doc index (metadata for progressive loading fallback)
echo "Step 1: Building doc index from frontmatter..."
python3 autodocs/meta/scripts/build-doc-index.py
INDEX_SIZE=$(du -h autodocs/meta/doc-index.json | cut -f1)
echo "  Doc index: $INDEX_SIZE"

# Step 2: Index docs with semantic search
echo ""
echo "Step 2: Indexing docs with semantic-search-client..."
# Remove existing index to force re-indexing with updated content
rm -rf ~/.kiro/doc-search
cargo run --quiet --example index_autodocs

# Step 3: Package search index
echo ""
echo "Step 3: Packaging search index..."
tar -czf autodocs/meta/doc-search-index.tar.gz -C ~/.kiro/doc-search .
SEARCH_INDEX_SIZE=$(du -h autodocs/meta/doc-search-index.tar.gz | cut -f1)
echo "  Search index: $SEARCH_INDEX_SIZE"

echo ""
echo "✓ Ready for build:"
echo "  - doc-index.json ($INDEX_SIZE) - metadata for progressive loading"
echo "  - doc-search-index.tar.gz ($SEARCH_INDEX_SIZE) - semantic search index"
echo ""
echo "Next: cargo build (both will be embedded in binary)"
