//! `kiro-bot-eval` — runs the offline RAG retrieval eval against a real
//! Bedrock Knowledge Base. The library (`kiro_bot_eval`) carries the testable
//! logic; this binary just wires CLI arguments through.

use std::path::PathBuf;

use anyhow::{
    Context,
    Result,
};
use clap::Parser;
use kiro_bot_eval::{
    EMBEDDED_CASES_JSONL,
    format_report,
    parse_cases,
    run_eval,
};
use kiro_knowledge_mcp::retrieve::BedrockRetriever;
use tracing::info;

#[derive(Debug, Parser)]
#[command(version, about = "Offline RAG retrieval eval for the kiro-help bot")]
struct Args {
    /// Bedrock Knowledge Base ID. Defaults to KIRO_KNOWLEDGE_KB_ID.
    #[arg(long, env = "KIRO_KNOWLEDGE_KB_ID")]
    kb_id: String,

    /// AWS region.
    #[arg(long, env = "AWS_REGION", default_value = "us-east-1")]
    region: String,

    /// Top-N retrieval count.
    #[arg(long, default_value_t = 5)]
    top_n: u32,

    /// Recall threshold; the runner exits non-zero when overall recall < this.
    #[arg(long, default_value_t = 0.7)]
    threshold: f64,

    /// Override the embedded JSONL case file.
    #[arg(long)]
    cases: Option<PathBuf>,
}

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt::try_init().ok();
    let args = Args::parse();

    let cases_src = match args.cases.as_ref() {
        Some(p) => std::fs::read_to_string(p).with_context(|| format!("reading {}", p.display()))?,
        None => EMBEDDED_CASES_JSONL.to_string(),
    };
    let cases = parse_cases(&cases_src)?;
    info!(case_count = cases.len(), kb_id = %args.kb_id, "starting eval");

    let retriever = BedrockRetriever::new(args.kb_id.clone(), Some(args.region.clone())).await?;
    let report = run_eval(&retriever, &cases, args.top_n).await?;
    print!("{}", format_report(&report, args.threshold));

    if !report.passes_threshold(args.threshold) {
        std::process::exit(1);
    }
    Ok(())
}
