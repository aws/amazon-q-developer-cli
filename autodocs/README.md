# Kiro CLI Documentation

Comprehensive documentation for all Kiro CLI user-facing features.

## Structure

```
autodocs/
├── docs/              # Documentation content
│   ├── tools/         # Built-in tools
│   ├── slash-commands/ # In-chat commands
│   ├── commands/      # CLI commands
│   ├── settings/      # Configuration options
│   └── features/      # Major features
└── meta/              # Metadata and tooling
    ├── feature-inventory.json
    ├── doc-index.json
    ├── plans/
    └── scripts/
```

## Documentation Categories

### tools/
**What**: Built-in tools that agents can invoke  
**Examples**: fs_read, fs_write, execute_bash, grep, code  
**When to use**: Documenting tools available in agent configurations

**Content includes**:
- Tool parameters (JSON format)
- Operation modes
- Configuration options (toolsSettings)
- Permission model
- Examples with JSON payloads
- Troubleshooting

**Invocation note**: All tool docs must include a note in the Overview section clarifying that the tool is invoked by the AI assistant, not by users directly. Example: "This tool is used by the AI assistant to fulfill your requests. You don't invoke it directly - simply ask questions naturally."

### slash-commands/
**What**: In-chat commands starting with `/`  
**Examples**: /save, /load, /agent, /context, /tools  
**When to use**: Documenting commands users type in chat

**Content includes**:
- Command syntax
- Subcommands
- Options/flags
- Interactive behavior
- Examples with chat interactions
- Troubleshooting

### commands/
**What**: CLI commands run from terminal  
**Examples**: kiro-cli chat, kiro-cli agent, kiro-cli settings  
**When to use**: Documenting terminal commands

**Content includes**:
- Command-line syntax
- Flags and options
- Subcommands
- Output formats
- Examples with terminal commands
- Troubleshooting

### settings/
**What**: Configuration options  
**Examples**: chat.enableTangentMode, chat.defaultAgent  
**When to use**: Documenting individual settings

**Content includes**:
- Setting name and type
- Default value
- How to get/set/reset
- What it controls
- Examples
- Related features

### features/
**What**: Major features with complex behavior  
**Examples**: Tangent Mode, Hooks System, Agent Configuration, Knowledge Management  
**When to use**: When feature needs comprehensive explanation beyond single tool/command

**Content includes**:
- Feature overview and architecture
- How it works (big picture)
- Configuration across multiple components
- Workflows and best practices
- Integration points
- Comprehensive examples
- Troubleshooting

## Metadata Format

Every doc includes YAML frontmatter:

```yaml
---
doc_meta:
  # Display/Navigation
  title: feature-name
  description: One-line description (max 120 chars)
  category: tool|slash_command|command|setting|feature
  keywords: [keyword1, keyword2, ...]
  related: [related-doc-1, related-doc-2]
  
  # Maintenance
  validated: YYYY-MM-DD
  commit: <git-hash>
  status: validated|draft|outdated
  testable_headless: true|false
---
```

### Metadata Fields

**title**: Feature name as users know it
- Tools: `fs_read`, `grep`, `code`
- Slash commands: `/save`, `/agent`, `/context`
- CLI commands: `kiro-cli chat`, `kiro-cli agent`
- Settings: `chat.enableTangentMode`, `chat.defaultAgent`
- Features: `Tangent Mode`, `Agent Configuration`

**description**: One-line summary (max 120 chars)
- Used in TOC generation
- Used in search results
- Should be clear and concise
- Focus on what it does, not how

**category**: Document type
- `tool` - Built-in tools
- `slash_command` - In-chat commands
- `command` - CLI commands
- `setting` - Configuration options
- `feature` - Major features

**keywords**: Search terms (array)
- Include variations and synonyms
- Include common misspellings
- Used for search and discovery

**related**: Related docs (array)
- Use doc filename without extension
- Links to related features
- Enables navigation

**validated**: Last verification date (YYYY-MM-DD)
- When doc was last checked against source code
- Helps identify stale docs

**commit**: Git commit hash
- Commit when doc was created/updated
- Links doc to code version

**status**: Current state
- `validated` - Verified against source code
- `draft` - Needs review
- `outdated` - Code changed, needs update

**testable_headless**: Can examples be auto-tested?
- `true` - Examples can run in `--no-interactive` mode
- `false` - Requires user interaction

## When to Create New Documentation

### New Tool
Create in `tools/` if:
- It's invoked by agents via tool calls
- Has JSON parameter format
- Can be configured in toolsSettings

### New Slash Command
Create in `slash-commands/` if:
- Users type it in chat with `/`
- It's an in-session command
- Has interactive behavior

### New CLI Command
Create in `commands/` if:
- Run from terminal
- Has command-line flags
- Part of `kiro-cli <command>` syntax

### New Setting
Create in `settings/` if:
- It's a configuration option
- Set via `kiro-cli settings`
- Controls behavior

### New Feature
Create in `features/` if:
- It's a major capability
- Spans multiple components
- Needs comprehensive explanation
- Has complex workflows
- Requires onboarding

## Documentation Standards

### Minimum Requirements
- 50+ lines for user-facing features
- 4+ sections (##)
- Examples section with 3+ code blocks
- Troubleshooting (where applicable)
- Related features section

### Required Sections
1. **Overview** - What it is and when to use it
2. **Usage** - How to use it (syntax, parameters)
3. **Examples** - Real-world usage with expected outputs
4. **Troubleshooting** - Common issues and solutions (if applicable)
5. **Related** - Links to related features
6. **Limitations** - Known constraints
7. **Technical Details** - Implementation notes

### Writing Guidelines
- **Read source code first** - Never write without understanding implementation
- **Verify accuracy** - Ensure docs match actual behavior
- **Use real outputs** - Test commands and show actual output
- **Be specific** - Avoid generic placeholders
- **Focus on users** - Document user-facing functionality, not internals
- **Keep current** - Use generic placeholders for backend-provided values (model IDs, etc.)

## Tooling

### Build Index
```bash
python3 autodocs/meta/scripts/build-doc-index.py
```

Generates `autodocs/meta/doc-index.json` with searchable metadata.

### Analyze Quality
```bash
python3 autodocs/meta/scripts/analyze-doc-quality.py
```

Checks all docs for quality issues. Target: 90+ average score.

### Verify Accuracy
```bash
python3 autodocs/meta/scripts/verify-doc-accuracy.py
```

Compares docs against source code for missing features.

## Maintenance Workflow

### After Code Changes
1. Identify affected features
2. Read updated source code
3. Update relevant docs
4. Verify examples still work
5. Update `validated` date and `commit` hash
6. Run quality analysis
7. Rebuild index

### Adding New Feature
1. Add to `feature-inventory.json`
2. Determine category (tool/command/setting/feature)
3. Read source code completely
4. Create doc in appropriate folder
5. Include all required sections
6. Add proper metadata
7. Run quality analysis
8. Rebuild index

### Quarterly Review
1. Check for outdated docs (old `validated` dates)
2. Compare `commit` hash with current code
3. Update docs for changed features
4. Remove docs for removed features
5. Run full quality analysis

## Quality Metrics

**Target**: 90+ average score, 90%+ Grade A

**Current**: Run `python3 autodocs/meta/scripts/analyze-doc-quality.py`

**Grading**:
- A (90-100): Excellent
- B (80-89): Good
- C (70-79): Acceptable
- D (60-69): Needs improvement
- F (0-59): Inadequate

## Integration

### Introspect Tool
Documentation can be embedded in introspect tool for LLM consumption.

**Smart loading** (future):
- Load `doc-index.json`
- Search by keywords
- Load only relevant docs (saves context)

### Documentation Website
Generate static site from docs using index for navigation.

### CI/CD
```yaml
- name: Validate documentation
  run: python3 autodocs/meta/scripts/analyze-doc-quality.py
```

## Related Files

- `feature-inventory.json` - Catalog of all 73+ features
- `doc-index.json` - Searchable metadata index
- `plans/` - Phase 1 & 2 documentation plans
- `scripts/` - Tooling for index generation and quality analysis
