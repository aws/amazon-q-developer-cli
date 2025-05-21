# Amazon Q CLI Documentation

This directory contains the documentation for the Amazon Q Developer CLI.

## Directory Structure

- `src/` - Source Markdown files for mdBook
- `generated/` - Generated Markdown files from the extraction script
- `book/` - Generated HTML files from mdBook
- `book.toml` - mdBook configuration file

## Building the Documentation

To build the documentation:

```bash
# Install mdBook (if not already installed)
cargo install mdbook

# Build the documentation
cd docs
mdbook build
```

The generated HTML files will be in the `book/` directory.

## Customization

To customize the documentation:

1. Edit the `book.toml` file to change mdBook settings
2. Add custom CSS to `src/custom.css`
3. Add custom JavaScript to `src/custom.js`

## Workflow

The documentation workflow is:

1. Extract documentation from source code to `generated/`
2. Copy generated files to `src/`
3. Build HTML documentation with mdBook
4. Deploy HTML files to S3/CloudFront

This process is automated via GitHub Actions.
