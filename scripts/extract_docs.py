#!/usr/bin/env python3
"""
Documentation extraction script for Amazon Q Developer CLI.
This script parses Rust source files to extract command documentation.
"""

import os
import re
import json
import argparse

def extract_commands_from_file(file_path):
    """Extract commands from a Rust file using more robust patterns."""
    print("Processing file: {}".format(file_path))
    
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
            
            print("Found command: {} - {}".format(command_name, description))
            commands[command_name] = {
                "name": command_name,
                "description": description
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
        
        print("Found command: {} - {}".format(command_name, description))
        commands[command_name] = {
            "name": command_name,
            "description": description
        }
    
    return commands

def main():
    parser = argparse.ArgumentParser(description='Extract documentation from Amazon Q CLI source code')
    parser.add_argument('--source', required=True, help='Source directory containing CLI code')
    parser.add_argument('--output', required=True, help='Output directory for documentation')
    
    args = parser.parse_args()
    
    # Specific files to check for command definitions
    target_files = [
        os.path.join(args.source, "crates/q_cli/src/cli/mod.rs"),
        os.path.join(args.source, "crates/chat-cli/src/cli/mod.rs"),
        os.path.join(args.source, "crates/chat-cli/src/cli/chat/mod.rs")
    ]
    
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
    
    print("Total commands found: {}".format(len(all_commands)))
    
    # Create output directory
    if not os.path.exists(args.output):
        os.makedirs(args.output)
    
    # Generate index file
    with open(os.path.join(args.output, 'index.md'), 'w') as f:
        f.write("# Amazon Q CLI Command Reference\n\n")
        f.write("This documentation is automatically generated from the Amazon Q CLI source code.\n\n")
        f.write("## Available Commands\n\n")
        
        for name, cmd in sorted(all_commands.items()):
            f.write("- [{0}]({0}.md): {1}\n".format(name, cmd['description']))
    
    # Generate individual command files
    for name, cmd in all_commands.items():
        with open(os.path.join(args.output, "{}.md".format(name)), 'w') as f:
            f.write("# {}\n\n".format(name))
            f.write("{}\n\n".format(cmd['description']))

if __name__ == "__main__":
    main()
