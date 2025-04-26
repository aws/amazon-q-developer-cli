# Amazon Q Git Hook Installation

This document provides instructions for installing a sample git hook that prevents accidental `git push` operations while using Amazon Q chat.

Note - Git hooks do not chain.  If you need more than one hook installed, you will require other tools to link more than one into a single hook.  Follow the install instructions for those other tools instead.

## What This Hook Does

The `pre-push` hook checks if the `QCHAT_PROCESS_ID` environment variable is set (which indicates an active Amazon Q chat session). If the variable is set, the hook blocks the push operation and displays an error message.

## Installation Instructions

1. Copy the hook file to your git hooks directory:

```bash
# Navigate to your git repository
cd /path/to/your/repo

# Create hooks directory if it doesn't exist
mkdir -p .git/hooks

# Copy the hook file
cp /path/to/pre-push-hook .git/hooks/pre-push

# Make the hook executable
chmod +x .git/hooks/pre-push
```

2. Verify the installation:

```bash
ls -la .git/hooks/pre-push
```

## Testing the Hook

To test if the hook is working correctly:

1. Start an Amazon Q chat session:
```bash
q chat
```

2. From within q chat, as it to push to a git repository:
```bash
> git push this repo
```

You should see an error message indicating that the push is blocked while Amazon Q chat is active.

## Global Installation

To install this hook for all repositories:

1. Configure git to use a global hooks directory:

```bash
# Create a global hooks directory
mkdir -p ~/.git-hooks

# Configure git to use this directory
git config --global core.hooksPath ~/.git-hooks
```

2. Install the pre-push hook in the global directory:

```bash
cp /path/to/pre-push-hook ~/.git-hooks/pre-push
chmod +x ~/.git-hooks/pre-push
```

## Uninstalling the Hook

To remove the hook:

```bash
# Delete the hook file
rm .git/hooks/pre-push
```

To remove a globally installed hook:

```bash
# Delete the hook file
rm ~/.git-hooks/pre-push
```


## Troubleshooting

If the hook isn't working:

1. Ensure the hook file is executable (`chmod +x .git/hooks/pre-push`)
2. Verify the hook path is correct (check `git config core.hooksPath`)
3. Make sure the hook doesn't have Windows line endings if you're on Unix/Linux
4. Check if you have other git configurations that might override hooks
