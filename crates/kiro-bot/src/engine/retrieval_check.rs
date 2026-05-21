//! Lightweight after-the-fact check that the model called retrieval before
//! answering a kiro-related question. This is a *belt-and-suspenders* layer
//! on top of the prompt's MUST-language: if the model still skips
//! `search_kiro_knowledge` and ships an ungrounded answer, we want a metric
//! that surfaces it instead of silently sending.
//!
//! Strategy:
//! - Look at the user prompt for kiro-keyword markers ("kiro", "kiro-cli",
//!   "/<slash-command>", error-message phrasing). If none, skip — the
//!   question wasn't supposed to retrieve.
//! - Look at the model reply for citation markers (`Sources:`, `docs/`,
//!   `github_issue:`). If found, retrieval likely happened — pass.
//! - If the prompt was kiro-related but the reply has no citation, log a
//!   structured warning so we can grep CloudWatch for it and add a metric.
//!
//! This is intentionally cheap text matching, not LLM-based. False positives
//! (logging a warning on a benign reply) are fine; false negatives (missing
//! a real ungrounded answer) are the cost we pay for keeping it cheap.
//!
//! See `kiro_help_prompt.md` and `kiro-help-skill` for the contract this
//! check enforces.

/// Result of a single retrieval-shape check. Tests assert on this; the
/// runtime caller turns `MissingCitation` into a tracing warn + metric.
#[derive(Debug, PartialEq, Eq)]
pub enum RetrievalCheck {
    /// Prompt didn't look kiro-related; we don't expect retrieval.
    NotApplicable,
    /// Prompt was kiro-related and the reply carries a citation marker.
    Cited,
    /// Prompt was kiro-related but the reply has no citation. Caller should
    /// emit a metric / warning.
    MissingCitation,
}

const KIRO_KEYWORDS: &[&str] = &[
    "kiro",
    "kiro-cli",
    "kiro cli",
    "kiro-bot",
    "kiro_help",
    "knowledge base",
    "knowledge-base",
];

/// Heuristic: does the prompt look like it should have hit retrieval?
fn prompt_is_kiro_related(prompt: &str) -> bool {
    let lower = prompt.to_lowercase();
    if KIRO_KEYWORDS.iter().any(|k| lower.contains(k)) {
        return true;
    }
    // A leading slash command is almost always a kiro-cli question.
    if lower
        .split_whitespace()
        .any(|w| w.starts_with('/') && w.len() > 1)
    {
        return true;
    }
    false
}

/// Heuristic: does the reply contain any of the citation shapes the skill
/// teaches the model to use?
fn reply_has_citation(reply: &str) -> bool {
    // The skill's worked example anchors on the literal "Sources:" line; that's
    // the dominant shape. Also accept inline `docs/...` and `github_issue:`
    // mentions because the model occasionally goes prose instead of a list.
    let needles = [
        "Sources:",
        "Source:",
        "docs/",
        "autodocs/",
        "github_issue:",
        "github.com/kiro-team/kiro-cli/issues/",
    ];
    needles.iter().any(|n| reply.contains(n))
}

pub fn check(prompt: &str, reply: &str) -> RetrievalCheck {
    if !prompt_is_kiro_related(prompt) {
        return RetrievalCheck::NotApplicable;
    }
    if reply_has_citation(reply) {
        RetrievalCheck::Cited
    } else {
        RetrievalCheck::MissingCitation
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn off_topic_prompt_is_not_applicable() {
        assert_eq!(
            check("hi how are you", "I'm a Slack bot, ask me about kiro-cli."),
            RetrievalCheck::NotApplicable,
        );
    }

    #[test]
    fn kiro_question_with_sources_line_is_cited() {
        let reply = "Run `kiro-cli login`.\n\nSources: `docs/auth.md`";
        assert_eq!(
            check("how do I log in to kiro-cli", reply),
            RetrievalCheck::Cited,
        );
    }

    #[test]
    fn kiro_question_with_inline_doc_path_is_cited() {
        // Some replies skip the trailing Sources: line and just inline the
        // path. Don't punish those.
        let reply = "Per `docs/commands/login.md`, run `kiro-cli login`.";
        assert_eq!(
            check("how do I log in to kiro-cli", reply),
            RetrievalCheck::Cited,
        );
    }

    #[test]
    fn kiro_question_with_no_citation_is_missing() {
        let reply = "Run `kiro-cli login` — it opens your browser to auth.";
        assert_eq!(
            check("how do I log in to kiro-cli", reply),
            RetrievalCheck::MissingCitation,
        );
    }

    #[test]
    fn slash_command_question_is_kiro_related_even_without_keyword() {
        // "what does /code init do?" never says kiro but is clearly a kiro-cli
        // question — slash-command shape is enough.
        let reply = "Initializes LSP for the workspace.";
        assert_eq!(
            check("what does /code init do?", reply),
            RetrievalCheck::MissingCitation,
        );
    }

    #[test]
    fn github_issue_link_counts_as_citation() {
        let reply = "Known issue — see github_issue:42";
        assert_eq!(
            check("is this a known kiro-cli bug?", reply),
            RetrievalCheck::Cited,
        );
    }
}
