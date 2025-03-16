# Context Profile Rename Feature Implementation

This document outlines the implementation details for adding a `--rename` option to the `/context profile` command in the Amazon Q Developer CLI.

## Implementation Checklist

- [x] Update the `ContextSubcommand` enum in `src/cli/chat/command.rs`
- [x] Add `rename_profile` method to the `ContextManager` struct
- [x] Update command parsing logic to handle the `--rename` option
- [x] Update command execution flow to handle the rename option
- [x] Update help text to include the rename option
- [x] Update command completion to include the rename option
- [x] Add unit tests for the rename functionality
- [x] Verify build and tests pass
- [x] Run formatter (`cargo +nightly fmt`)
- [x] Commit changes with conventional commit message

## 1. Update the `ContextSubcommand` Enum

First, we need to update the `ContextSubcommand` enum in `src/cli/chat/command.rs` to include the rename option:

```rust
pub enum ContextSubcommand {
    Show,
    Add { global: bool, paths: Vec<String> },
    Remove { global: bool, paths: Vec<String> },
    Profile { 
        delete: Option<String>, 
        create: Option<String>,
        rename: Option<(String, String)>, // (old_name, new_name)
    },
    Switch { name: String, create: bool },
    Clear { global: bool },
}
```

## 2. Add Rename Method to `ContextManager`

Next, we need to add a method to the `ContextManager` struct to handle renaming profiles:

```rust
impl ContextManager {
    // Existing methods...
    
    // Rename a profile
    pub fn rename_profile(&mut self, old_name: &str, new_name: &str) -> Result<()> {
        // Validate profile names
        if old_name == "default" {
            return Err(eyre!("Cannot rename the default profile"));
        }
        
        if new_name == "default" {
            return Err(eyre!("Cannot rename to 'default' as it's a reserved profile name"));
        }
        
        // Check if old profile exists
        let old_profile_path = self.profiles_dir.join(format!("{}.json", old_name));
        if !old_profile_path.exists() {
            return Err(eyre!("Profile '{}' not found", old_name));
        }
        
        // Check if new profile name already exists
        let new_profile_path = self.profiles_dir.join(format!("{}.json", new_name));
        if new_profile_path.exists() {
            return Err(eyre!("Profile '{}' already exists", new_name));
        }
        
        // Read the old profile configuration
        let profile_config = Self::load_profile_config(&self.profiles_dir, old_name)?;
        
        // Write the configuration to the new profile file
        let file = File::create(&new_profile_path)?;
        serde_json::to_writer_pretty(file, &profile_config)?;
        
        // Delete the old profile file
        fs::remove_file(&old_profile_path)?;
        
        // If the current profile is being renamed, update the current_profile field
        if self.current_profile == old_name {
            self.current_profile = new_name.to_string();
            self.profile_config = profile_config;
        }
        
        Ok(())
    }
}
```

## 3. Update Command Parsing Logic

Update the command parsing logic in `src/cli/chat/command.rs` to handle the `--rename` option:

```rust
// Inside the parse method for the Context command
"profile" => {
    let mut delete = None;
    let mut create = None;
    let mut rename = None;
    let mut i = 2;
    
    while i < parts.len() {
        match parts[i] {
            "--delete" | "-d" => {
                if i + 1 < parts.len() {
                    delete = Some(parts[i + 1].to_string());
                    i += 2;
                } else {
                    return Err("Missing profile name for --delete".to_string());
                }
            },
            "--create" | "-c" => {
                if i + 1 < parts.len() {
                    create = Some(parts[i + 1].to_string());
                    i += 2;
                } else {
                    return Err("Missing profile name for --create".to_string());
                }
            },
            "--rename" | "-r" => {
                if i + 2 < parts.len() {
                    rename = Some((parts[i + 1].to_string(), parts[i + 2].to_string()));
                    i += 3;
                } else {
                    return Err("Missing profile names for --rename. Usage: --rename <old_name> <new_name>".to_string());
                }
            },
            _ => {
                return Err(format!("Unknown option for profile command: {}", parts[i]));
            }
        }
    }
    
    // Ensure only one operation is specified
    let operations = [delete.is_some(), create.is_some(), rename.is_some()];
    if operations.iter().filter(|&&x| x).count() > 1 {
        return Err("Only one of --delete, --create, or --rename can be specified".to_string());
    }
    
    Self::Context { subcommand: ContextSubcommand::Profile { delete, create, rename } }
},
```

## 4. Update Command Execution Flow

Update the command execution flow in `src/cli/chat/mod.rs` to handle the rename option:

```rust
match Command::parse(&input) {
    Ok(Command::Context { subcommand: ContextSubcommand::Profile { delete, create, rename } }) => {
        if let Some(context_manager) = &mut conversation_state.context_manager {
            if let Some(name) = delete {
                match context_manager.delete_profile(&name) {
                    Ok(_) => println!("Deleted profile: {}", name),
                    Err(e) => eprintln!("Error deleting profile: {}", e),
                }
            } else if let Some(name) = create {
                match context_manager.create_profile(&name) {
                    Ok(_) => println!("Created profile: {}", name),
                    Err(e) => eprintln!("Error creating profile: {}", e),
                }
            } else if let Some((old_name, new_name)) = rename {
                match context_manager.rename_profile(&old_name, &new_name) {
                    Ok(_) => println!("Renamed profile: {} -> {}", old_name, new_name),
                    Err(e) => eprintln!("Error renaming profile: {}", e),
                }
            } else {
                match context_manager.list_profiles() {
                    Ok(profiles) => {
                        for profile in profiles {
                            if profile == context_manager.current_profile {
                                println!("* {}", profile);
                            } else {
                                println!("  {}", profile);
                            }
                        }
                    },
                    Err(e) => eprintln!("Error listing profiles: {}", e),
                }
            }
        } else {
            println!("Context manager not initialized");
        }
    },
    // Other commands...
}
```

## 5. Update Help Text

Update the help text in `src/cli/chat/mod.rs` to include the rename option:

```rust
const HELP_TEXT: &str = color_print::cstr! {"

<magenta,em>q</magenta,em> (Amazon Q Chat)

<em>/clear</em>        <black!>Clear the conversation history</black!>
<em>/acceptall</em>    <black!>Toggles acceptance prompting for the session.</black!>
<em>/help</em>         <black!>Show this help dialogue</black!>
<em>/quit</em>         <black!>Quit the application</black!>
<em>/context</em>      <black!>Manage context files for the chat session</black!>
  <em>show</em>        <black!>Display current context configuration [--expand]</black!>
  <em>add</em>         <black!>Add file(s) to context [--global]</black!>
  <em>rm</em>          <black!>Remove file(s) from context [--global]</black!>
  <em>profile</em>     <black!>List, create [--create], delete [--delete], or rename [--rename] context profiles</black!>
  <em>switch</em>      <black!>Switch to a different context profile [--create]</black!>
  <em>clear</em>       <black!>Clear all files from current context [--global]</black!>

<em>!{command}</em>    <black!>Quickly execute a command in your current session</black!>

"};
```

## 6. Update Command Completion

Update the command completion in `src/cli/chat/prompt.rs` to include the rename option:

```rust
const COMMANDS: &[&str] = &[
    "/clear", 
    "/help", 
    "/acceptall", 
    "/quit",
    "/context",
    "/context show",
    "/context add",
    "/context rm",
    "/context profile",
    "/context profile --create",
    "/context profile --delete",
    "/context profile --rename",
    "/context switch",
    "/context clear"
];
```

## 7. Testing

Add unit tests for the rename functionality:

```rust
#[test]
fn test_rename_profile() -> Result<()> {
    // Create a test context manager
    let (mut manager, _temp_dir) = tests::create_test_context_manager()?;

    // Create a test profile
    manager.create_profile("test-profile")?;
    
    // Add a path to the profile
    manager.switch_profile("test-profile", false)?;
    manager.add_paths(vec!["test/path".to_string()], false)?;
    
    // Test renaming the profile
    manager.rename_profile("test-profile", "new-profile")?;
    
    // Verify the old profile file is gone
    let old_profile_path = manager.profiles_dir.join("test-profile.json");
    assert!(!old_profile_path.exists());
    
    // Verify the new profile file exists
    let new_profile_path = manager.profiles_dir.join("new-profile.json");
    assert!(new_profile_path.exists());
    
    // Verify the content was transferred
    let mut file = File::open(&new_profile_path)?;
    let mut contents = String::new();
    file.read_to_string(&mut contents)?;
    
    let config: ContextConfig = serde_json::from_str(&contents)?;
    assert_eq!(config.paths, vec!["test/path".to_string()]);
    
    // Verify the current profile was updated
    assert_eq!(manager.current_profile, "new-profile");

    Ok(())
}
```

## Summary

This implementation adds a `--rename` option to the `/context profile` command, allowing users to rename existing context profiles. The implementation includes:

1. Updating the `ContextSubcommand` enum to include the rename option
2. Adding a `rename_profile` method to the `ContextManager` struct
3. Updating the command parsing logic to handle the `--rename` option
4. Updating the command execution flow to handle the rename option
5. Updating the help text to include the rename option
6. Adding unit tests for the rename functionality

The implementation follows the existing patterns in the codebase and maintains consistency with the other context management commands.
