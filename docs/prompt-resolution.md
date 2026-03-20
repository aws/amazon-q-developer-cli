# Prompt Resolution

How `@prompt-name args` references are parsed, resolved, and delivered to the model in both the main agent and subagent paths.

## Shared Functions (`cli/chat/cli/prompts.rs`)

| Function | Purpose |
|----------|---------|
| `parse_prompt_reference(input)` | Parses `@name [args...]` → `(name, Option<Vec<String>>)` |
| `resolve_prompt_reference(name, args, os, tool_manager)` | Resolves name to `Vec<PromptMessage>` via file or MCP |
| `stringify_prompt_content(content)` | Converts one `PromptMessageContent` → `String` for model input |
| `prompt_messages_to_text(messages)` | Joins all messages via `stringify_prompt_content` |

## Main Agent Path

User types `@my-prompt arg1` in the chat input.

```mermaid
flowchart TD
    A["User types: @my-prompt arg1"] --> B["mod.rs: starts_with('@')"]
    B --> C["Collect known prompt names
    FilePrompts::get_available_names()
    + tool_manager.list_prompts()"]
    C --> D{"parse_prompt_reference()
    name in known_prompts?"}
    D -- No --> E["try_at_reference_expansion()
    (file/dir @ references)"]
    D -- Yes --> F["PromptsSubcommand::Get
    {name, arguments, orig_input}"]
    F --> G["execute_get()"]

    G --> H{"FilePrompts::new(name)
    .load_existing()?"}
    H -- "Found file" --> I["display_file_prompt_content()
    (show to user on stderr)"]
    I --> J["Wrap in PromptMessage::Text
    push to pending_prompts"]

    H -- "No file" --> K["tool_manager.get_prompt(name, args)
    (MCP resolution)"]
    K -- Ok --> L["display_prompt_content()
    (show to user on stderr)"]
    L --> M["Push Vec&lt;PromptMessage&gt;
    to pending_prompts"]
    K -- Err --> N["Show error to user
    return PromptUser"]

    J --> O["return HandleInput"]
    M --> O

    O --> P["handle_input()"]
    P --> Q["pending_prompts.drain()"]
    Q --> R["conversation.append_prompts()"]
    R --> S["stringify_prompt_content()
    per message"]
    S --> T["Build UserMessage / AssistantMessage
    append to conversation history"]
    T --> U["set_next_user_message()"]
    U --> V["Send to model"]
```

## Subagent Path

The main agent's LLM calls `use_subagent` with a query like `@agent-sop:code-assist task_description="fix bug"`.

```mermaid
flowchart TD
    A["LLM calls use_subagent with
    query: '@agent-sop:code-assist task=...'"] --> B["UseSubagent::invoke()
    use_subagent.rs"]

    B --> C["resolve_prompt_in_query()
    for each subagent query"]

    C --> D{"parse_prompt_reference()
    starts with @?"}
    D -- No --> E["Return original query unchanged"]
    D -- Yes --> F["resolve_prompt_reference()
    (file then MCP, using parent's tool_manager)"]

    F -- Ok --> G["prompt_messages_to_text()"]
    G --> H["stringify_prompt_content()
    per message"]
    H --> I["Joined text string"]

    F -- Err --> J["Log warning, return original query"]

    I --> K["resolved_queries[id]"]
    E --> K
    J --> K

    K --> L["invoke_subagent.as_subagent()
    with query_override"]
    L --> M["Subagent.query = resolved text"]
    M --> N["agent.send_prompt(SendPromptArgs {
    content: [ContentChunk::Text(query)]
    })"]
    N --> O["Subagent model processes
    resolved prompt content"]
```

## Known Gaps (Subagent Path)

- **No known-prompt check**: The main agent only resolves `@name` if `name` is in the set of known file + MCP prompts. The subagent path attempts resolution for any `@`-prefixed query, which could collide with `@file-references` or fail on unknown names (falls back to raw query).
- **No conflict detection**: When both a file prompt and an MCP prompt share the same name, the main agent warns the user. The subagent path silently uses the file prompt.
- **Text only**: Subagents receive `ContentChunk::Text`. Non-text `PromptMessageContent` (Image, Resource, ResourceLink) is stringified via `stringify_prompt_content` but ultimately flattened to text.

| Aspect | Main Agent | Subagent |
|--------|-----------|----------|
| Input source | User keyboard input | LLM tool call parameter |
| Known-prompt check | Yes (only resolves known names) | No (attempts resolution for any `@` prefix) |
| Resolution context | Own `tool_manager` | Parent agent's `tool_manager` |
| Content delivery | `Vec<PromptMessage>` → `pending_prompts` → `append_prompts()` | `Vec<PromptMessage>` → `prompt_messages_to_text()` → `&str` |
| Multimodal support | Full (via `append_prompts`) | Text only (via `ContentChunk::Text`) |
| Error handling | Rich UI feedback to user | Log warning, fall back to raw query |
| Display to user | Yes (`display_prompt_content`) | No |
| Conflict detection | Yes (file vs MCP warning) | No (file wins silently) |
