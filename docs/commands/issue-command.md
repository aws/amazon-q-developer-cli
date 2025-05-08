# Issue Command

## Overview

The `/issue` command allows users to create GitHub issues directly from the Amazon Q CLI. It captures relevant context from the current conversation, including conversation history, context files, and system settings, to help with troubleshooting and bug reporting.

## Command Details

- **Name**: `issue`
- **Description**: Create a GitHub issue with conversation context
- **Usage**: `/issue <title> [--expected-behavior <text>] [--actual-behavior <text>] [--steps-to-reproduce <text>]`
- **Requires Confirmation**: No

## Implementation Approach

Rather than implementing a separate command handler for the `/issue` command, we leverage the existing `report_issue` tool functionality. This approach provides several benefits:

1. **Reuse of Existing Code**: The `report_issue` tool already implements all the necessary functionality for creating GitHub issues with proper context inclusion.

2. **Consistent Behavior**: Using the existing tool ensures that issues created through the command interface behave identically to those created through the tool interface.

3. **Reduced Maintenance Burden**: By avoiding duplicate implementations, we reduce the risk of divergent behavior and the maintenance burden of keeping two implementations in sync.

## Functionality

When the `/issue` command is invoked, the system:

1. Parses the command arguments to extract the issue title and optional details
2. Creates a `GhIssueContext` with the current conversation state
3. Initializes a `GhIssue` instance with the provided parameters
4. Sets the context on the `GhIssue` instance
5. Invokes the issue creation process, which:
   - Formats the conversation transcript
   - Gathers context file information
   - Collects system settings
   - Opens the default browser with a pre-filled GitHub issue template

## Context Information Included

The issue includes the following context information:

- **Conversation Transcript**: Recent conversation history (limited to the last 10 messages)
- **Context Files**: List of context files with their sizes
- **Chat Settings**: Interactive mode status and other settings
- **Tool Permissions**: List of trusted tools
- **Failed Request IDs**: Any failed request IDs for debugging purposes

## Example Usage

```
/issue "Command completion not working for git commands"
```

```
/issue "Unexpected error when adding context files" --steps-to-reproduce "1. Run q chat\n2. Try to add a large file as context\n3. Observe the error"
```

## Related Commands

- `/context`: Manage context files that will be included in the issue
- `/tools`: Manage tool permissions that will be included in the issue

## Notes

- The issue is created in the [amazon-q-developer-cli](https://github.com/aws/amazon-q-developer-cli) repository
- The browser will open with a pre-filled issue template
- You can edit the issue details before submitting
- The issue includes system information to help with troubleshooting
