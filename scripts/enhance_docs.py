#!/usr/bin/env python3
"""
Amazon Q CLI Documentation Enhancement Script

This script enhances basic CLI documentation by:
1. Analyzing the codebase to extract detailed command information
2. Using GPT-4 to generate comprehensive, user-friendly documentation
3. Outputting enhanced Markdown files ready for mdBook processing

Usage:
  python enhance_docs.py --input-dir docs/generated --code-dir . --output-dir docs/enhanced
"""

import os
import sys
import json
import argparse
import re
from pathlib import Path
import openai

def parse_args():
    """Parse command line arguments."""
    parser = argparse.ArgumentParser(description='Enhance CLI documentation using LLM')
    parser.add_argument('--input-dir', required=True, help='Directory containing basic extracted docs')
    parser.add_argument('--code-dir', required=True, help='Root directory of the codebase')
    parser.add_argument('--output-dir', required=True, help='Directory to write enhanced docs')
    parser.add_argument('--api-key', help='OpenAI API key (or use env var)')
    parser.add_argument('--model', default='gpt-4', help='LLM model to use')
    parser.add_argument('--max-tokens', type=int, default=4000, help='Maximum tokens for LLM response')
    parser.add_argument('--temperature', type=float, default=0.5, help='LLM temperature (0.0-1.0)')
    parser.add_argument('--force', action='store_true', help='Force regeneration of all docs')
    parser.add_argument('--verbose', action='store_true', help='Enable verbose output')
    return parser.parse_args()

def find_command_implementation(code_dir, command_name, verbose=False):
    """Find relevant code files for a specific command."""
    if verbose:
        print(f"Looking for implementation of command: {command_name}")
    
    command_files = []
    
    # Common patterns for CLI commands in Rust
    patterns = [
        f"fn {command_name}",
        f"Command::new\\(\"{command_name}\"",
        f"SubCommand::with_name\\(\"{command_name}\"",
        f"pub struct {command_name.capitalize()}",
        f"\\.subcommand\\(.*\"{command_name}\"",
        f"app\\(.*\"{command_name}\"",
    ]
    
    # Search through Rust files
    for root, _, files in os.walk(os.path.join(code_dir, "crates")):
        for file in files:
            if file.endswith(".rs"):
                file_path = os.path.join(root, file)
                try:
                    with open(file_path, 'r', encoding='utf-8') as f:
                        content = f.read()
                        for pattern in patterns:
                            if re.search(pattern, content, re.MULTILINE):
                                if verbose:
                                    print(f"  Found match in: {file_path}")
                                command_files.append((file_path, content))
                                break
                except UnicodeDecodeError:
                    if verbose:
                        print(f"  Warning: Could not decode {file_path}")
                    continue
    
    return command_files

def extract_command_details(command_files, command_name, verbose=False):
    """Extract comprehensive command details from code files."""
    if verbose:
        print(f"Extracting details for command: {command_name}")
    
    details = {
        "parameters": [],
        "options": [],
        "subcommands": [],
        "examples": [],
        "error_handling": [],
        "related_commands": []
    }
    
    for file_path, content in command_files:
        # Look for clap argument definitions
        arg_matches = re.finditer(r'\.arg\(\s*(?:Arg::new|arg!)\(\s*"([^"]+)"\s*\)(?:[^;]+)', content)
        for match in arg_matches:
            arg_name = match.group(1)
            arg_def = match.group(0)
            
            # Determine if it's required
            is_required = "required(true)" in arg_def
            
            # Look for help text
            help_match = re.search(r'\.help\(\s*"([^"]+)"\s*\)', arg_def)
            help_text = help_match.group(1) if help_match else ""
            
            # Determine if it's a flag or takes a value
            takes_value = "takes_value(true)" in arg_def
            
            if takes_value:
                details["parameters"].append({
                    "name": arg_name,
                    "required": is_required,
                    "description": help_text
                })
                if verbose:
                    print(f"  Found parameter: {arg_name}")
            else:
                details["options"].append({
                    "name": arg_name,
                    "description": help_text
                })
                if verbose:
                    print(f"  Found option: {arg_name}")
        
        # Look for examples in code comments
        example_matches = re.finditer(r'//\s*Example:?\s*```(?:bash|sh)?\s*\n([\s\S]*?)```', content)
        for match in example_matches:
            example = match.group(1).strip()
            if command_name in example:
                details["examples"].append(example)
                if verbose:
                    print(f"  Found example: {example[:50]}...")
        
        # Extract error handling patterns
        error_matches = re.finditer(r'(?:Err|Error|error!)\((?:[^)]*)"([^"]+)"', content)
        for match in error_matches:
            error_msg = match.group(1)
            details["error_handling"].append(error_msg)
            if verbose:
                print(f"  Found error handling: {error_msg[:50]}...")
        
        # Find related commands
        if "commands.rs" in file_path or "cli.rs" in file_path:
            cmd_matches = re.finditer(r'\.subcommand\(\s*(?:Command::new|app!)\(\s*"([^"]+)"\s*\)', content)
            for match in cmd_matches:
                related_cmd = match.group(1)
                if related_cmd != command_name and related_cmd not in details["related_commands"]:
                    details["related_commands"].append(related_cmd)
                    if verbose:
                        print(f"  Found related command: {related_cmd}")
    
    return details

def extract_code_snippets(command_files, command_name, max_snippets=3, max_lines=30):
    """Extract relevant code snippets for the command."""
    snippets = []
    
    for file_path, content in command_files:
        lines = content.split('\n')
        
        # Look for the command implementation
        for i, line in enumerate(lines):
            if re.search(f"fn {command_name}", line) or re.search(f"Command::new\\(\"{command_name}\"", line):
                # Extract a snippet around this line
                start = max(0, i - 5)
                end = min(len(lines), i + max_lines)
                
                snippet = "\n".join(lines[start:end])
                snippets.append(f"```rust\n// From {os.path.basename(file_path)}\n{snippet}\n```")
                
                if len(snippets) >= max_snippets:
                    break
    
    return "\n\n".join(snippets)

def generate_enhanced_docs(basic_content, command_name, command_details, code_snippets, model="gpt-4", max_tokens=4000, temperature=0.5):
    """Generate enhanced documentation using GPT-4."""
    # Prepare a detailed prompt
    prompt = f"""
    You are an expert technical writer creating documentation for the Amazon Q Developer CLI.
    
    # TASK
    Create comprehensive, user-friendly documentation for the '{command_name}' command that follows AWS documentation best practices.
    
    # INPUT INFORMATION
    ## Basic Documentation
    {basic_content}
    
    ## Technical Details Extracted from Code
    {json.dumps(command_details, indent=2)}
    
    ## Relevant Code Snippets
    {code_snippets}
    
    # OUTPUT REQUIREMENTS
    Your documentation MUST include:
    
    1. A clear introduction explaining:
       - What the command does
       - When and why users would use it
       - Any prerequisites or requirements
    
    2. Command syntax section showing the basic usage pattern
    
    3. Parameters and options section with:
       - Complete list of all parameters and options
       - Clear descriptions of each
       - Default values and required/optional status
       - Data types or allowed values
    
    4. At least 3 practical examples showing:
       - Basic usage
       - Common use cases
       - Advanced scenarios with multiple options
    
    5. Troubleshooting section covering:
       - Common errors and their solutions
       - Tips for resolving issues
    
    6. Related commands section
    
    # STYLE GUIDELINES
    - Use a friendly, professional tone appropriate for AWS documentation
    - Be concise but thorough
    - Use proper Markdown formatting
    - Use tables for parameters and options
    - Use code blocks with syntax highlighting for examples
    - Focus on the user perspective, not implementation details
    
    Format your response in clean, well-structured Markdown.
    """
    
    # Call the OpenAI API
    response = openai.ChatCompletion.create(
        model=model,
        messages=[
            {"role": "system", "content": "You are an expert AWS technical writer who creates clear, comprehensive documentation following AWS style guidelines."},
            {"role": "user", "content": prompt}
        ],
        max_tokens=max_tokens,
        temperature=temperature
    )
    
    # Extract and return the enhanced documentation
    return response.choices[0].message.content

def should_regenerate(input_path, output_path, force=False):
    """Determine if documentation should be regenerated."""
    # Always regenerate if forced
    if force:
        return True
    
    # Regenerate if output doesn't exist
    if not os.path.exists(output_path):
        return True
    
    # Regenerate if input is newer than output
    input_mtime = os.path.getmtime(input_path)
    output_mtime = os.path.getmtime(output_path)
    
    return input_mtime > output_mtime

def main():
    args = parse_args()
    
    # Set up API key
    if args.api_key:
        openai.api_key = args.api_key
    elif 'OPENAI_API_KEY' in os.environ:
        openai.api_key = os.environ['OPENAI_API_KEY']
    else:
        print("Error: No API key provided. Use --api-key or set OPENAI_API_KEY environment variable.")
        sys.exit(1)
    
    # Create output directory if it doesn't exist
    os.makedirs(args.output_dir, exist_ok=True)
    
    # Process each command file
    for file_name in os.listdir(args.input_dir):
        if not file_name.endswith('.md'):
            continue
            
        command_name = file_name.replace('.md', '')
        input_path = os.path.join(args.input_dir, file_name)
        output_path = os.path.join(args.output_dir, file_name)
        
        # Check if we need to regenerate this file
        if not should_regenerate(input_path, output_path, args.force):
            print(f"Skipping {command_name} (up to date)")
            continue
        
        print(f"Processing command: {command_name}")
        
        # Read basic documentation
        with open(input_path, 'r', encoding='utf-8') as f:
            basic_content = f.read()
        
        # Find command implementation in code
        command_files = find_command_implementation(args.code_dir, command_name, args.verbose)
        
        if not command_files:
            print(f"Warning: Could not find implementation for command '{command_name}'")
            # Copy the original file if no implementation found
            with open(output_path, 'w', encoding='utf-8') as f:
                f.write(basic_content)
            continue
        
        # Extract command details from code
        command_details = extract_command_details(command_files, command_name, args.verbose)
        
        # Extract code snippets
        code_snippets = extract_code_snippets(command_files, command_name)
        
        # Generate enhanced documentation
        try:
            enhanced_content = generate_enhanced_docs(
                basic_content, 
                command_name, 
                command_details, 
                code_snippets,
                model=args.model,
                max_tokens=args.max_tokens,
                temperature=args.temperature
            )
            
            # Write enhanced documentation
            with open(output_path, 'w', encoding='utf-8') as f:
                f.write(enhanced_content)
            
            print(f"Enhanced documentation written to {output_path}")
            
        except Exception as e:
            print(f"Error enhancing documentation for {command_name}: {e}")
            # Copy the original file if enhancement fails
            with open(output_path, 'w', encoding='utf-8') as f:
                f.write(basic_content)

if __name__ == "__main__":
    main()
