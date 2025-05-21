#!/usr/bin/env python3
"""
Enhanced documentation extraction script for Amazon Q Developer CLI.
This script parses Rust source files to extract command documentation with parameters
and implements selective regeneration.
"""

import os
import re
import json
import argparse
import hashlib
from datetime import datetime
from typing import Dict, List, Any

def extract_commands_from_file(file_path):
    """Extract commands from a Rust file using more robust patterns."""
    print(f"Processing file: {file_path}")
    
    with open(file_path, 'r') as f:
        content = f.read()
    
    commands = {}
    
    # First, try to find the CliRootCommands enum
    enum_match = re.search(r'pub enum CliRootCommands\s*\{([^}]*)\}', content, re.DOTALL)
    if enum_match:
        enum_body = enum_match.group(1)
        
        # Extract command variants with their descriptions
        command_pattern = r'///\s*(.*?)\s*\n\s*(?:#\[command[^\]]*\]\s*)?(\w+)'
        command_matches = re.finditer(command_pattern, enum_body, re.DOTALL)
        
        for match in command_matches:
            description_raw = match.group(1).strip()
            command_name = match.group(2).lower()
            
            # Skip if this is not a command (e.g., it's a struct field)
            if command_name in ['debug', 'clone', 'copy', 'default', 'partialeq', 'eq', 'valueenum']:
                continue
                
            # Clean up the description - take only the first sentence
            description = description_raw.split('.')[0].strip()
            if not description:
                description = description_raw.split('\n')[0].strip()
            
            print(f"Found command: {command_name} - {description}")
            commands[command_name] = {
                "name": command_name,
                "description": description,
                "parameters": extract_parameters(content, command_name),
                "examples": extract_examples(content, command_name),
                "source_file": file_path
            }
    
    # Also look for individual command definitions
    command_pattern = r'///\s*(.*?)\s*\n\s*#\[command[^\]]*\]\s*(\w+)'
    command_matches = re.finditer(command_pattern, content, re.DOTALL)
    
    for match in command_matches:
        description_raw = match.group(1).strip()
        command_name = match.group(2).lower()
        
        # Clean up the description - take only the first sentence
        description = description_raw.split('.')[0].strip()
        if not description:
            description = description_raw.split('\n')[0].strip()
        
        print(f"Found command: {command_name} - {description}")
        commands[command_name] = {
            "name": command_name,
            "description": description,
            "parameters": extract_parameters(content, command_name),
            "examples": extract_examples(content, command_name),
            "source_file": file_path
        }
    
    return commands

def extract_parameters(content, command_name):
    """Extract parameters for a command."""
    parameters = []
    
    # Look for arg definitions
    arg_pattern = r'\.arg\(\s*Arg::new\("([^"]+)"\)[^;]*?\.help\("([^"]+)"\)'
    arg_matches = re.finditer(arg_pattern, content, re.DOTALL)
    
    for match in arg_matches:
        param_name = match.group(1)
        help_text = match.group(2)
        
        # Try to determine if it's a flag or option
        param_type = "flag"
        if "takes_value" in match.group(0):
            param_type = "option"
        
        parameters.append({
            "name": param_name,
            "description": help_text,
            "type": param_type
        })
    
    return parameters

def extract_examples(content, command_name):
    """Extract usage examples for a command."""
    examples = []
    
    # Look for examples in comments
    example_pattern = r'///\s*Example:?\s*```(?:bash|sh)?\s*(.*?)\s*```'
    example_matches = re.finditer(example_pattern, content, re.DOTALL)
    
    for match in example_matches:
        example_text = match.group(1).strip()
        if command_name in example_text:
            examples.append(example_text)
    
    return examples

def calculate_content_hash(command):
    """Calculate a hash of the command content for change detection."""
    content = json.dumps(command, sort_keys=True)
    return hashlib.md5(content.encode()).hexdigest()

def load_metadata(metadata_path):
    """Load metadata from previous run if available."""
    if os.path.exists(metadata_path):
        with open(metadata_path, 'r') as f:
            return json.load(f)
    return {"command_hashes": {}, "last_updated": None}

def save_metadata(metadata, metadata_path):
    """Save metadata for future runs."""
    metadata["last_updated"] = datetime.now().isoformat()
    with open(metadata_path, 'w') as f:
        json.dump(metadata, f, indent=2)

def generate_command_doc(command, output_dir):
    """Generate markdown documentation for a command."""
    output_path = os.path.join(output_dir, f"{command['name']}.md")
    
    with open(output_path, 'w') as f:
        f.write(f"# {command['name']}\n\n")
        f.write(f"{command['description']}\n\n")
        
        # Add parameters section if available
        if command['parameters']:
            f.write("## Parameters\n\n")
            for param in command['parameters']:
                param_type = "flag" if param['type'] == "flag" else "option"
                f.write(f"### --{param['name']} ({param_type})\n\n")
                f.write(f"{param['description']}\n\n")
        
        # Add examples section if available
        if command['examples']:
            f.write("## Examples\n\n")
            for example in command['examples']:
                f.write(f"```bash\n{example}\n```\n\n")
    
    return output_path

def main():
    parser = argparse.ArgumentParser(description='Extract documentation from Amazon Q CLI source code')
    parser.add_argument('--source', required=True, help='Source directory containing CLI code')
    parser.add_argument('--output', required=True, help='Output directory for documentation')
    parser.add_argument('--force', action='store_true', help='Force regeneration of all documentation')
    
    args = parser.parse_args()
    
    # Specific files to check for command definitions
    target_files = [
        os.path.join(args.source, "crates/q_cli/src/cli/mod.rs"),
        os.path.join(args.source, "crates/chat-cli/src/cli/mod.rs"),
        os.path.join(args.source, "crates/chat-cli/src/cli/chat/mod.rs")
    ]
    
    # Create output directory
    if not os.path.exists(args.output):
        os.makedirs(args.output)
    
    # Path for metadata storage
    metadata_path = os.path.join(args.output, '.metadata.json')
    metadata = load_metadata(metadata_path)
    
    all_commands = {}
    
    # Process each target file
    for file_path in target_files:
        if os.path.exists(file_path):
            commands = extract_commands_from_file(file_path)
            all_commands.update(commands)
    
    # Manual corrections for known issues
    corrections = {
        "chat": "AI assistant in your terminal",
        "translate": "Natural Language to Shell translation",
        "settings": "Customize appearance & behavior",
        "diagnostic": "Run diagnostic tests",
        "setup": "Setup CLI components",
        "uninstall": "Uninstall Amazon Q",
        "update": "Update the Amazon Q application",
        "user": "Manage your account",
        "integrations": "Manage system integrations",
        "mcp": "Model Context Protocol (MCP)",
        "inline": "Inline shell completions",
        "hook": "Hook commands",
        "debug": "Debug the app",
        "telemetry": "Enable/disable telemetry",
        "version": "Show version information",
        "issue": "Create a new GitHub issue"
    }
    
    for cmd, desc in corrections.items():
        if cmd in all_commands:
            all_commands[cmd]["description"] = desc
    
    # Filter out non-command entries
    commands_to_remove = []
    for cmd in all_commands:
        if cmd in ['no_confirm', 'changelog', 'rootuser']:
            commands_to_remove.append(cmd)
    
    for cmd in commands_to_remove:
        if cmd in all_commands:
            del all_commands[cmd]
    
    print(f"Total commands found: {len(all_commands)}")
    
    # Track which commands were updated
    updated_commands = []
    updated_count = 0
    
    # Generate documentation for each command, but only if changed or forced
    for name, command in all_commands.items():
        command_hash = calculate_content_hash(command)
        previous_hash = metadata["command_hashes"].get(name)
        
        if args.force or previous_hash != command_hash:
            output_path = generate_command_doc(command, args.output)
            metadata["command_hashes"][name] = command_hash
            updated_commands.append(name)
            updated_count += 1
            print(f"Generated documentation for {name}")
    
    # Generate index file
    with open(os.path.join(args.output, 'index.md'), 'w') as f:
        f.write("# Amazon Q CLI Command Reference\n\n")
        f.write("This documentation is automatically generated from the Amazon Q CLI source code.\n\n")
        f.write("## Available Commands\n\n")
        
        for name, cmd in sorted(all_commands.items()):
            f.write(f"- [{name}]({name}.md): {cmd['description']}\n")
    
    # Save metadata for future runs
    save_metadata(metadata, metadata_path)
    
    print(f"Documentation generation complete. Updated {updated_count} of {len(all_commands)} commands.")

if __name__ == "__main__":
    main()
