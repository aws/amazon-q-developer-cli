- Feature Name: preprocessor_tools
- Start Date: 2025-03-31

# Summary

[summary]: #summary

This RFC introduces **Preprocessor Tools** in the Model Context Protocol (MCP). Preprocessor tools are executed deterministically for every prompt before the user input is processed by the language model (LLM). Their outputs are injected into the request context but do not persist in the conversation history. Preprocessor tools are not available as general tools and are excluded from the tool specifications sent to the LLM.

# Motivation

[motivation]: #motivation

Certain tools, such as a **RetrieveMemories** function in a personalization system, must be executed before every user prompt to provide necessary context to the LLM. Since these tools serve a preparatory function rather than an interactive one, they require a distinct designation within MCP.

By introducing a **preprocessor** flag in the MCP tool specification, we ensure deterministic execution of these tools while preserving the integrity of the tool interface. Additionally, this design maintains backward compatibility and provides a pathway for potential upstream adoption into the MCP project.

Another key benefit of preprocessors is **control over tool execution order**. By ensuring that preprocessors execute before the LLM receives a prompt, we can guarantee that their output is able to influence subsequent standard tool calls. If RetrieveMemories were a standard tool, it might execute later in the chain, preventing earlier tool calls from benefiting from its output. This could lead to suboptimal tool selections, as key user memory entries may have altered the request or influenced which tools should be invoked.

# Guide-level explanation

[guide-level-explanation]: #guide-level-explanation

### Introducing Preprocessor Tools

A **Preprocessor Tool** is a special category of MCP tool that runs before every user prompt. Unlike standard tools:

- They execute deterministically before the LLM receives the user's prompt.
- Their output is injected into the request context but does not persist in the conversation history.
- They are not accessible as general tools and do not appear in the LLM's toolset.

Example: Suppose we have a **RetrieveMemories** tool that fetches user-specific memories. This tool must execute on every prompt to provide personalization data to the LLM. By marking it as a preprocessor, we ensure it runs every time and its output is available to the model without the user having to call it explicitly.

### Example: RetrieveMemories Tool Specification

```json
{
  "name": "RetrieveMemories",
  "description": "Retrieves relevant user memories",
  "parameters": {
    "type": "object",
    "properties": {
      "query": {
        "type": "string",
        "description": "The user input to match against memories"
      }
    }
  },
  "preprocessor": true
}
```

# Reference-level explanation

[reference-level-explanation]: #reference-level-explanation

### MCP Tool Specification Update

A new `preprocessor` field is added to the MCP tool specification:

```json
{
  "name": "string",          // Unique identifier for the tool
  "description": "string",   // Optional human-readable description
  "inputSchema": {           // JSON Schema defining tool parameters
    "type": "object",
    "properties": { ... }    // Tool-specific parameters
  },
  "preprocessor": true       // New flag indicating preprocessor tool status
}
```

### Behavior of Preprocessor Tools

- **Deterministic Execution**: Preprocessor tools run for every prompt before it reaches the LLM.
- **Context Injection**: Their output is included in the request context but does not modify the conversation history.
- **Restricted Availability**: Preprocessors are not available as general tools and are omitted from the tool specs sent to the LLM.
- **Tool Execution Order Control**: Preprocessors guarantee that their output is available before the LLM decides which other tools to invoke.
- **Non-Disruptive Addition**: The introduction of this feature does not break existing MCP tool specifications.

# Drawbacks

[drawbacks]: #drawbacks

- **Latency**: Preprocessors introduce latency for prompts that do not require their output. However, in cases where a preprocessor would otherwise be called as a standard tool, this approach can reduce latency by eliminating an additional LLM call (if no other tools are involved).

# Rationale and alternatives

[rationale-and-alternatives]: #rationale-and-alternatives

- **Why this design?** The preprocessor flag ensures a clean, additive modification to MCP without disrupting existing tools.
- **Alternatives considered:**
  - Ensure 'always-execute' functionality by relying on the tool description: This is non-deterministic, and the reliability would depend on the wider context of the request. Also, there would be no control over the ordering of tool executions (e.g. if multiple tools have 'always-execute' tool descriptions).
  - Custom CLI integration/built-in tool: This would not harness MCP, and would not scale outside of the CLI.
- **Impact of not implementing:** Personalization and other pre-execution-dependent use cases would require workarounds.

# Unresolved questions

[unresolved-questions]: #unresolved-questions

- Should we provide an API for listing registered preprocessors?
- Should preprocessors support dependency ordering if multiple exist?
- Should there be logging/debugging tools specific to preprocessors?

# Future possibilities

[future-possibilities]: #future-possibilities

- **Upstream Contribution**: We may propose this feature for inclusion in the upstream MCP project.
- **Broader Applications**: Other potential use cases include real-time security validation, compliance checks, or dynamic user context augmentation.

This RFC provides a structured approach to introducing **Preprocessor Tools** in MCP, ensuring deterministic execution of essential context-providing functions while maintaining backwards compatibility.
