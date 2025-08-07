# Models

This directory contains machine learning models used by Amazon Q Developer CLI.

## Models Included

### all-MiniLM-L6-v2.zip
- **Purpose**: Semantic search and text embeddings for Amazon Q Developer CLI
- **Contains**: all-MiniLM-L6-v2 model from Hugging Face
- **Source**: [sentence-transformers/all-MiniLM-L6-v2](https://huggingface.co/sentence-transformers/all-MiniLM-L6-v2)
- **License**: Apache 2.0 (see LICENSE file)
- **Size**: ~83MB compressed
- **Usage**: Used for semantic code search and similarity matching

## Adding New Models

To add additional models:

1. Create a new zip file named after the model (e.g., `model-name.zip`)
2. Git LFS will automatically track all `.zip` files in this directory
3. Update this README with model information
4. Ensure proper license compliance

## License Compliance

This directory contains third-party software licensed under the Apache License 2.0.
All license requirements are met as follows:

1. **License Copy**: The full Apache 2.0 license text is provided in the LICENSE file
2. **Copyright Notice**: Original copyright notices are preserved
3. **Attribution**: Proper attribution is provided in NOTICE file
4. **Modification Notice**: Any modifications are clearly documented

## Git LFS

Model files are stored using Git LFS. To download the actual model files:

```bash
git lfs pull
```

## Files in this directory

- `all-MiniLM-L6-v2.zip` - The all-MiniLM-L6-v2 model files (tracked by Git LFS)
- `LICENSE` - Full Apache 2.0 license text
- `NOTICE` - Attribution and copyright notices
- `README.md` - This file

## Usage

Extract the model files to your local semantic search cache directory:

```bash
# Extract all-MiniLM-L6-v2 model
unzip models/all-MiniLM-L6-v2.zip -d ~/.semantic_search/models/all-MiniLM-L6-v2/
```

These models are redistributed in compliance with the Apache 2.0 license terms.
See the LICENSE file for full terms and conditions.
