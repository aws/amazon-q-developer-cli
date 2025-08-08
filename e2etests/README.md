# Q CLI E2E Test Framework

This test framework provides comprehensive end-to-end testing capabilities for Amazon Q CLI using a hybrid approach with expectrl.

## 🏗️ Architecture

### **Hybrid Approach**
- **expectrl (PTY)**: For interactive commands (`/help`, `/tools`, `/quit`, etc.)
- **Direct Process Streams**: For AI prompts ("What is AWS?", "Hello", etc.)

### **Why Hybrid?**
- **Commands** write to PTY stream → expectrl captures perfectly ✅
- **AI responses** write to stdout → direct streams capture properly ✅

## 📁 Core Files

### **`tests/q_chat_helper.rs`**
Base helper class providing:
- `QChatSession::new()` - Start Q Chat session
- `execute_command(cmd)` - Execute commands using expectrl + carriage return (`0x0D`)
- `send_prompt(prompt)` - Send AI prompts using direct process streams
- `quit()` - Clean session termination

### **Test Files**
- **`test_help_command.rs`** - Tests `/help` command with full content verification
- **`test_tools_command.rs`** - Tests `/tools` command with tool listing verification  
- **`test_ai_prompt.rs`** - Tests AI prompts with response content verification

## 🚀 Usage

### **Using Default System Q CLI Binary**
```bash
cd e2etests
./run_tests.sh
```

### **Using Custom Q CLI Builds**
Set the `Q_CLI_PATH` environment variable or pass the path directly:

```bash
# Test with workspace build
./run_tests.sh ../target/release/chat_cli

# Test with custom build
export Q_CLI_PATH="/path/to/your/custom/chat_cli"
cargo test --test test_help_command -- --nocapture

# Or use the convenience script
./run_tests.sh /path/to/your/custom/chat_cli test_help_command
./run_tests.sh /path/to/your/custom/chat_cli  # Run all tests
```

### **Run Individual Tests**
```bash
# Test help command
cargo test --test test_help_command -- --nocapture

# Test AI prompts  
cargo test --test test_ai_prompt test_what_is_aws_prompt -- --nocapture

# Test tools command
cargo test --test test_tools_command -- --nocapture
```

### **Run All Tests**
```bash
cargo test --test test_help_command --test test_tools_command --test test_ai_prompt -- --nocapture
```

## ✅ Test Coverage

### **Commands Tested**
- `/help` - Full help content with Commands, Options, MCP, Tips sections
- `/tools` - Tool listing with Built-in and MCP tools, permission status

### **AI Prompts Tested**  
- "What is AWS?" - Technical explanation with verification
- "Hello" - Basic greeting response

### **Verification Includes**
- **Content verification**: Specific text and sections present
- **Response quality**: Technical terms, appropriate length
- **Full output capture**: Complete interaction including UI elements

## 🎯 Success Metrics

- **Help Command**: 2343+ bytes, all sections verified ✅
- **Tools Command**: 3355+ bytes, all tools and permissions verified ✅  
- **AI Prompts**: 5000+ bytes, complete technical responses verified ✅

## 🔧 Integration with Workspace

This E2E test framework is designed to work with the Q CLI workspace:

- **Default binary**: Uses system `q` command (from PATH)
- **Workspace integration**: Can test the workspace build with `./run_tests.sh ../target/release/chat_cli`
- **CI/CD ready**: Can be integrated into build pipelines
- **Custom binary support**: Test different builds as needed

## 🔧 Extending

To add new tests:

1. **For Commands**: Use `execute_command()` method with expectrl
2. **For AI Prompts**: Use `send_prompt()` method with direct streams
3. **Always**: Print full output first, then verify content
4. **Pattern**: Start session → Execute → Verify → Quit

This framework provides comprehensive E2E testing for the Q CLI with both interactive commands and AI functionality.
