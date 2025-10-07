# Some hints on build process

## Work hints

**IMPORTANT** `cargo check` output can become extremely large after major changes, ONLY use temporary file + sub-q to analyze the output for any `cargo check` call
**IMPORTANT** You MUST use the following command template for the build:
 
 ```
cd /path/to/package && echo "Build started at: $(date)" && echo "Output file: /tmp/build_output_$(date +%s).txt" && cargo check > /tmp/build_output_$(date +%s).txt 2>&1 && echo "Build completed"
```
This template is heavily optimized for this environment!

**VERY IMPORTANT** ALWAYS USE THE BUILD COMMAND AS PROVIDED ABOVE! DO NOT try to make it fancier, DO NOT try to 

**EXTREMELY VERY BERRY IMPORTANT** SERIOUSLY. DO NOT modify this command, use it as is!