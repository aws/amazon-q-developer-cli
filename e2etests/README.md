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
Tests are organized into 9 functional categories using Rust features:

1. **Core Session Commands** (4 tests) - `core_session`
   - `/help`, `/tools`, `/quit`, `/clear`

2. **Agent Commands** (8 tests) - `agent`
   - `/agent list`, `/agent create`, `/agent help`, etc.

3. **Context Commands** (5 tests) - `context`
   - `/context show`, `/context add`, `/context help`, etc.

4. **Save/Load Commands** (4 tests) - `save_load`
   - `/save`, `/load`, help commands

5. **Model Commands** (2 tests) - `model`
   - `/model`, `/model --help`

6. **Session Management Commands** (4 tests) - `session_mgmt`
   - `/compact`, `/usage`, help commands

7. **Integration Commands** (4 tests) - `integration`
   - `/subscribe`, `/hooks`, `/editor` help commands

8. **MCP Commands** (2 tests) - `mcp`
   - `/mcp`, `/mcp --help`

9. **AI Prompts** (2 tests) - `ai_prompts`
   - "What is AWS?", "Hello" prompts

## 📁 Core Files

### **`tests/q_chat_helper.rs`**
Base helper class providing:
- `QChatSession::new()` - Start Q Chat session
- `execute_command(cmd)` - Execute commands using expectrl + carriage return (`0x0D`)
- `send_prompt(prompt)` - Send AI prompts using direct process streams
- `quit()` - Clean session termination

### **`run_simple_categorized.sh`**
Advanced categorized test runner with:
- **Real-time per-test feedback** - Shows ✅/❌ as each test completes
- **Category organization** - Groups tests by functional area
- **Configurable categories** - Enable/disable categories for faster iteration
- **Quiet and verbose modes** - Control output detail level
- **Final summary reporting** - Shows passed/failed categories

## 🚀 Usage

### **Categorized Test Runner (Recommended)**

```bash
# Run all categories with real-time feedback
./run_simple_categorized.sh

# Quiet mode - faster, less verbose
./run_simple_categorized.sh --quiet

# With custom Q CLI binary
./run_simple_categorized.sh /path/to/custom/q --quiet
```

**Example Output:**
```
🧪 Core Session Commands
---------------------
📋 Tests in this category:
   • test_help_command
   • test_tools_command
   • test_quit_command
   • test_clear_command

🔄 Running tests...
   ✅ test_help_command
   ✅ test_tools_command
   ✅ test_quit_command
   ✅ test_clear_command
✅ Core Session Commands completed successfully

🎯 FINAL SUMMARY
================================
✅ Categories Passed: 9
❌ Categories Failed: 0
📊 Total Categories: 9
🎉 All categories passed!
```

### **Category Configuration**
Edit the top of `run_simple_categorized.sh` to enable/disable categories:

```bash
# Enable/disable categories for faster iteration
RUN_CORE_SESSION=true
RUN_AGENT=false          # Skip agent tests
RUN_CONTEXT=true
RUN_AI_PROMPTS=true
# etc...
```

### **Individual Category Testing**
```bash
# Test specific categories
cargo test --tests --features "core_session" -- --nocapture
cargo test --tests --features "agent" -- --nocapture
cargo test --tests --features "ai_prompts" -- --nocapture
```

### **Legacy Test Runner**
```bash
# Original test runner (still available)
./run_tests.sh
./run_tests.sh ../target/release/chat_cli
```

## ✅ Comprehensive Test Coverage

### **Commands Tested (32+ tests)**
- **Core Session**: `/help`, `/tools`, `/quit`, `/clear`
- **Agent Management**: `/agent list`, `/agent create`, `/agent help`, etc.
- **Context Management**: `/context show`, `/context add`, `/context help`, etc.
- **Save/Load**: `/save`, `/load`, help commands
- **Model Selection**: `/model`, `/model --help`
- **Session Management**: `/compact`, `/usage`, help commands
- **Integration**: `/subscribe`, `/hooks`, `/editor` help commands
- **MCP**: `/mcp`, `/mcp --help`

### **AI Prompts Tested**  
- "What is AWS?" - Technical explanation with verification
- "Hello" - Basic greeting response

### **Verification Includes**
- **Content verification**: Specific text and sections present
- **Response quality**: Technical terms, appropriate length
- **Full output capture**: Complete interaction including UI elements
- **Real-time feedback**: Per-test pass/fail status

## 🎯 Success Metrics

- **34 Total Tests** across 9 functional categories ✅
- **Real-time feedback** with per-test results ✅
- **Categorized organization** for better reporting ✅
- **Configurable execution** for faster iteration ✅
- **Comprehensive coverage** of all Q CLI commands ✅

## 🔧 Integration with Workspace

This E2E test framework is designed to work with the Q CLI workspace:

- **Default binary**: Uses system `q` command (from PATH)
- **Workspace integration**: Can test the workspace build
- **CI/CD ready**: Can be integrated into build pipelines with categorized reporting
- **Custom binary support**: Test different builds as needed

## 🔧 Extending

### **Adding New Tests**

1. **Create test file** in `tests/` directory
2. **Add feature attribute** to categorize the test:
   ```rust
   #[test]
   #[cfg(feature = "category_name")]
   fn test_new_command() -> Result<(), Box<dyn std::error::Error>> {
       // Test implementation
   }
   ```
3. **Update category configuration** in `run_simple_categorized.sh` if needed

### **Adding New Categories**

1. **Add feature** to `Cargo.toml`:
   ```toml
   [features]
   new_category = []
   ```
2. **Add category** to `run_simple_categorized.sh`:
   ```bash
   RUN_NEW_CATEGORY=true
   # ...
   if [ "$RUN_NEW_CATEGORY" = true ]; then
       run_category "new_category" "New Category Commands"
   fi
   ```

### **Test Patterns**

- **For Commands**: Use `execute_command()` method with expectrl
- **For AI Prompts**: Use `send_prompt()` method with direct streams
- **Always**: Print full output first, then verify content
- **Pattern**: Start session → Execute → Verify → Quit

This framework provides comprehensive, categorized E2E testing for the Q CLI with real-time feedback and flexible execution options.
