# Language Symbol Configuration

This JSON file defines how to extract symbols, function calls, and imports from AST nodes. It maps tree-sitter node kinds to symbol types and specifies which child node contains the symbol name.

## Purpose

When `TreeSitterSymbolService` analyzes code, it uses this config to:
1. **symbols** - Identify symbol definitions (functions, classes, etc.) and extract their names
2. **calls** - Track function/method invocations for building call graphs
3. **imports** - Detect import statements for dependency analysis

## File Location

`languages/languages.json` - embedded at compile time via `include_str!`.

## Format

```json
{
  "language_name": {
    "extensions": ["ext1", "ext2"],
    "symbols": [
      {
        "node_kind": "tree_sitter_node_type",
        "symbol_type": "Function|Class|Method|Struct|Enum|Interface|Trait|Module|etc",
        "name_child": "child_node_kind_containing_name"
      }
    ],
    "calls": [
      {
        "node_kind": "call_expression",
        "name_child": "function"
      }
    ],
    "imports": [
      {"node_kind": "import_statement"}
    ]
  }
}
```

### Field Descriptions

| Field | Purpose | Example |
|-------|---------|---------|
| `symbols` | Define what AST nodes represent symbol definitions | `function_item` → Function in Rust |
| `calls` | Define what AST nodes represent function/method calls | `call_expression` in most languages |
| `imports` | Define what AST nodes represent import/include statements | `use_declaration` in Rust, `import_statement` in JS/TS |

## Adding a New Language

### Step 1: Check ast-grep supports the language

ast-grep supports these languages:
```
Bash, C, Cpp, CSharp, Css, Elixir, Go, Haskell, Html, Java, 
JavaScript, Json, Kotlin, Lua, Nix, Php, Python, Ruby, Rust, 
Scala, Solidity, Swift, Tsx, TypeScript, Yaml
```

### Step 2: Find node kinds using ast-grep

```bash
# Create a sample file with the construct you want to identify
echo 'fn my_func() {}' > /tmp/test.rs

# Use ast-grep to dump the AST structure
ast-grep run -p 'fn $F() {}' -l rust --debug-query=ast /tmp/test.rs
```

Output shows the AST:
```
Debug AST:
source_file (0,0)-(0,12)
  function_item (0,0)-(0,12)
    name: identifier (0,3)-(0,10)    <-- name_child is "identifier"
    ...
```

### Step 3: Verify against official tree-sitter grammar

Fetch the official `node-types.json` to confirm node kinds:

```bash
# Example for Rust
curl -s https://raw.githubusercontent.com/tree-sitter/tree-sitter-rust/master/src/node-types.json | jq '.[] | select(.type == "function_item")'
```

Common grammar repos:
| Language | Repository |
|----------|------------|
| Rust | `tree-sitter/tree-sitter-rust` |
| Python | `tree-sitter/tree-sitter-python` |
| TypeScript | `tree-sitter/tree-sitter-typescript` |
| Go | `tree-sitter/tree-sitter-go` |
| Java | `tree-sitter/tree-sitter-java` |
| C/C++ | `tree-sitter/tree-sitter-c` |
| Ruby | `tree-sitter/tree-sitter-ruby` |
| C# | `tree-sitter/tree-sitter-c-sharp` |
| Kotlin | `fwcd/tree-sitter-kotlin` |
| Swift | `alex-pinkus/tree-sitter-swift` |
| Scala | `tree-sitter/tree-sitter-scala` |
| PHP | `tree-sitter/tree-sitter-php` |

### Step 4: Add to languages.json

```json
{
  "newlang": {
    "extensions": ["nl"],
    "symbols": [
      {"node_kind": "function_definition", "symbol_type": "Function", "name_child": "identifier"},
      {"node_kind": "class_definition", "symbol_type": "Class", "name_child": "identifier"}
    ]
  }
}
```

### Step 5: Validate and test

```bash
# Validate JSON syntax
python3 -m json.tool languages/languages.json > /dev/null && echo "Valid JSON"

# Run tests
cargo test --package code-agent-sdk --lib tree_sitter
```

## Updating an Existing Language

### Step 1: Identify the missing symbol type

Example: Java is missing `enum_declaration`

### Step 2: Verify the node kind exists

```bash
echo 'enum Color { RED, GREEN }' > /tmp/Test.java
ast-grep run -p 'enum $E { $$$BODY }' -l java --debug-query=ast /tmp/Test.java
```

### Step 3: Add the symbol definition

```json
{"node_kind": "enum_declaration", "symbol_type": "Enum", "name_child": "identifier"}
```

## Handling Nested Names (C/C++ Example)

Some languages have nested name structures. For C/C++ functions:

```
function_definition
  └── declarator: function_declarator
        └── declarator: identifier  <-- name is nested!
```

The code uses recursive name finding, so `name_child: "identifier"` works even when nested.

## Validated Languages

These have been validated against official tree-sitter grammars:

| Language | Status | Notes |
|----------|--------|-------|
| Rust | ✅ | function_item, struct_item, enum_item, impl_item, trait_item, mod_item, const_item, static_item |
| Python | ✅ | function_definition, class_definition |
| TypeScript | ✅ | function_declaration, class_declaration, method_definition, interface_declaration, type_alias_declaration, enum_declaration |
| JavaScript | ✅ | Same as TypeScript minus type-specific nodes |
| Go | ✅ | function_declaration, method_declaration (field_identifier), type_declaration |
| Java | ✅ | method_declaration, class_declaration, interface_declaration, enum_declaration, record_declaration |
| C# | ✅ | method_declaration, class_declaration, interface_declaration, struct_declaration, enum_declaration, record_declaration |
| Kotlin | ✅ | function_declaration (simple_identifier), class_declaration (type_identifier), object_declaration |
| Ruby | ✅ | method, class, module |
| C/C++ | ✅ | function_definition (nested identifier), struct_specifier, enum_specifier, class_specifier |
| Swift | ✅ | function_declaration (simple_identifier), class_declaration, struct_declaration |
| Scala | ✅ | function_definition, class_definition, object_definition, trait_definition |
| PHP | ✅ | function_definition, class_declaration, method_declaration |
| Lua | ✅ | function_declaration |
| Bash | ✅ | function_definition (word) |
| Elixir | ⚠️ | Uses `call` nodes - limited support |

## Troubleshooting

### "No enclosing symbols found"
- The node_kind in config doesn't match the actual AST
- Use `ast-grep --debug-query=ast` to see actual node kinds

### Name extraction returns empty
- The name_child doesn't exist as a child of the node
- Check if name is nested (like C/C++) - recursive finder handles this
- Verify with `ast-grep --debug-query=ast`

### Pattern doesn't match
- ast-grep pattern syntax differs from tree-sitter queries
- Test patterns at https://ast-grep.github.io/playground.html

## Tree-sitter Parser Distribution

### How parsers are included

Tree-sitter grammars are compiled into the binary via the `ast-grep-language` crate. This crate bundles pre-compiled parsers for all supported languages as native code, not as separate files or dynamic libraries.

```
ast-grep-language (dependency)
└── Embeds compiled tree-sitter grammars for ~25 languages
```

### Binary size implications

Adding tree-sitter support increases binary size significantly:
- Each language grammar adds ~1-3 MB to the final binary
- All grammars are included regardless of which languages you use
- The `ast-grep-language` crate doesn't support selective grammar inclusion

### Adding new languages

New languages require:
1. The language must be supported by `ast-grep-language` (see list above)
2. Add the language config to `languages.json` with appropriate node kinds
3. No additional dependencies needed - grammar is already bundled

Languages NOT in ast-grep require upstream support first, or using tree-sitter grammars directly (which would add more complexity and binary size).
