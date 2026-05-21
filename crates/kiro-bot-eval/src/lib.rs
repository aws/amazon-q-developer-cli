//! Offline RAG retrieval eval suite for the kiro-help bot.
//!
//! Hand-curated cases in `data/kiro-help.jsonl` declare a query and a list of
//! source paths that must appear in the top-N retrieval results. The runner
//! computes overall recall@N and a per-category breakdown, and exits non-zero
//! when recall drops below a threshold so it can act as a CI gate.

use kiro_knowledge_mcp::{
    Retriever,
    SearchInput,
    SourceFilter,
};
use serde::{
    Deserialize,
    Serialize,
};

/// One eval case: a query plus the set of doc paths at least one of which
/// must show up in the top-N retrieved chunks for the case to pass.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct EvalCase {
    pub id: String,
    pub query: String,
    pub must_retrieve_any_of: Vec<String>,
    #[serde(default)]
    pub category: String,
}

/// One row of the report — case id, whether it passed, and which retrieved
/// paths were considered.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CaseResult {
    pub id: String,
    pub category: String,
    pub passed: bool,
    pub retrieved_paths: Vec<String>,
    pub expected_any_of: Vec<String>,
}

/// Aggregate eval report.
#[derive(Debug, Clone, PartialEq)]
pub struct EvalReport {
    pub total: usize,
    pub passed: usize,
    pub recall: f64,
    pub by_category: Vec<(String, f64)>,
    pub results: Vec<CaseResult>,
}

impl EvalReport {
    pub fn passes_threshold(&self, threshold: f64) -> bool {
        self.recall >= threshold
    }
}

/// Parse an embedded JSONL file (or contents read from disk). Comment lines
/// starting with `//` and blank lines are tolerated.
pub fn parse_cases(jsonl: &str) -> anyhow::Result<Vec<EvalCase>> {
    let mut out = Vec::new();
    for (idx, line) in jsonl.lines().enumerate() {
        let line = line.trim();
        if line.is_empty() || line.starts_with("//") {
            continue;
        }
        let case: EvalCase = serde_json::from_str(line)
            .map_err(|e| anyhow::anyhow!("line {} not valid JSON: {}", idx + 1, e))?;
        out.push(case);
    }
    Ok(out)
}

/// Embedded default eval set. Co-located with the binary so the eval can run
/// without needing the source tree.
pub const EMBEDDED_CASES_JSONL: &str = include_str!("../data/kiro-help.jsonl");

/// Run all cases through the supplied retriever and produce a report.
pub async fn run_eval(
    retriever: &dyn Retriever,
    cases: &[EvalCase],
    top_n: u32,
) -> anyhow::Result<EvalReport> {
    let mut results = Vec::with_capacity(cases.len());
    for case in cases {
        let chunks = retriever
            .retrieve(&SearchInput {
                query: case.query.clone(),
                source_filter: SourceFilter::default(),
                max_results: top_n,
            })
            .await?;
        let retrieved_paths: Vec<String> = chunks.iter().map(|c| c.source_path.clone()).collect();
        // Bedrock returns full S3 URIs like
        //   s3://<bucket>/root/docs/auth.md
        // (or s3://<bucket>/docs/auth.md). Eval cases are written as bare
        // repo-relative paths (`docs/auth.md`) because the corpus bucket is
        // an internal detail and the case file shouldn't churn when the
        // bucket name changes. Match by suffix so both formats work.
        let passed = case.must_retrieve_any_of.iter().any(|expected| {
            retrieved_paths.iter().any(|got| {
                got == expected
                    || got.ends_with(&format!("/{expected}"))
                    || got.ends_with(expected.as_str())
            })
        });
        results.push(CaseResult {
            id: case.id.clone(),
            category: case.category.clone(),
            passed,
            retrieved_paths,
            expected_any_of: case.must_retrieve_any_of.clone(),
        });
    }
    Ok(summarize(results))
}

fn summarize(results: Vec<CaseResult>) -> EvalReport {
    let total = results.len();
    let passed = results.iter().filter(|r| r.passed).count();
    let recall = if total == 0 {
        1.0
    } else {
        passed as f64 / total as f64
    };
    let mut counts: std::collections::BTreeMap<String, (usize, usize)> = Default::default();
    for r in &results {
        let entry = counts.entry(r.category.clone()).or_insert((0, 0));
        entry.0 += 1;
        if r.passed {
            entry.1 += 1;
        }
    }
    let by_category: Vec<(String, f64)> = counts
        .into_iter()
        .map(|(cat, (n, ok))| (cat, if n == 0 { 1.0 } else { ok as f64 / n as f64 }))
        .collect();
    EvalReport { total, passed, recall, by_category, results }
}

/// Format a human-readable summary suitable for stdout / CI logs.
pub fn format_report(report: &EvalReport, threshold: f64) -> String {
    let mut s = String::new();
    s.push_str(&format!(
        "Eval result: {}/{} cases passed (recall = {:.1}%, threshold = {:.1}%)\n",
        report.passed,
        report.total,
        report.recall * 100.0,
        threshold * 100.0,
    ));
    if !report.by_category.is_empty() {
        s.push_str("\nBy category:\n");
        for (cat, recall) in &report.by_category {
            s.push_str(&format!("  {:<24} {:.1}%\n", cat, recall * 100.0));
        }
    }
    let failed: Vec<&CaseResult> = report.results.iter().filter(|r| !r.passed).collect();
    if !failed.is_empty() {
        s.push_str("\nFailed cases:\n");
        for r in failed {
            s.push_str(&format!(
                "  [{}] {:<32} expected one of: {:?}\n      got: {:?}\n",
                r.category, r.id, r.expected_any_of, r.retrieved_paths,
            ));
        }
    }
    s
}

#[cfg(test)]
mod tests {
    use super::*;
    use kiro_knowledge_mcp::{
        RetrievedChunk,
        StubRetriever,
    };

    fn case(id: &str, query: &str, expected: &[&str], category: &str) -> EvalCase {
        EvalCase {
            id: id.to_string(),
            query: query.to_string(),
            must_retrieve_any_of: expected.iter().map(|s| s.to_string()).collect(),
            category: category.to_string(),
        }
    }

    fn chunk(path: &str, relevance: f64) -> RetrievedChunk {
        RetrievedChunk {
            source_path: path.to_string(),
            content: "stub".to_string(),
            relevance,
        }
    }

    #[test]
    fn parses_jsonl_with_comments_and_blank_lines() {
        let src = r#"
// header comment
{"id":"a","query":"q","must_retrieve_any_of":["docs/a.md"],"category":"docs"}

{"id":"b","query":"q","must_retrieve_any_of":["docs/b.md"],"category":"docs"}
"#;
        let cases = parse_cases(src).unwrap();
        assert_eq!(cases.len(), 2);
        assert_eq!(cases[0].id, "a");
        assert_eq!(cases[1].id, "b");
    }

    #[test]
    fn parse_cases_reports_line_number_on_bad_json() {
        let src = "{\"id\":\"a\",\"query\":\"q\",\"must_retrieve_any_of\":[\"docs/a.md\"]}\nnot json\n";
        let err = parse_cases(src).unwrap_err();
        assert!(err.to_string().contains("line 2"), "msg was {err}");
    }

    #[test]
    fn embedded_cases_are_well_formed() {
        let cases = parse_cases(EMBEDDED_CASES_JSONL).expect("embedded JSONL must parse");
        assert!(!cases.is_empty(), "must have at least one embedded case");
        for c in &cases {
            assert!(!c.id.is_empty(), "every case needs an id");
            assert!(!c.query.is_empty(), "every case needs a query");
            assert!(
                !c.must_retrieve_any_of.is_empty(),
                "case {} has no must_retrieve_any_of",
                c.id
            );
        }
    }

    #[tokio::test]
    async fn run_eval_passes_when_any_expected_path_returned() {
        let retriever = StubRetriever::new(vec![
            chunk("docs/auth.md", 0.91),
            chunk("docs/other.md", 0.50),
        ]);
        let cases = vec![case("auth-001", "login", &["docs/auth.md"], "auth")];
        let report = run_eval(&retriever, &cases, 5).await.unwrap();
        assert_eq!(report.passed, 1);
        assert_eq!(report.total, 1);
        assert!((report.recall - 1.0).abs() < 1e-9);
        assert!(report.passes_threshold(0.7));
    }

    #[tokio::test]
    async fn run_eval_fails_when_expected_path_missing() {
        let retriever = StubRetriever::new(vec![chunk("docs/other.md", 0.50)]);
        let cases = vec![case("auth-001", "login", &["docs/auth.md"], "auth")];
        let report = run_eval(&retriever, &cases, 5).await.unwrap();
        assert_eq!(report.passed, 0);
        assert!(!report.passes_threshold(0.5));
    }

    #[tokio::test]
    async fn run_eval_breaks_down_recall_per_category() {
        // StubRetriever returns the same chunks for every query, so set up a
        // fixed retrieval set and craft two cases — one match, one miss — per
        // category.
        let retriever = StubRetriever::new(vec![chunk("docs/auth.md", 0.9)]);
        let cases = vec![
            case("auth-1", "q", &["docs/auth.md"], "auth"),    // pass
            case("mcp-1", "q", &["docs/mcp.md"], "mcp"),       // miss
        ];
        let report = run_eval(&retriever, &cases, 5).await.unwrap();
        let by_cat: std::collections::BTreeMap<&str, f64> =
            report.by_category.iter().map(|(c, r)| (c.as_str(), *r)).collect();
        assert!((by_cat["auth"] - 1.0).abs() < 1e-9);
        assert!((by_cat["mcp"] - 0.0).abs() < 1e-9);
        assert!((report.recall - 0.5).abs() < 1e-9);
    }

    #[test]
    fn format_report_lists_failed_cases() {
        let r = EvalReport {
            total: 2,
            passed: 1,
            recall: 0.5,
            by_category: vec![("auth".to_string(), 0.5)],
            results: vec![
                CaseResult {
                    id: "auth-1".to_string(),
                    category: "auth".to_string(),
                    passed: true,
                    retrieved_paths: vec!["docs/auth.md".to_string()],
                    expected_any_of: vec!["docs/auth.md".to_string()],
                },
                CaseResult {
                    id: "auth-2".to_string(),
                    category: "auth".to_string(),
                    passed: false,
                    retrieved_paths: vec!["docs/other.md".to_string()],
                    expected_any_of: vec!["docs/login.md".to_string()],
                },
            ],
        };
        let s = format_report(&r, 0.7);
        assert!(s.contains("1/2"));
        assert!(s.contains("50.0%"));
        assert!(s.contains("Failed cases"));
        assert!(s.contains("auth-2"));
    }
}
