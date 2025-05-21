#!/usr/bin/env python3
"""
Test script for the documentation enhancement process.
This script tests the code analysis functions without making API calls.
"""

import os
import json
import argparse
# Import functions directly to avoid OpenAI dependency during testing
import sys
import os

# Add the current directory to the path so we can import from enhance_docs
sys.path.append(os.path.dirname(os.path.abspath(__file__)))

# Import only the functions we need for testing
from enhance_docs import find_command_implementation, extract_command_details, extract_code_snippets

def parse_args():
    parser = argparse.ArgumentParser(description='Test CLI documentation enhancement')
    parser.add_argument('--code-dir', required=True, help='Root directory of the codebase')
    parser.add_argument('--command', required=True, help='Command name to test')
    parser.add_argument('--output', help='Output file for results')
    parser.add_argument('--verbose', action='store_true', help='Enable verbose output')
    return parser.parse_args()

def main():
    args = parse_args()
    
    print(f"Testing documentation enhancement for command: {args.command}")
    
    # Find command implementation
    command_files = find_command_implementation(args.code_dir, args.command, args.verbose)
    
    if not command_files:
        print(f"Error: Could not find implementation for command '{args.command}'")
        return
    
    print(f"Found {len(command_files)} relevant files")
    
    # Extract command details
    command_details = extract_command_details(command_files, args.command, args.verbose)
    
    # Extract code snippets
    code_snippets = extract_code_snippets(command_files, args.command)
    
    # Print results
    print("\nCommand Details:")
    print(json.dumps(command_details, indent=2))
    
    print("\nCode Snippets:")
    print(code_snippets[:500] + "..." if len(code_snippets) > 500 else code_snippets)
    
    # Save results if output file specified
    if args.output:
        with open(args.output, 'w', encoding='utf-8') as f:
            json.dump({
                "command": args.command,
                "details": command_details,
                "snippets": code_snippets
            }, f, indent=2)
        print(f"\nResults saved to {args.output}")

if __name__ == "__main__":
    main()
