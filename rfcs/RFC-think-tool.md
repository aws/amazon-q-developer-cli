- Feature Name: think-tool
- Start Date: 2025-03-27

# Summary

[summary]: #summary

Add a "think" tool that allows the model to reason through complex problems during response generation. This tool provides a dedicated space for the model to process information from tool call results, navigate complex decision trees, and improve the quality of responses in multi-step scenarios.

# Motivation

[motivation]: #motivation

When handling complex tasks, the model often needs to reason through multiple steps, analyze information from previous tool calls, or plan a sequence of operations. Currently, this reasoning happens implicitly within the model's response generation, which can lead to:

1. Incomplete reasoning due to token limitations
2. Difficulty tracking state across multiple tool calls
3. Mixing reasoning with user-facing content
4. Reduced clarity in complex decision-making processes

The "think" tool provides a dedicated space for this reasoning, allowing the model to:

1. Process information from tool call results
2. Navigate complex decision trees
3. Plan multi-step operations
4. Maintain state across interactions
5. Improve the quality of responses in complex scenarios

# Guide-level explanation

[guide-level-explanation]: #guide-level-explanation

The "think" tool gives the model a space to work through complex problems step by step without showing this process to the user. Think of it as the model's scratch pad or internal monologue.

When implementing features that use the "think" tool, you'll see code like this:

```rust
// Example of how a developer would use the think tool in their code
let thought = "I need to analyze the file structure before suggesting changes";
think_tool.invoke(thought);
```

For users, this feature is invisible by default but can be enabled:

```shell
# Enable the thinking feature
q settings enable_thinking true
```

### Implementation as Rules

These usage guidelines will be implemented as rules within the system rather than being added to the AmazonQ.md development guidelines. This ensures that the model will follow these guidelines automatically when using the think tool.

## Flow Diagram

The following diagram illustrates how the think tool works with the feature flag:

```
┌─────────────────┐     ┌───────────────────┐     ┌───────────────────┐
│                 │     │                   │     │                   │
│  load_tools()   │     │ Is enable_thinking│     │ "think" tool      │
│  function runs  ├────►│ setting enabled?  │ No ►│ removed from      │
│                 │     │                   │     │ tool list         │
│                 │     │                   │     │                   │
└────────┬────────┘     └─────────┬─────────┘     └───────────────────┘
         │                        │ Yes
         ▼                        ▼
┌─────────────────┐     ┌───────────────────┐
│                 │     │                   │
│  "think" tool   │     │ Tool included     │
│  included in    ├────►│ in model's        │
│  tool list      │     │ available tools   │
│                 │     │                   │
└────────┬────────┘     └─────────┬─────────┘
         │                        │
         ▼                        ▼
┌─────────────────┐     ┌───────────────────┐
│                 │     │                   │
│  Model uses     │     │ Model continues   │
│  "think" tool   ├────►│ with response     │
│                 │     │ generation        │
│                 │     │                   │
└─────────────────┘     └───────────────────┘
```

# Reference-level explanation

[reference-level-explanation]: #reference-level-explanation

The "think" tool accepts a single parameter:

```json
{
  "thought": "string"
}
```

The implementation consists of the following key components:

1. A new `think.rs` file in the `tools` module:
   ```rust
   use std::io::Write;
   use eyre::Result;
   use fig_settings::settings;
   use serde::Deserialize;
   
   use super::{
       InvokeOutput,
       OutputKind,
   };
   
   #[derive(Debug, Clone, Deserialize)]
   pub struct Think {
       /// The thought content that the model wants to process
       pub thought: String,
   }
   
   impl Think {
       /// Checks if the thinking feature is enabled in settings
       fn is_enabled() -> bool {
           // Default to disabled if setting doesn't exist or can't be read
           settings::get_value("enable_thinking")
               .map(|val| val.and_then(|v| v.as_bool()).unwrap_or(false))
               .unwrap_or(false)
       }
       
       /// Checks if the think tool should be included in the tool list
       pub fn should_include_in_tools() -> bool {
           Self::is_enabled()
       }
       
       /// Invokes the think tool
       pub async fn invoke(&self, _updates: &mut impl Write) -> Result<InvokeOutput> {
           // Only process non-empty thoughts if the feature is enabled
           if Self::is_enabled() && !self.thought.trim().is_empty() {
               // Log the thought for debugging purposes
               log::debug!("Model thought: {}", self.thought);
               
               // Return the thought as part of the output, but mark it as internal
               // This ensures it's added to conversation history but not displayed to the user
               return Ok(InvokeOutput {
                   output: OutputKind::Internal(self.thought.clone()),
               });
           }
           
           // If disabled or empty thought, return empty output
           Ok(InvokeOutput {
               output: OutputKind::Text(String::new()),
           })
       }
       
       /// Validates the thought - accepts empty thoughts
       pub async fn validate(&mut self, _ctx: &fig_os_shim::Context) -> Result<()> {
           // We accept empty thoughts - they'll just be ignored
           Ok(())
       }
   }
   ```

2. Modified `load_tools` function in `chat/mod.rs`:
   ```rust
   /// Returns all tools supported by Q chat.
   fn load_tools() -> Result<HashMap<String, ToolSpec>> {
       let mut tools: HashMap<String, ToolSpec> = serde_json::from_str(include_str!("tools/tool_index.json"))?;
       
       // Only include the think tool if the feature is enabled
       if !tools::think::Think::should_include_in_tools() {
           tools.remove("think");
       }
       
       Ok(tools)
   }
   ```

3. Added setting subcommand in `settings.rs`:
   ```rust
   #[derive(Debug, Subcommand, PartialEq, Eq)]
   pub enum SettingsSubcommands {
       // ... existing subcommands
       /// Enable the thinking tool (beta feature)
       EnableThinking {
           /// Enable or disable the thinking tool
           #[arg(default_value_t = true)]
           enable: bool,
       },
   }
   ```

4. Tool specification in `tool_index.json`:
   ```json
   "think": {
     "name": "think",
     "description": "A tool for the model to reason through complex problems. This is an internal tool that doesn't produce visible output to the user.",
     "input_schema": {
       "type": "object",
       "properties": {
         "thought": {
           "type": "string",
           "description": "The thought content that the model wants to process internally"
         }
       },
       "required": ["thought"]
     }
   }
   ```

5. Added a new `OutputKind::Internal` variant to handle thoughts that should be added to conversation history but not displayed to the user:
   ```rust
   #[non_exhaustive]
   #[derive(Debug)]
   pub enum OutputKind {
       Text(String),
       Json(serde_json::Value),
       /// Internal output that should be added to conversation history but not displayed to the user
       Internal(String),
   }
   ```

6. Updated the `From<InvokeOutput>` implementation to handle the new `Internal` variant:
   ```rust
   impl From<InvokeOutput> for ToolResultContentBlock {
       fn from(value: InvokeOutput) -> Self {
           match value.output {
               crate::cli::chat::tools::OutputKind::Text(text) => Self::Text(text),
               crate::cli::chat::tools::OutputKind::Json(value) => Self::Json(serde_value_to_document(value)),
               crate::cli::chat::tools::OutputKind::Internal(thought) => {
                   // For internal thoughts, we still add them to the conversation history
                   // but they won't be displayed to the user
                   Self::Text(thought)
               },
           }
       }
   }
   ```

   This implementation ensures that thoughts are properly added to the conversation state while remaining invisible to the user. When a thought is processed:
   
   1. The thought is converted to a `ToolResultContentBlock::Text`
   2. This block is added to the conversation history in `ConversationState`
   3. The model can reference this thought in future responses
   4. The thought is not displayed in the user interface


# Drawbacks

[drawbacks]: #drawbacks

- **Increased Complexity**: Adds another tool that developers need to understand
- **Performance Impact**: Additional tool calls might impact response generation time
- **Debugging Challenges**: Without visibility into thoughts, it might be harder to understand model reasoning

# Rationale and alternatives

[rationale-and-alternatives]: #rationale-and-alternatives

## Why this design?

This design is simple, lightweight, and achieves the goal of providing a dedicated space for model reasoning without significant changes to the existing architecture. The key benefits are:

1. **Completely Optional**: The tool is only included when explicitly enabled
2. **Minimal Overhead**: No impact on performance when disabled
3. **Simple Implementation**: Uses existing mechanisms for tool registration and invocation
4. **Transparent Logging**: Thoughts are logged to the debug log for analysis when needed

## Alternatives considered:

1. **Structured Thinking**: A more complex tool that accepts structured data for different types of reasoning. Rejected because it adds unnecessary complexity for the initial implementation.

2. **No Tool Approach**: Have the model reason within its response generation process. Rejected because it doesn't solve the token limitation and state tracking issues.

3. **Automatic Thinking**: Automatically insert thinking steps between tool calls. Rejected because it would be harder to control and might lead to unnecessary overhead.

4. **Always-Included Tool**: Include the tool but ignore its calls when disabled. Rejected because it unnecessarily increases the tool list size and might confuse the model.

## Impact of not doing this:

Without this feature, complex reasoning will continue to happen implicitly within response generation, leading to:
- Less transparent decision-making
- Potential reasoning errors due to token limitations
- Difficulty debugging complex model behaviors

# Unresolved questions

[unresolved-questions]: #unresolved-questions

- How will we measure the effectiveness of the "think" tool in improving response quality?
- Should thoughts be stored for later analysis or discarded after use?
- Are there privacy implications to logging model thoughts?
- Should we implement rate limiting to prevent excessive thinking?

# Future possibilities

[future-possibilities]: #future-possibilities

1. **Thought Templates**: Implement specialized thought templates for different domains:
   - Code analysis templates
   - HR data processing templates
   - Financial data reasoning templates
   - Security assessment templates
2. **Add Think Tool rules** : Usage Guidelines (This will be added to rules) :The Think Tool is an internal reasoning mechanism enabling the model to systematically approach complex tasks by logically breaking them down before responding or acting; use it specifically for multi-step problems requiring step-by-step dependencies, reasoning through multiple constraints, synthesizing results from previous tool calls, planning intricate sequences of actions, troubleshooting complex errors, or making decisions involving multiple trade-offs. Avoid using it for straightforward tasks, basic information retrieval, summaries, always clearly define the reasoning challenge, structure thoughts explicitly, consider multiple perspectives, and summarize key insights before important decisions or complex tool interactions.
3. **Plan Mode Integration** : This can be incorporated in Plan Mode.