# Shell Awareness Feature for Amazon Q CLI

## Overview

This feature adds shell awareness to Amazon Q CLI, allowing it to detect and display the correct shell-specific tool name (e.g., `execute_zsh` instead of `execute_bash`) when running commands.

## The Problem

The original Q CLI always showed `execute_bash` in the UI, regardless of what shell the user was actually running. This was misleading for users running zsh, fish, or other shells, as it suggested the tool wasn't aware of their shell environment.

### Root Cause Analysis

The issue was caused by **three separate shell detection systems** that weren't coordinated:

1. **Tool Creation System** (`tool_manager.rs`): Created shell-specific tools but used basic detection
2. **Execute Module** (`execute/mod.rs`): Had its own simple shell detection for permissions/settings
3. **UI Display System** (`tools/mod.rs`): Had hardcoded shell detection for display names

Each system used different detection logic:
- Simple detection: Just checked `$SHELL` environment variable and split on `/`
- The problem: `$SHELL` shows the **login shell** (often bash), not the **current shell** (e.g., zsh)

### Specific Issues Found

1. **Environment Variable Mismatch**: Even in zsh sessions, `$SHELL` often shows `/bin/bash` because that's the login shell
2. **Missing Shell-Specific Variables**: `$ZSH_VERSION` wasn't available to child processes in some configurations
3. **Inconsistent Detection**: Three different detection methods led to mismatched results
4. **Hardcoded Tool References**: Some code still had hardcoded `execute_bash` references

## The Solution

### Unified Shell Detection

We implemented a **unified shell detection system** that all three components use:

```rust
pub fn detect_shell_for_execute() -> String {
    // Method 1: Check shell-specific environment variables
    if env::var("ZSH_VERSION").is_ok() {
        return "zsh".to_string();
    }
    if env::var("BASH_VERSION").is_ok() {
        return "bash".to_string();
    }
    
    // Method 2: Active detection - test if zsh is available and working
    if let Ok(output) = Command::new("zsh").args(&["-c", "echo $ZSH_VERSION"]).output() {
        if output.status.success() {
            if let Ok(version) = String::from_utf8(output.stdout) {
                let version = version.trim();
                if !version.is_empty() {
                    return "zsh".to_string();
                }
            }
        }
    }
    
    // Method 3: Fallback to SHELL environment variable
    if let Ok(shell_path) = env::var("SHELL") {
        if let Some(shell_name) = shell_path.split('/').last() {
            match shell_name {
                "zsh" => return "zsh".to_string(),
                "bash" => return "bash".to_string(),
                // ... other shells
            }
        }
    }
    
    // Default fallback
    "bash".to_string()
}
```

### Key Improvements

1. **Active Detection**: Tests if zsh is actually available by running `zsh -c "echo $ZSH_VERSION"`
2. **Unified Function**: All three systems now call the same detection function
3. **Consistent Results**: Tool creation, execution, and display all show the same shell
4. **Removed Hardcoded References**: Eliminated hardcoded `execute_bash` references

### Files Modified

- `crates/chat-cli/src/cli/chat/tool_manager.rs`: Unified tool creation
- `crates/chat-cli/src/cli/chat/tools/execute/mod.rs`: Updated execute module detection  
- `crates/chat-cli/src/cli/chat/tools/mod.rs`: Fixed UI display detection
- `crates/chat-cli/src/cli/chat/tools/mod.rs`: Removed hardcoded `execute_bash` from tool list

## Testing the Feature

### Prerequisites

Since this feature isn't in the released version of Q CLI, you'll need to build it from source:

1. **Get the Code**:
   ```
   # Clone the repository (or use existing clone)
   git clone https://github.com/aws/amazon-q-developer-cli.git
   cd amazon-q-developer-cli
   git checkout shell-awareness-feature
   ```

2. **Install Rust** (if not already installed):
   ```
   curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
   rustup default stable
   ```

### Building and Testing

1. **Build the Project**:
   ```
   cargo build --release
   ```

2. **Test the Feature**:
   ```
   # Run the built version directly (don't install it to avoid conflicts)
   ./target/release/chat_cli
   ```

### Expected Behavior

#### Before the Fix
```
🛠️ Using tool: execute_bash (trusted)
```
(Even when running in zsh)

#### After the Fix
```
🛠️ Using tool: execute_zsh (trusted)
```
(When running in zsh)

### Test Cases

1. **In zsh**: Should show `execute_zsh`
2. **In bash**: Should show `execute_bash`  
3. **In fish**: Should show `execute_fish` (if fish is detected)
4. **Unknown shells**: Should fall back to `execute_bash`

### Debug Output

The feature includes debug output to help verify detection:

```
DEBUG: Unix shell detection starting...
DEBUG: Detected shell: zsh
DEBUG: Creating tool: execute_zsh
DEBUG: Execute module detected shell: zsh
DEBUG: Execute module using tool_name: execute_zsh
```

## Limitations and Future Work

### Current Limitations

1. **Fish Detection**: May not work perfectly due to fish's unique environment variable handling
2. **Complex Shell Setups**: May not detect shells in complex nested or containerized environments
3. **Performance**: Active detection adds a small startup cost (running `zsh -c` command)

### Shell Support Status

- ✅ **zsh**: Fully supported with active detection
- ✅ **bash**: Fully supported  
- ⚠️ **fish**: Basic support (may not detect reliably)
- ⚠️ **Other shells**: Basic support via `$SHELL` variable parsing

### Future Improvements

1. **Better Fish Detection**: Implement fish-specific detection methods
2. **Caching**: Cache shell detection results to improve performance
3. **Configuration Override**: Allow users to manually specify their shell
4. **Extended Shell Support**: Add detection for more shells (ksh, tcsh, etc.)

## Contributing

If you encounter issues with shell detection or want to improve support for additional shells, please:

1. Fork the repository
2. Create a feature branch
3. Add debug output to understand the detection behavior
4. Test with your specific shell configuration
5. Submit a pull request with improvements

## Troubleshooting

### Shell Not Detected Correctly

1. **Check Environment Variables**:
   ```
   echo "SHELL: $SHELL"
   echo "ZSH_VERSION: $ZSH_VERSION"
   echo "BASH_VERSION: $BASH_VERSION"
   ```

2. **Test Active Detection**:
   ```
   zsh -c "echo $ZSH_VERSION"  # Should return version if zsh works
   ```

3. **Check Debug Output**: Look for the debug messages during Q CLI startup

### Performance Issues

If shell detection is slow:
1. The active detection method may be taking time
2. Consider using a simpler detection method for your use case
3. File an issue with your shell configuration details
