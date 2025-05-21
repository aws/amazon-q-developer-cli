#!/usr/bin/env python3
"""
Script to update the GitHub Actions workflow to include the documentation enhancement step.
"""

import os
import sys
import yaml

def update_workflow(workflow_path):
    """Update the GitHub Actions workflow to include the documentation enhancement step."""
    
    # Read the existing workflow file
    with open(workflow_path, 'r', encoding='utf-8') as f:
        workflow = yaml.safe_load(f)
    
    # Find the build-docs job
    if 'jobs' not in workflow or 'build-docs' not in workflow['jobs']:
        print("Error: Could not find build-docs job in workflow file")
        return False
    
    build_docs_job = workflow['jobs']['build-docs']
    
    # Find the steps in the build-docs job
    if 'steps' not in build_docs_job:
        print("Error: Could not find steps in build-docs job")
        return False
    
    steps = build_docs_job['steps']
    
    # Find the index of the step that generates documentation
    generate_docs_index = None
    for i, step in enumerate(steps):
        if 'name' in step and step['name'] == 'Generate documentation':
            generate_docs_index = i
            break
    
    if generate_docs_index is None:
        print("Error: Could not find 'Generate documentation' step")
        return False
    
    # Find the index of the step that sets up mdBook
    mdbook_index = None
    for i, step in enumerate(steps):
        if 'name' in step and step['name'] == 'Setup mdBook structure':
            mdbook_index = i
            break
    
    if mdbook_index is None:
        print("Error: Could not find 'Setup mdBook structure' step")
        return False
    
    # Create the new steps for documentation enhancement
    enhance_docs_steps = [
        {
            'name': 'Install documentation enhancement dependencies',
            'run': 'python -m pip install openai'
        },
        {
            'name': 'Enhance documentation with LLM',
            'run': 'python scripts/enhance_docs.py --input-dir docs/generated --code-dir . --output-dir docs/enhanced',
            'env': {
                'OPENAI_API_KEY': '${{ secrets.OPENAI_API_KEY }}'
            }
        }
    ]
    
    # Insert the new steps after the generate documentation step
    for i, step in enumerate(enhance_docs_steps):
        steps.insert(generate_docs_index + 1 + i, step)
    
    # Update the mdBook setup step to use the enhanced docs
    for i, step in enumerate(steps):
        if 'name' in step and step['name'] == 'Setup mdBook structure':
            run_lines = step['run'].split('\n')
            for j, line in enumerate(run_lines):
                if line.strip().startswith('cp -r docs/generated/*'):
                    run_lines[j] = '    cp -r docs/enhanced/* docs/src/'
            step['run'] = '\n'.join(run_lines)
            steps[i] = step
            break
    
    # Write the updated workflow file
    with open(workflow_path, 'w', encoding='utf-8') as f:
        yaml.dump(workflow, f, default_flow_style=False)
    
    return True

def main():
    if len(sys.argv) < 2:
        print("Usage: python update_github_workflow.py <workflow_path>")
        sys.exit(1)
    
    workflow_path = sys.argv[1]
    
    if not os.path.exists(workflow_path):
        print(f"Error: Workflow file not found: {workflow_path}")
        sys.exit(1)
    
    if update_workflow(workflow_path):
        print(f"Successfully updated workflow file: {workflow_path}")
    else:
        print(f"Failed to update workflow file: {workflow_path}")
        sys.exit(1)

if __name__ == "__main__":
    main()
