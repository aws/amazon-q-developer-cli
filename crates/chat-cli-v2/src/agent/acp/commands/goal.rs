use agent::goal::GoalDefinition;
use agent::tui_commands::{
    CommandResult,
    GoalArgs,
};

use super::{
    CommandContext,
    shell_split,
};

/// Goal action encoded in CommandResult.data for the caller to handle.
const GOAL_ACTION_KEY: &str = "goal_action";

/// Maximum length of a goal description, in chars. Anything longer is almost certainly a
/// paste accident. The description gets inlined into both the initial and nudge prompts,
/// so an oversized description costs tokens on every iteration.
const MAX_GOAL_DESCRIPTION_CHARS: usize = 4_000;

/// Hard ceiling on `--max` to prevent runaway loops or accidental --max 999999.
const MAX_GOAL_ITERATIONS: u32 = 50;

pub async fn execute(args: &GoalArgs, ctx: &CommandContext<'_>) -> CommandResult {
    let Some(ref input) = args.subcommand else {
        // No args: open goal panel (TUI reads from store)
        return CommandResult::success("");
    };

    let parts = shell_split(input);
    let first = parts.first().map_or("", |s| s.as_str());

    match first {
        "clear" => handle_clear(ctx),
        _ => handle_set_goal(input),
    }
}

fn handle_set_goal(input: &str) -> CommandResult {
    let args = shell_split(input);
    let mut max: u32 = 5;

    // Only parse --max if it's the second-to-last token (avoids eating
    // "--max" that appears inside the description, e.g. "fix the --max flag").
    let desc_args = if args.len() >= 2
        && args[args.len() - 2] == "--max"
        && let Ok(n) = args[args.len() - 1].parse::<u32>()
    {
        max = n;
        &args[..args.len() - 2]
    } else {
        &args[..]
    };

    let description = desc_args.join(" ");

    if description.is_empty() {
        return CommandResult::error("Usage: /goal <description> [--max <N>]");
    }
    let desc_chars = description.chars().count();
    if desc_chars > MAX_GOAL_DESCRIPTION_CHARS {
        return CommandResult::error(format!(
            "Goal description is too long ({} chars). Keep it under {} chars.",
            desc_chars, MAX_GOAL_DESCRIPTION_CHARS
        ));
    }
    if max == 0 {
        return CommandResult::error("--max must be at least 1");
    }
    if max > MAX_GOAL_ITERATIONS {
        return CommandResult::error(format!(
            "--max {} exceeds the {} iteration ceiling. Use a lower value or break the goal into pieces.",
            max, MAX_GOAL_ITERATIONS
        ));
    }

    let definition = GoalDefinition {
        description: description.clone(),
        max_iterations: max,
    };

    let display_desc = if description.chars().count() > 120 {
        format!("{}...", description.chars().take(117).collect::<String>())
    } else {
        description.clone()
    };
    CommandResult::success_with_data(
        format!("goal set: \"{display_desc}\" (max {max})"),
        serde_json::json!({
            GOAL_ACTION_KEY: "set",
            "definition": definition,
            "label": format!("/goal {}", description)
        }),
    )
}

fn handle_clear(_ctx: &CommandContext<'_>) -> CommandResult {
    CommandResult::success_with_data("Goal cleared", serde_json::json!({ GOAL_ACTION_KEY: "clear" }))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn free_text_without_quotes() {
        let result = handle_set_goal("implement pagination for users");
        assert!(result.data.is_some());
        let data = result.data.unwrap();
        assert_eq!(data["definition"]["description"], "implement pagination for users");
    }

    #[test]
    fn quoted_text_still_works() {
        let result = handle_set_goal("\"implement pagination for users\"");
        assert!(result.data.is_some());
        let data = result.data.unwrap();
        assert_eq!(data["definition"]["description"], "implement pagination for users");
    }

    #[test]
    fn free_text_with_max_flag() {
        let result = handle_set_goal("fix all tests --max 10");
        assert!(result.data.is_some());
        let data = result.data.unwrap();
        assert_eq!(data["definition"]["description"], "fix all tests");
        assert_eq!(data["definition"]["max_iterations"], 10);
    }

    #[test]
    fn free_text_with_all_flags() {
        let result = handle_set_goal("build the API --max 8");
        assert!(result.data.is_some());
        let data = result.data.unwrap();
        assert_eq!(data["definition"]["description"], "build the API");
        assert_eq!(data["definition"]["max_iterations"], 8);
    }

    #[test]
    fn max_in_middle_is_part_of_description() {
        let result = handle_set_goal("fix the --max flag in my CLI");
        assert!(result.data.is_some());
        let data = result.data.unwrap();
        assert_eq!(data["definition"]["description"], "fix the --max flag in my CLI");
        assert_eq!(data["definition"]["max_iterations"], 5); // default
    }

    #[test]
    fn empty_input_returns_error() {
        let result = handle_set_goal("");
        assert!(!result.success);
    }

    #[test]
    fn whitespace_only_returns_error() {
        let result = handle_set_goal("   \t  ");
        assert!(!result.success);
    }

    #[test]
    fn multi_word_with_extra_internal_whitespace() {
        // shell_split handles internal whitespace by tokenizing — multiple spaces collapse
        let result = handle_set_goal("hello    world");
        assert!(result.data.is_some());
        let data = result.data.unwrap();
        assert_eq!(data["definition"]["description"], "hello world");
    }

    #[test]
    fn only_flags_returns_error() {
        let result = handle_set_goal("--max 5");
        assert!(!result.success);
    }

    #[test]
    fn description_too_long_returns_error() {
        let huge = "a".repeat(MAX_GOAL_DESCRIPTION_CHARS + 1);
        let result = handle_set_goal(&huge);
        assert!(!result.success);
        assert!(result.message.contains("too long"));
    }

    #[test]
    fn description_at_limit_succeeds() {
        let at_limit = "a".repeat(MAX_GOAL_DESCRIPTION_CHARS);
        let result = handle_set_goal(&at_limit);
        assert!(result.success);
    }

    #[test]
    fn max_zero_returns_error() {
        let result = handle_set_goal("do thing --max 0");
        assert!(!result.success);
        assert!(result.message.contains("at least 1"));
    }

    #[test]
    fn max_above_ceiling_returns_error() {
        let result = handle_set_goal(&format!("do thing --max {}", MAX_GOAL_ITERATIONS + 1));
        assert!(!result.success);
        assert!(result.message.contains("ceiling"));
    }

    #[test]
    fn max_at_ceiling_succeeds() {
        let result = handle_set_goal(&format!("do thing --max {}", MAX_GOAL_ITERATIONS));
        assert!(result.success);
    }
}
