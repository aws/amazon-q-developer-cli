//! /stats command — dump request IDs and timings from the in-memory ring buffer.

use agent::tui_commands::{
    CommandResult,
    StatsArgs,
};
use serde_json::json;

use super::CommandContext;
use crate::agent::acp::request_stats::{
    RequestRecord,
    RequestStats,
};

pub async fn execute(args: &StatsArgs, ctx: &CommandContext<'_>) -> CommandResult {
    // Parse "save <filename>" subcommand
    if let Some(ref sub) = args.subcommand {
        let parts: Vec<&str> = sub.splitn(2, char::is_whitespace).collect();
        if parts.first().is_some_and(|s| *s == "save") {
            let filename = parts.get(1).map_or("stats.json", |s| s.trim());
            return save(ctx.request_stats, filename, ctx.cwd);
        }
        if let Ok(n) = sub.parse::<u32>() {
            return show(ctx.request_stats, Some(n));
        }
        return CommandResult::error(format!("Unknown subcommand: {sub}\nUsage: /stats [N|save <filename>]"));
    }

    show(ctx.request_stats, args.last)
}

fn show(stats: &RequestStats, last: Option<u32>) -> CommandResult {
    let all = stats.snapshot();

    let records: &[RequestRecord] = match last {
        Some(n) if (n as usize) < all.len() => &all[all.len() - n as usize..],
        _ => &all,
    };

    let stats_json: Vec<serde_json::Value> = records
        .iter()
        .map(|r| {
            json!({
                "request_id": r.request_id,
                "timestamp": r.timestamp.to_rfc3339(),
                "duration_ms": r.duration.map(|d| d.as_secs_f64() * 1000.0),
                "ttfc_ms": r.time_to_first_chunk.map(|d| d.as_secs_f64() * 1000.0),
                "input_tokens": r.input_tokens,
                "output_tokens": r.output_tokens,
                "status_code": r.status_code,
                "had_tool_use": r.had_tool_use,
                "error": r.error,
            })
        })
        .collect();

    let summary = compute_summary(records);

    let message = if records.is_empty() {
        "No requests recorded yet".to_string()
    } else {
        format!(
            "{} request{} recorded",
            records.len(),
            if records.len() == 1 { "" } else { "s" }
        )
    };

    CommandResult::success_with_data(&message, json!({ "stats": stats_json, "summary": summary }))
}

fn compute_summary(records: &[RequestRecord]) -> Option<serde_json::Value> {
    let durations: Vec<f64> = records
        .iter()
        .filter_map(|r| r.duration.map(|d| d.as_secs_f64() * 1000.0))
        .collect();
    if durations.is_empty() {
        return None;
    }
    let avg = durations.iter().sum::<f64>() / durations.len() as f64;
    let max = durations.iter().cloned().fold(f64::NEG_INFINITY, f64::max);
    let mut sorted = durations;
    sorted.sort_by(|a, b| a.partial_cmp(b).unwrap());
    let p90 = sorted[((sorted.len() as f64 * 0.9).ceil() as usize).min(sorted.len()) - 1];
    Some(json!({
        "avg_ms": avg,
        "p90_ms": p90,
        "max_ms": max,
        "errors": records.iter().filter(|r| r.error.is_some()).count(),
    }))
}

fn save(stats: &RequestStats, filename: &str, cwd: &std::path::Path) -> CommandResult {
    let records = stats.snapshot();
    if records.is_empty() {
        return CommandResult::error("No requests recorded yet.");
    }

    let json: Vec<serde_json::Value> = records
        .iter()
        .map(|r| {
            json!({
                "request_id": r.request_id,
                "timestamp": r.timestamp.to_rfc3339(),
                "duration_ms": r.duration.map(|d| d.as_secs_f64() * 1000.0),
                "ttfc_ms": r.time_to_first_chunk.map(|d| d.as_secs_f64() * 1000.0),
                "input_tokens": r.input_tokens,
                "output_tokens": r.output_tokens,
                "status_code": r.status_code,
                "had_tool_use": r.had_tool_use,
                "error": r.error,
            })
        })
        .collect();

    let path = if std::path::Path::new(filename).is_absolute() {
        std::path::PathBuf::from(filename)
    } else {
        cwd.join(filename)
    };

    match std::fs::write(&path, serde_json::to_string_pretty(&json).unwrap()) {
        Ok(()) => CommandResult::success(format!("Saved {} records to {}", records.len(), path.display())),
        Err(e) => CommandResult::error(format!("Failed to write {}: {e}", path.display())),
    }
}
