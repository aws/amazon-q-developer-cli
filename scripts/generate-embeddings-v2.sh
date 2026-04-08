#!/bin/bash
# generate-embeddings-v2.sh - Run this when V2 docs change to regenerate embeddings

set -e

echo "Generating embeddings for V2 documentation..."
echo ""

# Step 1: Build doc index
echo "Step 1: Building doc index from frontmatter..."
python3 autodocs-v2/meta/scripts/build-doc-index.py
INDEX_SIZE=$(du -h autodocs-v2/meta/doc-index.json | cut -f1)
echo "  Doc index: $INDEX_SIZE"

# Step 2: Index docs with semantic search
echo ""
echo "Step 2: Indexing docs with semantic-search-client..."
rm -rf ~/.kiro/doc-search-v2
cargo run --quiet -p agent --example index_autodocs_v2

# Step 3: Package search index
echo ""
echo "Step 3: Packaging search index..."
tar -czf autodocs-v2/meta/doc-search-index.tar.gz -C ~/.kiro/doc-search-v2 .
SEARCH_INDEX_SIZE=$(du -h autodocs-v2/meta/doc-search-index.tar.gz | cut -f1)
echo "  Search index: $SEARCH_INDEX_SIZE"

echo ""
echo "✓ Ready for build:"
echo "  - doc-index.json ($INDEX_SIZE) - metadata for progressive loading"
echo "  - doc-search-index.tar.gz ($SEARCH_INDEX_SIZE) - semantic search index"
echo ""
echo "Next: cargo build (both will be embedded in binary via agent crate)"
