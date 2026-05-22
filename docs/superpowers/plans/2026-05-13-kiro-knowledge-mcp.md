# kiro-knowledge-mcp Implementation Plan (Phase 1)

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Build a standalone MCP stdio server (`kiro-knowledge-mcp`) that exposes one tool — `search_kiro_knowledge` — backed by an Amazon Bedrock Knowledge Base. The agent in the kiro-help bot will use this to ground its answers in kiro-cli docs and GitHub issues.

**Architecture:** New Rust crate `crates/kiro-knowledge-mcp/`. The `rmcp` crate provides the MCP transport (matches the pattern in `crates/mock-mcp-server/`). A `Retriever` trait separates retrieval logic from MCP plumbing — production uses `BedrockRetriever` calling `bedrock-agent-runtime:Retrieve`; tests use a `StubRetriever` so CI doesn't need AWS. The binary takes a `--stub` flag for integration testing.

**Tech Stack:** Rust 2024, `rmcp` (MCP server), `aws-sdk-bedrockagentruntime` (new workspace dep), `tokio`, `clap`, `tracing`, `serde`. Spec: [docs/superpowers/specs/2026-05-13-kiro-help-bot-design.md](docs/superpowers/specs/2026-05-13-kiro-help-bot-design.md).

**Phase 1 deliverable:** A buildable, testable crate with passing unit + integration tests. The crate is **not yet wired into the bot** — that happens in a later phase. Manual verification at the end: run the binary against a real Bedrock KB and confirm it returns chunks.

---

## Task 1: Workspace setup — add `kiro-knowledge-mcp` member and Bedrock SDK

**Files:**
- Modify: `Cargo.toml` (workspace root)
- Create: `crates/kiro-knowledge-mcp/Cargo.toml`
- Create: `crates/kiro-knowledge-mcp/src/lib.rs`
- Create: `crates/kiro-knowledge-mcp/src/main.rs`

- [ ] **Step 1: Add workspace dep for the Bedrock runtime SDK**

Edit `Cargo.toml`. Find the line `aws-sdk-ssooidc = "1.51.0"` and add a new line directly after it:

```toml
aws-sdk-bedrockagentruntime = "1.66"
```

(Verify on crates.io; pin to a recent stable. The crate name is `aws-sdk-bedrockagentruntime` — single word, no hyphen between "bedrock" and "agent".)

- [ ] **Step 2: Add the new crate to the workspace members list**

In `Cargo.toml`, find the `members = [...]` line at the top of `[workspace]` and add `"crates/kiro-knowledge-mcp"` to the array (alphabetical order is fine, place it next to `"crates/kiro-bot"`):

```toml
members = ["crates/amzn-codewhisperer-client", ..., "crates/kiro-bot", "crates/kiro-knowledge-mcp"]
```

- [ ] **Step 3: Create the crate's `Cargo.toml`**

Write `crates/kiro-knowledge-mcp/Cargo.toml` with:

```toml
[package]
name = "kiro-knowledge-mcp"
version.workspace = true
edition.workspace = true
authors.workspace = true
license.workspace = true
publish.workspace = true
description = "MCP stdio server backed by an Amazon Bedrock Knowledge Base; exposes search_kiro_knowledge for use by the kiro-help bot."

[lib]
name = "kiro_knowledge_mcp"
path = "src/lib.rs"

[[bin]]
name = "kiro-knowledge-mcp"
path = "src/main.rs"

[dependencies]
anyhow.workspace = true
async-trait.workspace = true
aws-config = { workspace = true }
aws-sdk-bedrockagentruntime.workspace = true
clap = { workspace = true }
rmcp = { workspace = true, features = ["server", "transport-io"] }
serde = { workspace = true, features = ["derive"] }
serde_json.workspace = true
tokio = { workspace = true, features = ["full"] }
tracing.workspace = true
tracing-subscriber.workspace = true

[dev-dependencies]
tempfile.workspace = true
```

Note `aws-config` is already in the workspace; we use its `BehaviorVersion::latest()` to construct the SDK config.

- [ ] **Step 4: Create empty `lib.rs` and `main.rs` so `cargo build` succeeds**

Write `crates/kiro-knowledge-mcp/src/lib.rs`:

```rust
//! MCP stdio server backed by an Amazon Bedrock Knowledge Base.
```

Write `crates/kiro-knowledge-mcp/src/main.rs`:

```rust
fn main() {
    eprintln!("kiro-knowledge-mcp: not implemented yet");
    std::process::exit(1);
}
```

- [ ] **Step 5: Verify the workspace builds**

Run: `cargo build -p kiro-knowledge-mcp`
Expected: builds clean, produces `target/debug/kiro-knowledge-mcp` (placeholder binary).

- [ ] **Step 6: Commit**

```bash
git add Cargo.toml crates/kiro-knowledge-mcp/
git commit -m "feat(kiro-knowledge-mcp): add empty crate skeleton"
```

---

## Task 2: Define `RetrievedChunk` and `SearchInput` types in `lib.rs`

**Files:**
- Modify: `crates/kiro-knowledge-mcp/src/lib.rs`

- [ ] **Step 1: Write failing tests for `SearchInput` parsing**

Replace `crates/kiro-knowledge-mcp/src/lib.rs` with:

```rust
//! MCP stdio server backed by an Amazon Bedrock Knowledge Base.

use serde::{Deserialize, Serialize};

/// Source filter for narrowing a knowledge search to a single corpus partition.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SourceFilter {
    Docs,
    Issues,
    Releases,
    All,
}

impl Default for SourceFilter {
    fn default() -> Self {
        Self::All
    }
}

/// Input arguments to the `search_kiro_knowledge` tool.
#[derive(Debug, Clone, Deserialize)]
pub struct SearchInput {
    pub query: String,
    #[serde(default)]
    pub source_filter: SourceFilter,
    #[serde(default = "default_max_results")]
    pub max_results: u32,
}

fn default_max_results() -> u32 {
    5
}

/// A chunk returned by the underlying retriever.
#[derive(Debug, Clone, PartialEq)]
pub struct RetrievedChunk {
    pub source_path: String,
    pub content: String,
    pub relevance: f64,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn search_input_defaults_apply() {
        let input: SearchInput = serde_json::from_value(serde_json::json!({
            "query": "how do i log in"
        }))
        .unwrap();
        assert_eq!(input.query, "how do i log in");
        assert_eq!(input.source_filter, SourceFilter::All);
        assert_eq!(input.max_results, 5);
    }

    #[test]
    fn search_input_accepts_explicit_source_filter() {
        let input: SearchInput = serde_json::from_value(serde_json::json!({
            "query": "release notes",
            "source_filter": "releases",
            "max_results": 10
        }))
        .unwrap();
        assert_eq!(input.source_filter, SourceFilter::Releases);
        assert_eq!(input.max_results, 10);
    }

    #[test]
    fn search_input_rejects_invalid_source_filter() {
        let result: Result<SearchInput, _> = serde_json::from_value(serde_json::json!({
            "query": "x",
            "source_filter": "bogus"
        }));
        assert!(result.is_err());
    }
}
```

- [ ] **Step 2: Run tests — verify they pass**

Run: `cargo test -p kiro-knowledge-mcp --lib`
Expected: 3 tests pass.

(These pass on first write because the implementation is in the same step. The TDD discipline here is structural — types/test live together so future edits keep tests green.)

- [ ] **Step 3: Commit**

```bash
git add crates/kiro-knowledge-mcp/src/lib.rs
git commit -m "feat(kiro-knowledge-mcp): add SearchInput and RetrievedChunk types"
```

---

## Task 3: Define the `Retriever` trait and `format_chunks` helper

**Files:**
- Modify: `crates/kiro-knowledge-mcp/src/lib.rs`

- [ ] **Step 1: Write failing tests for `format_chunks`**

Append to the `tests` module in `crates/kiro-knowledge-mcp/src/lib.rs`:

```rust
    #[test]
    fn format_chunks_renders_numbered_chunks() {
        let chunks = vec![
            RetrievedChunk {
                source_path: "docs/auth.md".into(),
                content: "Run kiro-cli login.".into(),
                relevance: 0.9123,
            },
            RetrievedChunk {
                source_path: "github_issue:kiro-team/kiro-cli#42".into(),
                content: "Login hangs on Linux.".into(),
                relevance: 0.7000,
            },
        ];
        let rendered = format_chunks(&chunks);
        assert!(rendered.contains("[1] docs/auth.md (relevance: 0.91)"));
        assert!(rendered.contains("Run kiro-cli login."));
        assert!(rendered.contains("[2] github_issue:kiro-team/kiro-cli#42 (relevance: 0.70)"));
        assert!(rendered.contains("Login hangs on Linux."));
    }

    #[test]
    fn format_chunks_handles_empty_input() {
        let rendered = format_chunks(&[]);
        assert_eq!(rendered, "No relevant results found.");
    }
```

- [ ] **Step 2: Run tests — verify they fail**

Run: `cargo test -p kiro-knowledge-mcp --lib format_chunks`
Expected: FAIL — `format_chunks` not defined.

- [ ] **Step 3: Add the `Retriever` trait and `format_chunks` function**

Below the `RetrievedChunk` struct in `crates/kiro-knowledge-mcp/src/lib.rs`, add:

```rust
/// Pluggable retriever backend. Production uses Bedrock; tests use a stub.
#[async_trait::async_trait]
pub trait Retriever: Send + Sync {
    async fn retrieve(&self, input: &SearchInput) -> anyhow::Result<Vec<RetrievedChunk>>;
}

/// Render retrieved chunks into the agent-facing string format.
pub fn format_chunks(chunks: &[RetrievedChunk]) -> String {
    if chunks.is_empty() {
        return "No relevant results found.".to_string();
    }
    let mut out = String::new();
    for (i, chunk) in chunks.iter().enumerate() {
        out.push_str(&format!(
            "[{n}] {path} (relevance: {rel:.2})\n    {content}\n\n",
            n = i + 1,
            path = chunk.source_path,
            rel = chunk.relevance,
            content = chunk.content,
        ));
    }
    out.trim_end().to_string()
}
```

Note: `async_trait` is already in dependencies via Step 3 of Task 1.

- [ ] **Step 4: Run tests — verify they pass**

Run: `cargo test -p kiro-knowledge-mcp --lib`
Expected: 5 tests pass.

- [ ] **Step 5: Commit**

```bash
git add crates/kiro-knowledge-mcp/src/lib.rs
git commit -m "feat(kiro-knowledge-mcp): add Retriever trait and format_chunks"
```

---

## Task 4: Add `StubRetriever` for testing

**Files:**
- Modify: `crates/kiro-knowledge-mcp/src/lib.rs`

- [ ] **Step 1: Write failing test that uses `StubRetriever`**

Append to the `tests` module in `crates/kiro-knowledge-mcp/src/lib.rs`:

```rust
    #[tokio::test]
    async fn stub_retriever_returns_canned_chunks() {
        let stub = StubRetriever::new(vec![RetrievedChunk {
            source_path: "docs/x.md".into(),
            content: "stub".into(),
            relevance: 0.5,
        }]);
        let input = SearchInput {
            query: "anything".into(),
            source_filter: SourceFilter::All,
            max_results: 5,
        };
        let chunks = stub.retrieve(&input).await.unwrap();
        assert_eq!(chunks.len(), 1);
        assert_eq!(chunks[0].source_path, "docs/x.md");
    }

    #[tokio::test]
    async fn stub_retriever_respects_max_results() {
        let stub = StubRetriever::new(vec![
            RetrievedChunk { source_path: "a".into(), content: "a".into(), relevance: 1.0 },
            RetrievedChunk { source_path: "b".into(), content: "b".into(), relevance: 0.9 },
            RetrievedChunk { source_path: "c".into(), content: "c".into(), relevance: 0.8 },
        ]);
        let input = SearchInput {
            query: "anything".into(),
            source_filter: SourceFilter::All,
            max_results: 2,
        };
        let chunks = stub.retrieve(&input).await.unwrap();
        assert_eq!(chunks.len(), 2);
        assert_eq!(chunks[0].source_path, "a");
        assert_eq!(chunks[1].source_path, "b");
    }
```

Add `tokio = { workspace = true, features = ["macros", "rt"] }` to `[dev-dependencies]` if not already present:

Edit `crates/kiro-knowledge-mcp/Cargo.toml`. Under `[dev-dependencies]`, ensure:

```toml
[dev-dependencies]
tempfile.workspace = true
tokio = { workspace = true, features = ["macros", "rt"] }
```

- [ ] **Step 2: Run tests — verify they fail**

Run: `cargo test -p kiro-knowledge-mcp --lib stub_retriever`
Expected: FAIL — `StubRetriever` not defined.

- [ ] **Step 3: Add `StubRetriever` to `lib.rs`**

Below the `format_chunks` function in `crates/kiro-knowledge-mcp/src/lib.rs`, add:

```rust
/// In-memory retriever used by tests and the `--stub` CLI mode.
pub struct StubRetriever {
    chunks: Vec<RetrievedChunk>,
}

impl StubRetriever {
    pub fn new(chunks: Vec<RetrievedChunk>) -> Self {
        Self { chunks }
    }
}

#[async_trait::async_trait]
impl Retriever for StubRetriever {
    async fn retrieve(&self, input: &SearchInput) -> anyhow::Result<Vec<RetrievedChunk>> {
        Ok(self
            .chunks
            .iter()
            .take(input.max_results as usize)
            .cloned()
            .collect())
    }
}
```

- [ ] **Step 4: Run tests — verify they pass**

Run: `cargo test -p kiro-knowledge-mcp --lib`
Expected: 7 tests pass.

- [ ] **Step 5: Commit**

```bash
git add crates/kiro-knowledge-mcp/Cargo.toml crates/kiro-knowledge-mcp/src/lib.rs
git commit -m "feat(kiro-knowledge-mcp): add StubRetriever for testing"
```

---

## Task 5: Implement `BedrockRetriever`

**Files:**
- Create: `crates/kiro-knowledge-mcp/src/retrieve.rs`
- Modify: `crates/kiro-knowledge-mcp/src/lib.rs`

- [ ] **Step 1: Wire up the new module in `lib.rs`**

At the top of `crates/kiro-knowledge-mcp/src/lib.rs`, below the doc-comment, add:

```rust
pub mod retrieve;
```

- [ ] **Step 2: Create `retrieve.rs` with the Bedrock client**

Write `crates/kiro-knowledge-mcp/src/retrieve.rs`:

```rust
//! Production `Retriever` impl backed by Amazon Bedrock Knowledge Base.

use anyhow::{Context, Result};
use aws_sdk_bedrockagentruntime::Client;
use aws_sdk_bedrockagentruntime::types::{
    KnowledgeBaseQuery, KnowledgeBaseRetrievalConfiguration,
    KnowledgeBaseVectorSearchConfiguration,
};

use crate::{RetrievedChunk, Retriever, SearchInput, SourceFilter};

pub struct BedrockRetriever {
    client: Client,
    knowledge_base_id: String,
}

impl BedrockRetriever {
    pub async fn new(knowledge_base_id: impl Into<String>) -> Result<Self> {
        let config = aws_config::load_defaults(aws_config::BehaviorVersion::latest()).await;
        let client = Client::new(&config);
        Ok(Self {
            client,
            knowledge_base_id: knowledge_base_id.into(),
        })
    }
}

#[async_trait::async_trait]
impl Retriever for BedrockRetriever {
    async fn retrieve(&self, input: &SearchInput) -> Result<Vec<RetrievedChunk>> {
        // NOTE: source_filter is intentionally ignored in the v1 cut.
        // The agent can filter post-hoc using `source_path` on the returned
        // chunks. Wiring up Bedrock RetrievalFilter::Equals against the
        // metadata attribute "source" is a Phase 1.5 follow-up — see TODO
        // below.
        let _ = input.source_filter; // silence unused warning until 1.5

        let query = KnowledgeBaseQuery::builder()
            .text(&input.query)
            .build()
            .context("building Bedrock query")?;

        let vector_cfg = KnowledgeBaseVectorSearchConfiguration::builder()
            .number_of_results(input.max_results as i32)
            .build()
            .context("building vector search config")?;

        let retrieval_cfg = KnowledgeBaseRetrievalConfiguration::builder()
            .vector_search_configuration(vector_cfg)
            .build()
            .context("building retrieval config")?;

        let resp = self
            .client
            .retrieve()
            .knowledge_base_id(&self.knowledge_base_id)
            .retrieval_query(query)
            .retrieval_configuration(retrieval_cfg)
            .send()
            .await
            .context("calling Bedrock Retrieve")?;

        let chunks = resp
            .retrieval_results
            .into_iter()
            .map(|result| {
                let content = result
                    .content
                    .as_ref()
                    .and_then(|c| c.text.clone())
                    .unwrap_or_default();
                let source_path = result
                    .location
                    .as_ref()
                    .and_then(|loc| loc.s3_location.as_ref())
                    .map(|s3| s3.uri.clone().unwrap_or_default())
                    .unwrap_or_else(|| "<unknown>".to_string());
                let relevance = result.score.unwrap_or(0.0);
                RetrievedChunk {
                    source_path,
                    content,
                    relevance,
                }
            })
            .collect();

        Ok(chunks)
    }
}

// TODO(phase-1.5): wire up source_filter via RetrievalFilter::Equals on a
// `source` metadata attribute. The exact builder shape depends on the
// pinned SDK version — when implementing, run `cargo doc --open
// -p aws-sdk-bedrockagentruntime` and look at `RetrievalFilter` and
// `FilterAttribute`. Reference: the ingest Lambda (Phase 5) writes a
// `source` field on every chunk it stores in S3, so the filter target
// already exists in the data.
```

> **Implementation note:** SDK builder method names (`number_of_results`, `text`, `retrieval_query`, etc.) are version-pinned. After writing the file, run `cargo check -p kiro-knowledge-mcp` — if a builder method doesn't exist, run `cargo doc --open -p aws-sdk-bedrockagentruntime` and find the right name. The overall shape (`Client::retrieve().knowledge_base_id(...).retrieval_query(...).retrieval_configuration(...).send()`) is stable across minor versions.

- [ ] **Step 3: Build and fix compile errors**

Run: `cargo build -p kiro-knowledge-mcp`
Expected: builds clean. If the `RetrievalFilter` construction fails, simplify to no-op filtering (return `None` always) and add a `TODO(filter)` comment.

- [ ] **Step 4: Commit**

```bash
git add crates/kiro-knowledge-mcp/src/lib.rs crates/kiro-knowledge-mcp/src/retrieve.rs
git commit -m "feat(kiro-knowledge-mcp): add BedrockRetriever"
```

---

## Task 6: Wire up the MCP server in `main.rs`

**Files:**
- Modify: `crates/kiro-knowledge-mcp/src/main.rs`

- [ ] **Step 1: Replace `main.rs` with the full MCP ServerHandler implementation**

Write `crates/kiro-knowledge-mcp/src/main.rs`:

```rust
//! kiro-knowledge-mcp — MCP stdio server exposing search_kiro_knowledge,
//! backed either by Amazon Bedrock or by an in-memory stub for testing.

use std::sync::Arc;

use anyhow::Result;
use clap::Parser;
use kiro_knowledge_mcp::retrieve::BedrockRetriever;
use kiro_knowledge_mcp::{
    RetrievedChunk, Retriever, SearchInput, StubRetriever, format_chunks,
};
use rmcp::ServerHandler;
use rmcp::ServiceExt;
use rmcp::model::*;
use rmcp::service::{RequestContext, RoleServer};
use rmcp::transport::stdio;

#[derive(Parser, Debug)]
#[command(name = "kiro-knowledge-mcp", version, about)]
struct Args {
    /// Bedrock Knowledge Base ID. Required unless --stub is set.
    #[arg(long, env = "KIRO_KNOWLEDGE_KB_ID")]
    kb_id: Option<String>,

    /// AWS region. Falls back to the default credential chain if unset.
    #[arg(long, env = "AWS_REGION")]
    region: Option<String>,

    /// Use an in-memory stub retriever instead of Bedrock. For testing only.
    #[arg(long)]
    stub: bool,
}

#[derive(Clone)]
struct KnowledgeServer {
    retriever: Arc<dyn Retriever>,
}

impl ServerHandler for KnowledgeServer {
    fn get_info(&self) -> ServerInfo {
        ServerInfo {
            capabilities: ServerCapabilities::builder().enable_tools().build(),
            instructions: Some(
                "Search the kiro-cli documentation, GitHub issues, and release notes."
                    .to_string(),
            ),
            ..Default::default()
        }
    }

    async fn list_tools(
        &self,
        _request: Option<PaginatedRequestParams>,
        _context: RequestContext<RoleServer>,
    ) -> Result<ListToolsResult, ErrorData> {
        let schema: serde_json::Value = serde_json::json!({
            "type": "object",
            "properties": {
                "query": { "type": "string" },
                "source_filter": {
                    "type": "string",
                    "enum": ["docs", "issues", "releases", "all"],
                    "default": "all"
                },
                "max_results": { "type": "integer", "default": 5 }
            },
            "required": ["query"]
        });
        let tool = Tool {
            name: "search_kiro_knowledge".to_string().into(),
            description: Some(
                "Search the kiro-cli documentation, GitHub issues, and release notes for relevant context. Use this whenever the user asks about kiro-cli behavior, errors, or how-tos before answering.".to_string().into()
            ),
            input_schema: Arc::new(serde_json::from_value(schema).unwrap_or_default()),
            output_schema: None,
            annotations: None,
            execution: None,
            icons: None,
            title: None,
            meta: None,
        };
        Ok(ListToolsResult::with_all_items(vec![tool]))
    }

    async fn call_tool(
        &self,
        request: CallToolRequestParams,
        _context: RequestContext<RoleServer>,
    ) -> Result<CallToolResult, ErrorData> {
        if request.name.as_ref() != "search_kiro_knowledge" {
            return Err(ErrorData::invalid_params(
                format!("unknown tool: {}", request.name),
                None,
            ));
        }

        let args_value: serde_json::Value = match request.arguments {
            Some(map) => serde_json::Value::Object(map.into_iter().collect()),
            None => serde_json::Value::Object(Default::default()),
        };

        let input: SearchInput = serde_json::from_value(args_value).map_err(|e| {
            ErrorData::invalid_params(format!("invalid arguments: {e}"), None)
        })?;

        let chunks: Vec<RetrievedChunk> = self
            .retriever
            .retrieve(&input)
            .await
            .map_err(|e| ErrorData::internal_error(e.to_string(), None))?;

        let text = format_chunks(&chunks);

        Ok(CallToolResult {
            content: vec![Content::text(text)],
            structured_content: None,
            is_error: Some(false),
            meta: None,
        })
    }
}

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info")),
        )
        .with_writer(std::io::stderr)
        .init();

    let args = Args::parse();

    let retriever: Arc<dyn Retriever> = if args.stub {
        Arc::new(StubRetriever::new(vec![
            RetrievedChunk {
                source_path: "docs/stub.md".into(),
                content: "Stub response from kiro-knowledge-mcp.".into(),
                relevance: 0.42,
            },
        ]))
    } else {
        let kb_id = args.kb_id.ok_or_else(|| {
            anyhow::anyhow!("--kb-id is required when --stub is not set (or set KIRO_KNOWLEDGE_KB_ID)")
        })?;
        Arc::new(BedrockRetriever::new(kb_id).await?)
    };

    let server = KnowledgeServer { retriever };
    let service = server.serve(stdio()).await?;
    service.waiting().await?;
    Ok(())
}
```

- [ ] **Step 2: Build**

Run: `cargo build -p kiro-knowledge-mcp`
Expected: builds clean.

> **Note:** Field/variant names on `rmcp::model::*` are version-pinned. If a field name (`structured_content`, `is_error`, etc.) doesn't match the rmcp version in this workspace, run `cargo doc --open -p rmcp` and adjust. The structure (Tool struct + ListToolsResult + CallToolResult) is stable across minor versions; specific optional fields may differ.

- [ ] **Step 3: Smoke test the binary in stub mode**

Run:
```bash
echo '{"jsonrpc":"2.0","id":1,"method":"tools/list"}' | cargo run --quiet -p kiro-knowledge-mcp -- --stub
```

Expected: a JSON-RPC response containing `"name":"search_kiro_knowledge"`.

If the binary hangs, that's actually correct — `rmcp::transport::stdio()` reads from stdin until EOF. Pipe one line and the connection closes; the test should print one response and then exit.

- [ ] **Step 4: Commit**

```bash
git add crates/kiro-knowledge-mcp/src/main.rs
git commit -m "feat(kiro-knowledge-mcp): wire up MCP ServerHandler with stub mode"
```

---

## Task 7: Integration test — spawn binary, do tools/list and tools/call

**Files:**
- Create: `crates/kiro-knowledge-mcp/tests/integration.rs`

- [ ] **Step 1: Write the failing integration test**

Write `crates/kiro-knowledge-mcp/tests/integration.rs`:

```rust
//! End-to-end test: spawn the kiro-knowledge-mcp binary in --stub mode,
//! send tools/list + tools/call requests, parse responses.
//!
//! These tests exercise the full MCP wire format without requiring AWS.

use std::io::{BufRead, BufReader, Write};
use std::process::{Command, Stdio};
use std::time::Duration;

fn cargo_bin() -> String {
    env!("CARGO_BIN_EXE_kiro-knowledge-mcp").to_string()
}

#[test]
fn lists_search_kiro_knowledge_tool() {
    let mut child = Command::new(cargo_bin())
        .args(["--stub"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("spawn binary");

    let mut stdin = child.stdin.take().unwrap();
    let stdout = child.stdout.take().unwrap();
    let mut reader = BufReader::new(stdout);

    // Initialize handshake.
    let init = serde_json::json!({
        "jsonrpc": "2.0",
        "id": 1,
        "method": "initialize",
        "params": {
            "protocolVersion": "2024-11-05",
            "capabilities": {},
            "clientInfo": {"name": "test", "version": "0.1"}
        }
    });
    writeln!(stdin, "{init}").unwrap();
    stdin.flush().unwrap();
    let mut line = String::new();
    reader.read_line(&mut line).unwrap();
    assert!(line.contains("\"jsonrpc\""), "init response: {line}");

    // tools/list
    let list = serde_json::json!({
        "jsonrpc": "2.0",
        "id": 2,
        "method": "tools/list"
    });
    writeln!(stdin, "{list}").unwrap();
    stdin.flush().unwrap();
    let mut line = String::new();
    reader.read_line(&mut line).unwrap();
    assert!(
        line.contains("search_kiro_knowledge"),
        "tools/list response missing tool name: {line}"
    );

    // Close stdin so the server exits.
    drop(stdin);
    let _ = child.wait_timeout(Duration::from_secs(5));
    let _ = child.kill();
}

#[test]
fn calls_search_kiro_knowledge_returns_chunks() {
    let mut child = Command::new(cargo_bin())
        .args(["--stub"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("spawn binary");

    let mut stdin = child.stdin.take().unwrap();
    let stdout = child.stdout.take().unwrap();
    let mut reader = BufReader::new(stdout);

    // initialize
    let init = serde_json::json!({
        "jsonrpc": "2.0",
        "id": 1,
        "method": "initialize",
        "params": {
            "protocolVersion": "2024-11-05",
            "capabilities": {},
            "clientInfo": {"name": "test", "version": "0.1"}
        }
    });
    writeln!(stdin, "{init}").unwrap();
    stdin.flush().unwrap();
    let mut line = String::new();
    reader.read_line(&mut line).unwrap();

    // tools/call
    let call = serde_json::json!({
        "jsonrpc": "2.0",
        "id": 3,
        "method": "tools/call",
        "params": {
            "name": "search_kiro_knowledge",
            "arguments": { "query": "test" }
        }
    });
    writeln!(stdin, "{call}").unwrap();
    stdin.flush().unwrap();
    let mut line = String::new();
    reader.read_line(&mut line).unwrap();

    assert!(line.contains("docs/stub.md"), "tools/call response: {line}");
    assert!(line.contains("Stub response"), "tools/call response: {line}");

    drop(stdin);
    let _ = child.kill();
}

// Tiny helper — since we don't pull in `wait-timeout` for this crate.
trait WaitTimeout {
    fn wait_timeout(&mut self, d: Duration) -> std::io::Result<Option<std::process::ExitStatus>>;
}
impl WaitTimeout for std::process::Child {
    fn wait_timeout(&mut self, _d: Duration) -> std::io::Result<Option<std::process::ExitStatus>> {
        // We don't actually need wait_timeout for these tests — they kill the child explicitly.
        // Stub it out so the trait import elsewhere doesn't break.
        Ok(None)
    }
}
```

(Add `serde_json` to `[dev-dependencies]` if not already pulled in transitively. Edit `crates/kiro-knowledge-mcp/Cargo.toml`:

```toml
[dev-dependencies]
serde_json.workspace = true
tempfile.workspace = true
tokio = { workspace = true, features = ["macros", "rt"] }
```
)

- [ ] **Step 2: Run integration tests**

Run: `cargo test -p kiro-knowledge-mcp --test integration`
Expected: 2 tests pass.

If a test hangs, the most likely cause is line-buffering on the spawned process. The MCP stdio framing in `rmcp::transport::stdio()` is line-delimited JSON-RPC; ensure each request uses `writeln!` (already done above) and ensure `stdin.flush()` is called after each write (also done).

- [ ] **Step 3: Commit**

```bash
git add crates/kiro-knowledge-mcp/Cargo.toml crates/kiro-knowledge-mcp/tests/integration.rs
git commit -m "test(kiro-knowledge-mcp): integration test for tools/list + tools/call"
```

---

## Task 8: Add a crate `README.md`

**Files:**
- Create: `crates/kiro-knowledge-mcp/README.md`

- [ ] **Step 1: Write the README**

Write `crates/kiro-knowledge-mcp/README.md`:

```markdown
# kiro-knowledge-mcp

MCP stdio server backed by an Amazon Bedrock Knowledge Base. Exposes one tool — `search_kiro_knowledge` — for use by the kiro-help bot.

## Build

```bash
cargo build --release -p kiro-knowledge-mcp
```

## Run (stub mode, for local testing)

```bash
kiro-knowledge-mcp --stub
```

Returns a fixed canned response. No AWS credentials required. Used by integration tests and for verifying MCP wire-up without standing up Bedrock.

## Run (production)

```bash
kiro-knowledge-mcp --kb-id ABC123XYZ --region us-west-2
```

Or via env vars:

```bash
KIRO_KNOWLEDGE_KB_ID=ABC123XYZ AWS_REGION=us-west-2 kiro-knowledge-mcp
```

The process uses the AWS default credential chain (env vars, profile, EC2/ECS task role).

## Tool: `search_kiro_knowledge`

```json
{
  "name": "search_kiro_knowledge",
  "inputSchema": {
    "type": "object",
    "properties": {
      "query":         { "type": "string" },
      "source_filter": { "type": "string", "enum": ["docs","issues","releases","all"], "default": "all" },
      "max_results":   { "type": "integer", "default": 5 }
    },
    "required": ["query"]
  }
}
```

Returns the top-N retrieved chunks formatted as numbered citations:

```
[1] docs/auth.md (relevance: 0.91)
    Run kiro-cli login. ...

[2] github_issue:kiro-team/kiro-cli#42 (relevance: 0.70)
    Login hangs on Linux. ...
```

## IAM

The host process needs:

- `bedrock:Retrieve` on the target Knowledge Base ARN.

That is the only AWS permission this binary uses.

## See also

- Spec: [docs/superpowers/specs/2026-05-13-kiro-help-bot-design.md](../../docs/superpowers/specs/2026-05-13-kiro-help-bot-design.md)
- Existing MCP server pattern: [crates/mock-mcp-server/](../mock-mcp-server/)
```

- [ ] **Step 2: Commit**

```bash
git add crates/kiro-knowledge-mcp/README.md
git commit -m "docs(kiro-knowledge-mcp): add crate README"
```

---

## Task 9: Final verification — workspace-wide build and test

- [ ] **Step 1: Workspace build**

Run: `cargo build -p kiro-knowledge-mcp`
Expected: builds clean.

- [ ] **Step 2: Workspace tests**

Run: `cargo test -p kiro-knowledge-mcp`
Expected: 7 unit tests + 2 integration tests pass (9 total).

- [ ] **Step 3: Verify clippy is clean for the new crate**

Run: `cargo clippy -p kiro-knowledge-mcp -- -D warnings`
Expected: no warnings. If clippy complains about `clippy::large_types_passed_by_value` or similar workspace-level lints, fix in place — don't suppress.

- [ ] **Step 4: Verify formatting**

Run: `cargo fmt -p kiro-knowledge-mcp -- --check`
Expected: no diff. If it complains, run `cargo fmt -p kiro-knowledge-mcp` and amend the previous commit.

- [ ] **Step 5: Manual verification (optional, requires AWS account)**

This is the final acceptance check. Skip if no Bedrock KB is provisioned yet.

```bash
# Stand up a tiny test KB in account 551670267384 with one S3-backed data source containing a single doc.
# Get its KB id, then:
echo '{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2024-11-05","capabilities":{},"clientInfo":{"name":"manual","version":"0"}}}
{"jsonrpc":"2.0","id":2,"method":"tools/call","params":{"name":"search_kiro_knowledge","arguments":{"query":"how do i log in"}}}' | \
  KIRO_KNOWLEDGE_KB_ID=<kb-id> AWS_REGION=us-west-2 \
  cargo run --release -p kiro-knowledge-mcp
```

Expected: second response contains chunks pulled from the test KB. If it errors with a credentials issue, ensure your AWS profile is exported (`AWS_PROFILE=...`) and has `bedrock:Retrieve` on the KB.

This step is **not required for Phase 1 acceptance** — Phase 5 (CDK ingest stack) provisions the real KB. The point of running it manually now is to flush out wiring bugs the integration test can't catch (IAM, SDK version mismatches, KB-not-found).

- [ ] **Step 6: No commit needed for verification — Phase 1 is done.**

---

## Phase 1 acceptance

The crate `kiro-knowledge-mcp/` exists, builds clean, has unit and integration tests passing, and has a working `--stub` mode plus a Bedrock-backed real mode. It is **not yet wired into kiro-bot** — that's Phase 4. Engineers reviewing this should be able to:

- `cargo test -p kiro-knowledge-mcp` and see green.
- Run the binary in stub mode and exercise the MCP wire protocol manually.
- Read the README to understand inputs, outputs, and IAM requirements.
- Trace from spec section "1. New Rust crate: `crates/kiro-knowledge-mcp/`" to the implemented code.

## What this plan deliberately does NOT do

- **No bot integration.** The `kiro-help` agent mode and bot config directory belong to Phase 4. This plan produces a standalone binary.
- **No deployment.** No Dockerfile, no GH Actions workflow, no AWS infra. Phase 5 + 6 cover that. The binary will be containerized later — for now it builds and runs locally.
- **No source filter implementation.** If `RetrievalFilter` construction proves fiddly against the SDK version pinned in Task 1, we ship Phase 1 with `SourceFilter::All` semantics regardless of input. The `source_path` in returned chunks lets the agent filter post-hoc. Filter implementation is a Phase 1.5 task.
- **No real-Bedrock CI test.** The integration test uses `--stub`. A live Bedrock test against a beta KB happens in the GH Actions workflow (Phase 6) and as part of the eval suite (Phase 4).
