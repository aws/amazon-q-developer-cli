# How to build the project

## Build process

**IMPORTANT** `cargo check` output can become extremely large after major changes, ONLY use temporary file + sub-q to analyze the output for any `cargo check` call
**IMPORTANT** You MUST use the following command template for the build:
 
 ```
cd /path/to/package && echo "Build started at: $(date)" && echo "Output file: /tmp/build_output_$(date +%s).txt" && cargo check > /tmp/build_output_$(date +%s).txt 2>&1 && echo "Build completed"
```
This template is heavily optimized for this environment!

**VERY IMPORTANT** ALWAYS USE THE BUILD COMMAND AS PROVIDED ABOVE! DO NOT try to make it fancier, DO NOT try to 

**EXTREMELY VERY BERRY IMPORTANT** SERIOUSLY. DO NOT modify this command, use it as is!

## Build analysys process - using simple sub-agent

When you need to analyze a large set of data, you CANT do it directly - it will overflow AI agent context window.

You can ask another instance of AI agent to do it. It's usually refrred to as 'sub-q'.

**CRITICAL**: NEVER, UNDER NO CIRCUMSTANCES use `--trust-all-tools` argument with sub-agent calls. NEVER.

Use the following bash command:
```
q chat --agent sub-agent --no-interactive "<PROMPT>"
```

In the prompt you have to provide following instructions:

```
You act as a build analyzer. Your goal is to read the following files into the memory, analyze them, and provide answers to questions below.

Files:
- <PATHS-TO-FILES, one path per item>

Questions:
- <QUESTIONS-ABOUT-THE CONTENT, one questyion per item>

Extra context:
- <EXTRA CONTEXT: some data you think would be useful for the agent to know. Nature of the data in the files, some information about the structure, where they come from. Anything you find useful to answer the questions>
```

**IMPORTANT** You MUST follow exactly provided command pattern - it's optimized in various ways
**IMPORTANT** You MUST put the prompt for the sub-q on one line with proper delimeters. It's presented as multi-line here only for readability.

