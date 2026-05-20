//! Nightly Lambda that scans the kiro-bot-feedback table, computes the
//! 7-day rolling negative-feedback rate, and publishes
//! `KiroHelpBot::NegativeFeedbackRate` (plus per-bucket counts) to
//! CloudWatch.
//!
//! Configuration via env (set by `KiroBotAlarmsStack` when this Lambda is
//! wired up — currently a follow-up; the binary is ready ahead of the CDK
//! plumbing):
//!
//! - `STAGE`           — alpha | beta | prod (drives metric Stage dimension)
//! - `FEEDBACK_TABLE`  — DDB feedback table name (e.g. kiro-bot-feedback-prod)
//! - `AWS_REGION`

use std::env;

use anyhow::{
    Context,
    Result,
};
use aws_sdk_cloudwatch::Client as CwClient;
use aws_sdk_cloudwatch::types::{
    Dimension,
    MetricDatum,
    StandardUnit,
};
use aws_sdk_dynamodb::Client as DdbClient;
use chrono::{
    DateTime,
    Utc,
};
use kiro_bot_metrics::{
    FeedbackRow,
    compute_negative_feedback_rate,
};
use lambda_runtime::{
    Error,
    LambdaEvent,
    service_fn,
};
use tracing::{
    error,
    info,
};

const NAMESPACE: &str = "KiroHelpBot";

#[tokio::main]
async fn main() -> Result<(), Error> {
    tracing_subscriber::fmt()
        .with_target(false)
        .with_ansi(false)
        .without_time()
        .init();
    let func = service_fn(handler);
    lambda_runtime::run(func).await
}

async fn handler(event: LambdaEvent<serde_json::Value>) -> Result<serde_json::Value, Error> {
    let _ = event;
    let stage = env::var("STAGE").unwrap_or_else(|_| "alpha".to_string());
    let table = env::var("FEEDBACK_TABLE").context("FEEDBACK_TABLE env required")?;
    let region = env::var("AWS_REGION").ok();

    let mut loader = aws_config::defaults(aws_config::BehaviorVersion::latest());
    if let Some(r) = &region {
        loader = loader.region(aws_config::Region::new(r.clone()));
    }
    let cfg = loader.load().await;
    let ddb = DdbClient::new(&cfg);
    let cw = CwClient::new(&cfg);

    let rows = scan_feedback(&ddb, &table).await.context("scanning feedback")?;
    info!(rows = rows.len(), "scanned feedback rows");

    let now = Utc::now();
    let tally = compute_negative_feedback_rate(&rows, now);
    info!(?tally, "tally");

    publish(&cw, &stage, now, &tally).await.context("CloudWatch PutMetricData")?;
    Ok(serde_json::json!({
        "stage": stage,
        "up": tally.up,
        "down": tally.down,
        "negative_rate": tally.negative_rate(),
    }))
}

async fn scan_feedback(client: &DdbClient, table: &str) -> Result<Vec<FeedbackRow>> {
    let mut rows = Vec::new();
    let mut last_key = None;
    loop {
        let req = client.scan().table_name(table);
        let req = if let Some(start) = last_key.take() {
            req.set_exclusive_start_key(Some(start))
        } else {
            req
        };
        let resp = req.send().await.with_context(|| format!("Scan {table}"))?;
        for item in resp.items.unwrap_or_default() {
            let slack_msg_id = match item.get("slack_msg_id").and_then(|v| v.as_s().ok()) {
                Some(v) => v.clone(),
                None => continue,
            };
            let reaction = match item.get("reaction").and_then(|v| v.as_s().ok()) {
                Some(v) => v.clone(),
                None => continue,
            };
            let ts_str = match item.get("ts").and_then(|v| v.as_s().ok()) {
                Some(v) => v.clone(),
                None => continue,
            };
            let ts: DateTime<Utc> = match DateTime::parse_from_rfc3339(&ts_str) {
                Ok(t) => t.with_timezone(&Utc),
                Err(_) => continue,
            };
            rows.push(FeedbackRow { slack_msg_id, reaction, ts });
        }
        match resp.last_evaluated_key {
            Some(k) if !k.is_empty() => last_key = Some(k),
            _ => break,
        }
    }
    Ok(rows)
}

async fn publish(
    cw: &CwClient,
    stage: &str,
    now: DateTime<Utc>,
    tally: &kiro_bot_metrics::FeedbackTally,
) -> Result<()> {
    let dim = Dimension::builder()
        .name("Stage")
        .value(stage)
        .build();
    let timestamp = aws_sdk_cloudwatch::primitives::DateTime::from_secs(now.timestamp());
    let datums = vec![
        MetricDatum::builder()
            .metric_name("NegativeFeedbackRate")
            .dimensions(dim.clone())
            .timestamp(timestamp)
            .value(tally.negative_rate())
            .unit(StandardUnit::None)
            .build(),
        MetricDatum::builder()
            .metric_name("FeedbackThumbsUp")
            .dimensions(dim.clone())
            .timestamp(timestamp)
            .value(tally.up as f64)
            .unit(StandardUnit::Count)
            .build(),
        MetricDatum::builder()
            .metric_name("FeedbackThumbsDown")
            .dimensions(dim)
            .timestamp(timestamp)
            .value(tally.down as f64)
            .unit(StandardUnit::Count)
            .build(),
    ];
    let resp = cw
        .put_metric_data()
        .namespace(NAMESPACE)
        .set_metric_data(Some(datums))
        .send()
        .await;
    if let Err(e) = resp {
        error!(?e, "PutMetricData failed");
        anyhow::bail!("PutMetricData failed: {e}");
    }
    Ok(())
}
