# Agent Environment Reorganization

This directory contains the planning documents for reorganizing the prototype from `crates/chat-exp/main.rs` into a maintainable structure under `crates/chat-cli/src/agent_env/`.

## Documents

### 1. [reorganization-plan.md](reorganization-plan.md)
**Main planning document** covering:
- Overall goals and objectives
- Component analysis (core vs demo)
- Directory structure
- Module dependencies
- Integration with ChatArgs.execute()
- Migration steps
- Design principles
- Future evolution path
- Testing strategy
- Success criteria

### 2. [component-extraction-guide.md](component-extraction-guide.md)
**Detailed extraction instructions** for each component:
- Line-by-line extraction guidance
- Module structure templates
- Dependency information
- Key implementation points
- Extraction order
- Common issues and solutions

## Quick Start

1. **Read** `reorganization-plan.md` to understand the overall architecture
2. **Follow** `component-extraction-guide.md` for step-by-step extraction
3. **Test** with `cargo run --bin chat_cli`

## Key Concepts

### Core Components (Production-Ready)
- **Continuations**: Job completion callback system with latched state
- **Model Provider**: LLM communication abstraction with streaming
- **Worker**: Agent configuration with state management
- **Worker Task**: Interface for executable work units
- **Worker Job**: Running job combining worker, task, and execution
- **Session**: Central orchestrator for workers and jobs
- **Worker Interface**: Communication contract between workers and UI

### Demo Components (Temporary)
- **WorkerProtoLoop**: Demo task showing complete agent flow
- **CliInterface**: Console-based UI implementation
- **Initialization**: Demo-specific setup functions

## Architecture Goals

1. **Multiple Concurrent Agents**: Enable parallel execution of independent agents
2. **Flexible Configuration**: Each agent can have unique settings, tools, and behavior
3. **Clean Abstractions**: Separate core logic from demo implementations
4. **Gradual Evolution**: Replace demo code with production incrementally
5. **Extensibility**: Easy to add new tasks, providers, and UI implementations

## Integration Point

The architecture integrates directly into `ChatArgs.execute()` as a production step. The demo components (WorkerProtoLoop, CliInterface) will be replaced with production implementations in the next iteration.

## Next Steps

1. Create directory structure under `crates/chat-cli/src/agent_env/`
2. Extract core components following the extraction guide
3. Extract demo components
4. Integrate with ChatArgs.execute()
5. Test and verify functionality
6. Begin replacing demo components with production code
