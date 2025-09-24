# Q CLI E2E Test Framework

This test framework provides comprehensive end-to-end testing capabilities for Amazon Q CLI using a hybrid approach with expectrl and feature-based categorization.

## 🏗️ Architecture

### **Hybrid Approach**
- **expectrl (PTY)**: For interactive commands (`/help`, `/tools`, `/quit`, etc.)
- **Direct Process Streams**: For AI prompts ("What is AWS?", "Hello", etc.)

### **Why Hybrid?**
- **Commands** write to PTY stream → expectrl captures perfectly ✅
- **AI responses** write to stdout → direct streams capture properly ✅

## 🎯 Categorized Test Framework

### **Feature-Based Organization**
Tests are organized into 12 functional categories using Rust features:

1. **Agent Commands** (8 tests) - `agent`
   - `/agent list`, `/agent create`, `/agent help`, etc.

2. **AI Prompts** (5 tests) - `ai_prompts`
   - "What is AWS?", "Hello" prompts

3. **Context Commands** (10 tests) - `context`
   - `/context show`, `/context add`, `/context help`, etc.

4. **Core Session Commands** (3 tests) - `core_session`
   - `/help`, `/quit`, `/clear`

5. **Integration Commands** (21 tests) - `integration`
   - `/subscribe`, `/hooks`, `/editor` help commands

6. **MCP Commands** (18 tests) - `mcp`
   - `/mcp`, `/mcp --help`

7. **Model Commands** (3 tests) - `model`
   - `/model`, `/model --help`

8. **Q Subcommands** (15 tests) - `q_subcommand`
   - q chat, q debug, q doctor, etc.

9. **Save/Load Commands** (10 tests) - `save_load`
   - `/save`, `/load`, help commands

10. **Session Management Commands** (14 tests) - `session_mgmt`
    - `/compact`, `/usage`, help commands

11. **Todos Commands** - `todos`
    - todos command

12. **Tools Commands** (15 tests) - `tools`
    - `/tools`, tool management commands

## 📁 Core Files

### **`src/lib.rs`**
Base helper class providing:
- `QChatSession::new()` - Start Q Chat session with timeout configuration
- `execute_command(cmd)` - Execute commands using expectrl + carriage return (`0x0D`) with character-by-character typing
- `send_prompt(prompt)` - Send AI prompts using direct process streams with concurrent stdout/stderr reading
- `send_key_input(key)` - Send key inputs for interactive navigation
- `execute_q_subcommand()` - Execute Q CLI subcommands directly in terminal
- `execute_q_subcommand_with_stdin()` - Execute Q CLI subcommands with stdin input support
- `execute_interactive_menu_selection()` - Handle interactive menu navigation with arrow keys and selection
- `execute_interactive_menu_selection_with_command()` - Execute interactive menu with full command string
- `read_response()` - Internal method for reading session responses with timeout handling
- `quit()` - Clean session termination

### **`tests/all_tests.rs`**
Main test entry point that includes all test modules and organizes them by feature categories.

### **`tests/*/mod.rs`**
Module files in test subdirectories that group related test functions by feature area.

### **`run_tests.py`**
Python test runner with:
- **Real-time per-test feedback** - Shows ✅/❌ as each test completes
- **Category organization** - Groups tests by functional area
- **Configurable categories** - Enable/disable categories for faster iteration
- **Quiet and verbose modes** - Control output detail level
- **Final summary reporting** - Shows passed/failed categories
- **HTML and JSON reports** - Generates detailed test reports in both formats
- **Run complete test suites** - Execute all available test categories
- **Run individual features** - Target specific test categories
- **Check list of available features** - Display all available test categories
- **Convert JSON report to HTML** - Transform JSON reports into HTML format
- **Custom binary support** - Specify custom Q CLI binary path, defaults to system-installed `q` if not provided

**Example Commands:**
```bash

# Run individual features
python run_tests.py --features agent,context --quiet

# Run default sanity test suite
python run_tests.py --quiet

# List available features
python run_tests.py --list-features

# Convert JSON to HTML report
python run_tests.py --json-to-html reports/test_report.json
```

## 🚀 Usage

### **Python Test Runner (Recommended)**

```bash
# Run all categories with real-time feedback
python run_tests.py --quiet

# Check help section for all available options
python run_tests.py --help

```

**Example Output:**
```
🧪 Running Sanity Test Suite
========================================
🔄 Running: usage with sanity
✅ usage (sanity) - 59.41s - 3 passed, 0 failed

📋 Feature Summary:
  ✅ usage (sanity): 3 passed, 0 failed
    ✅ session_mgmt::test_usage_command::test_usage_command ...
    ✅ session_mgmt::test_usage_command::test_usage_h_command ...
    ✅ session_mgmt::test_usage_command::test_usage_help_command ...

🎯 FINAL SUMMARY
================================
🏷️  Features Tested: 1
✅ Features 100% Pass: 1
❌ Features with Failures: 0
✅ Individual Tests Passed: 3
❌ Individual Tests Failed: 0
📊 Total Individual Tests: 3
📈 Success Rate: 100.0%

🎉 All tests passed!

📄 Detailed report saved to: reports/qcli_test_summary_usage_sanity_091625223834.json
🌐 HTML report saved to: reports/qcli_test_summary_usage_sanity_091625223834.html
```

### **Help and Configuration**
Use the help command to see all available options:

```bash
# View all available options and categories
python run_tests.py --help
```

```

e2etests % python run_tests.py --help 

Q CLI E2E Test Framework - Python script for comprehensive Amazon Q CLI testing
        
This Python script executes end-to-end tests organized into functional feature categories.
Default test suite is 'sanity' providing core functionality validation.
You can also specify 'regression' suite for extended testing (currently no tests added under regression).
Test execution automatically generates both JSON and HTML reports under the reports directory for detailed analysis.
JSON reports contain raw test data, system info, and execution details for programmatic use.
HTML reports provide visual dashboards with charts, summaries, and formatted test results.
Report filenames follow syntax: q_cli_e2e_report_{features}_{suite}_{timestamp}.json/html
Example sanity reports: q_cli_e2e_report_sanity_082825232555.json, example regression: q_cli_e2e_report_regression_082825232555.html

Additional Features:
  • JSON to HTML conversion: Convert JSON test reports to visual HTML dashboards
  • Feature discovery: Automatically detect available test features and list the available features
  • Multiple test suites: Support for sanity and regression test categories
  • Flexible feature selection: Run individual or grouped features
  • Comprehensive reporting: Generate both JSON and HTML reports with charts

Options:
  -h, --help                    Show this help message and exit
  --features <FEATURES>         Comma-separated list of features (Check example section)
  --binary <BINARY_PATH>        Path to Q-CLI binary. If not provided, script will use default "q" (Q-CLI installed on the system)
  --quiet                       Quiet mode - reduces console output by hiding system info, cargo commands, and test details while preserving complete data in generated reports
  --list-features               List all available features (Check example section)
  --json-to-html <JSON_PATH>    Convert JSON report (previously generated by running test) to HTML (Check example section)

Syntax:
  run_tests.py [-h] [--features <FEATURES>] [--binary <BINARY_PATH>] [--quiet] [--list-features] [--json-to-html <JSON_PATH>]

Usage:
  run_tests.py [options]                           # Run tests with default settings
  run_tests.py --features <FEATURES>               # Run specific features
  run_tests.py --list-features                     # List available features
  run_tests.py --json-to-html <JSON_PATH>          # Convert JSON report to HTML (provide JSON file path)

options:
  -h, --help            show this help message and exit
  --list-features       List all available features
  --json-to-html JSON_PATH
                        Convert JSON report to HTML (provide JSON file path)
  --features FEATURES   Comma-separated list of features
  --binary BINARY       Path to Q CLI binary
  --quiet               Quiet mode

Examples:
  # Basic usage
  run_tests.py                                     # Run all tests with default sanity suite
  run_tests.py --features usage                    # Run usage tests with default sanity suite
  run_tests.py --features "usage,agent"            # Run usage+agent tests with default sanity suite
  
  # Test suites
  run_tests.py --features sanity                   # Run all tests with sanity suite
  run_tests.py --features regression               # Run all tests with regression suite
  run_tests.py --features "usage,regression"       # Run usage tests with regression suite

  
  # Multiple features (different ways)
  run_tests.py --features "usage,agent,context"    # Comma-separated features with default sanity suite
  run_tests.py --features usage --features agent   # Multiple --features flags with default sanity suite
  run_tests.py --features core_session             # Run grouped feature (includes help,quit,clear) with default sanity suite
  
  # Binary and output options
  run_tests.py --binary /path/to/q --features usage   # Executes the usage tests on provided q-cli binary instead of installed 
  run_tests.py --quiet --features sanity              # Executes the tests in quiet mode
  
  # Utility commands
  run_tests.py --list-features                     # List all available features
  run_tests.py --json-to-html report.json          # Convert JSON report (previously generated by running test) to HTML
  
  # Advanced examples
  run_tests.py --features "core_session,regression" --binary ./target/release/q
  run_tests.py --features "agent,mcp,sanity" --quiet
```

### **Individual Category Testing Using Cargo**
```bash
# Test specific categories using cargo directly
cargo test --tests --features "core_session" -- --nocapture
cargo test --tests --features "agent" -- --nocapture
cargo test --tests --features "ai_prompts" -- --nocapture
```

## ✅ Comprehensive Test Coverage

### **Commands Tested (122+ tests)**
- **Agent Commands** (8 tests): `/agent list`, `/agent create`, `/agent help`, etc.
- **AI Prompts** (5 tests): "What is AWS?", "Hello" prompts
- **Context Commands** (10 tests): `/context show`, `/context add`, `/context help`, etc.
- **Core Session** (3 tests): `/help`, `/quit`, `/clear`
- **Integration Commands** (21 tests): `/subscribe`, `/hooks`, `/editor` help commands
- **MCP Commands** (18 tests): `/mcp`, `/mcp --help`
- **Model Commands** (3 tests): `/model`, `/model --help`
- **Q Subcommands** (15 tests): q chat, q debug, q doctor, etc.
- **Save/Load Commands** (10 tests): `/save`, `/load`, help commands
- **Session Management** (14 tests): `/compact`, `/usage`, help commands
- **Todos Commands**: todos command
- **Tools Commands** (15 tests): `/tools`, tool management commands

### **AI Prompts Tested**  
- "What is AWS?" - Technical explanation with verification
- "Hello" - Basic greeting response

### **Verification Includes**
- **Content verification**: Specific text and sections present
- **Real-time feedback**: Per-test pass/fail status

## 🎯 Success Metrics

- **122 Total Tests** across 12 functional categories ✅
- **Real-time feedback** with per-test results ✅
- **Categorized organization** for better reporting ✅
- **Configurable execution** for faster iteration ✅
- **Comprehensive coverage** of all Q CLI commands ✅

## 🔧 Integration with Workspace

This E2E test framework is designed to work with the Q CLI workspace:

- **Default binary**: Uses system `q` command (from PATH)
- **Workspace integration**: Can test the workspace build
- **CI/CD integration**: Currently blocked due to Q CLI authentication issues
- **Custom binary support**: Test different builds as needed

## 🔧 Extending

### **Adding New Tests**

1. **Create test file** in `tests/` directory
2. **Add feature attribute** to categorize the test:
   ```rust
   /// Brief description of what the test does
   /// More detailed description of verification steps and expected behavior
   #[test]
   #[cfg(all(feature = "category_name", feature = "sanity"))]
   fn test_new_command() -> Result<(), Box<dyn std::error::Error>> {
       println!("\n🔍 Testing /new command... | Description: Tests the <code>/new</code> command to verify functionality and expected behavior");
       
       let session = get_chat_session();
       let mut chat = session.lock().unwrap();
       
       let response = chat.execute_command("/new")?;
       
       println!("📝 FULL OUTPUT:");
       println!("{}", response);
       println!("📝 END OUTPUT");
       
       // Verify response content
       assert!(response.contains("expected_text"), "Missing expected content");
       println!("✅ Command executed successfully!");
       
       Ok(())
   }
   ```
3. **Update category configuration** in `run_tests.py` if needed

### **Adding New Categories**

1. **Add feature** to `Cargo.toml`:
   ```toml
   [features]
   core_session = ["help", "quit", "clear"]
   help = []
   quit = []
   clear = []
   tools = []
   agent = []
   context = []
   save_load = []
   model = []
   session_mgmt = ["compact", "usage"]
   compact = []
   usage = []
   integration = ["subscribe", "hooks", "editor", "issue_reporting"]
   subscribe = []
   hooks = []
   editor = []
   issue_reporting = []
   mcp = []
   ai_prompts = []
   q_subcommand = []
   regression = []
   sanity = []
   ```
**Python script will automatically pick the features from toml file.
### **Test Patterns**

- **For Commands**: Use `execute_command()` method with expectrl
- **For AI Prompts**: Use `send_prompt()` method with direct streams
- **Always**: Print full output first, then verify content
- **Pattern**: Start session → Execute → Verify → Quit

This framework provides comprehensive, categorized E2E testing for the Q CLI with real-time feedback and flexible execution options.
