# Some hints on build process

## Work hints

**IMPORTANT** `cargo check` output can become extremely large after major changes, ONLY use temporary file + sub-q to analyze the output for any `cargo check` call
**IMPORTANT** Use command template like `cd /path/to/package && echo "Build started at: $(date)" && echo "Output file: /tmp/build_output_$(date +%s).txt" && cargo check > /tmp/build_output_$(date +%s).txt 2>&1 && echo "Build completed"` - it is optimized for this environment
