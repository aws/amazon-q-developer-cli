//! Rendering for context window information

use crossterm::style::Attribute;
use crossterm::{
    execute,
    queue,
    style,
};

use super::context_data_provider::ContextWindowData;
use crate::cli::chat::token_counter::TokenCount;
use crate::cli::chat::{
    ChatError,
    ChatSession,
};
use crate::theme::{
    StyledText,
    theme,
};

/// Component percentages for context window breakdown
#[derive(Debug, Clone, Copy)]
struct ComponentPercentages {
    context: f32,
    tools: f32,
    assistant: f32,
    user: f32,
}

#[cfg(test)]
impl ComponentPercentages {
    /// Calculate total percentage (sum of all components)
    fn total(&self) -> f32 {
        self.context + self.tools + self.assistant + self.user
    }
}

/// Calculate context percentage from token counts (private utility)
///
/// Note: This calculates percentage based on estimated token counts.
/// For more accurate results, use `scale_component_percentage` when backend total is available.
fn calculate_context_percentage(tokens: TokenCount, context_window_size: usize) -> f32 {
    (tokens.value() as f32 / context_window_size as f32) * 100.0
}

/// Adjust component percentages to match backend total
///
/// Strategy: Always keep context + tools stable (they tokenize predictably)
/// and fill the remaining percentage to user + assistant (where char-based estimates
/// are unreliable, especially for repetitive text).
///
/// This works whether estimates are too high or too low compared to backend.
///
/// # Arguments
/// * `context_tokens` - Estimated context file tokens
/// * `tools_tokens` - Estimated tool tokens
/// * `assistant_tokens` - Estimated assistant message tokens
/// * `user_tokens` - Estimated user message tokens
/// * `context_window_size` - Size of the context window
/// * `backend_total_pct` - Accurate total percentage from backend
///
/// # Returns
/// Adjusted component percentages that sum to backend_total_pct
fn adjust_component_percentages(
    context_tokens: TokenCount,
    tools_tokens: TokenCount,
    assistant_tokens: TokenCount,
    user_tokens: TokenCount,
    context_window_size: usize,
    backend_total_pct: f32,
) -> ComponentPercentages {
    // Calculate initial estimates for all components
    let context_pct = calculate_context_percentage(context_tokens, context_window_size);
    let tools_pct = calculate_context_percentage(tools_tokens, context_window_size);
    let assistant_pct = calculate_context_percentage(assistant_tokens, context_window_size);
    let user_pct = calculate_context_percentage(user_tokens, context_window_size);

    // Strategy: Always keep context + tools stable (they tokenize predictably)
    // Fill remaining to user + assistant (where char-based estimates are unreliable)
    let stable_total = context_pct + tools_pct;

    if stable_total == 0.0 && backend_total_pct == 0.0 {
        return ComponentPercentages {
            context: 0.0,
            tools: 0.0,
            assistant: 0.0,
            user: 0.0,
        };
    }

    let remaining = backend_total_pct - stable_total;

    // Edge case: stable components exceed backend (shouldn't happen in practice)
    if remaining < 0.0 {
        let scale_factor = backend_total_pct / stable_total;
        return ComponentPercentages {
            context: context_pct * scale_factor,
            tools: tools_pct * scale_factor,
            assistant: 0.0,
            user: 0.0,
        };
    }

    // Distribute remaining to user + assistant, preserving their ratio
    let variable_estimate = assistant_pct + user_pct;
    let (assistant_final, user_final) = if variable_estimate > 0.0 {
        let assistant_ratio = assistant_pct / variable_estimate;
        (remaining * assistant_ratio, remaining * (1.0 - assistant_ratio))
    } else {
        (0.0, remaining)
    };

    ComponentPercentages {
        context: context_pct,
        tools: tools_pct,
        assistant: assistant_final,
        user: user_final,
    }
}

/// Render context window information section
pub async fn render_context_window(
    context_data: &ContextWindowData,
    session: &mut ChatSession,
) -> Result<(), ChatError> {
    if !context_data.dropped_context_files.is_empty() {
        execute!(
            session.stderr,
            StyledText::warning_fg(),
            style::Print("\nSome context files are dropped due to size limit, please run "),
            StyledText::success_fg(),
            style::Print("/context show "),
            StyledText::warning_fg(),
            style::Print("to learn more.\n"),
            StyledText::reset(),
        )?;
    }

    let window_width = session.terminal_width();
    // set a max width for the progress bar for better aesthetic
    let progress_bar_width = std::cmp::min(window_width, 80);

    // Calculate percentages - use backend-scaled if available, otherwise use estimates
    let (context_pct, tools_pct, assistant_pct, user_pct, total_pct) =
        if let Some(backend_pct) = context_data.backend_total_percentage {
            // Adjust components: keep context/tools stable, fill remaining to user+assistant
            let adjusted = adjust_component_percentages(
                context_data.context_tokens,
                context_data.tools_tokens,
                context_data.assistant_tokens,
                context_data.user_tokens,
                context_data.context_window_size,
                backend_pct,
            );

            (
                adjusted.context,
                adjusted.tools,
                adjusted.assistant,
                adjusted.user,
                backend_pct,
            )
        } else {
            // Fall back to estimates when backend value not available
            let calc = |tokens| calculate_context_percentage(tokens, context_data.context_window_size);
            (
                calc(context_data.context_tokens),
                calc(context_data.tools_tokens),
                calc(context_data.assistant_tokens),
                calc(context_data.user_tokens),
                calc(context_data.total_tokens),
            )
        };

    // Calculate bar widths - use scaled percentages if available to match displayed percentages
    let (context_width, tools_width, assistant_width, user_width) = if context_data.backend_total_percentage.is_some() {
        // Use scaled percentages for bar widths to match displayed values
        (
            ((context_pct / 100.0) * progress_bar_width as f32) as usize,
            ((tools_pct / 100.0) * progress_bar_width as f32) as usize,
            ((assistant_pct / 100.0) * progress_bar_width as f32) as usize,
            ((user_pct / 100.0) * progress_bar_width as f32) as usize,
        )
    } else {
        // Use raw token estimates for bar widths (fallback when no backend data)
        (
            ((context_data.context_tokens.value() as f64 / context_data.context_window_size as f64)
                * progress_bar_width as f64) as usize,
            ((context_data.tools_tokens.value() as f64 / context_data.context_window_size as f64)
                * progress_bar_width as f64) as usize,
            ((context_data.assistant_tokens.value() as f64 / context_data.context_window_size as f64)
                * progress_bar_width as f64) as usize,
            ((context_data.user_tokens.value() as f64 / context_data.context_window_size as f64)
                * progress_bar_width as f64) as usize,
        )
    };

    // Calculate tiny indicators for sections with tokens but 0 width
    let context_tiny = if context_width == 0 && context_data.context_tokens.value() > 0 {
        1
    } else {
        0
    };
    let tools_tiny = if tools_width == 0 && context_data.tools_tokens.value() > 0 {
        1
    } else {
        0
    };
    let assistant_tiny = if assistant_width == 0 && context_data.assistant_tokens.value() > 0 {
        1
    } else {
        0
    };
    let user_tiny = if user_width == 0 && context_data.user_tokens.value() > 0 {
        1
    } else {
        0
    };
    let total_tiny = context_tiny + tools_tiny + assistant_tiny + user_tiny;

    let left_over_width = progress_bar_width
        - std::cmp::min(
            context_width + assistant_width + user_width + tools_width + total_tiny,
            progress_bar_width,
        );

    let is_overflow = (context_width + assistant_width + user_width + tools_width + total_tiny) > progress_bar_width;

    // Format the header differently based on whether we have backend data
    let header = if context_data.backend_total_percentage.is_none() {
        format!("\nContext window: {total_pct:.1}% used (estimated)\n")
    } else {
        format!("\nContext window: {total_pct:.1}% used\n")
    };

    if is_overflow {
        queue!(
            session.stderr,
            style::Print(header),
            StyledText::error_fg(),
            style::Print("█".repeat(progress_bar_width)),
            StyledText::reset(),
            style::Print(" "),
            style::Print(format!("{total_pct:.1}%")),
        )?;
    } else {
        queue!(
            session.stderr,
            style::Print(header),
            // Context files
            StyledText::brand_fg(),
            // add a nice visual to mimic "tiny" progress, so the overrall progress bar doesn't look too
            // empty
            style::Print("|".repeat(context_tiny)),
            style::Print("█".repeat(context_width)),
            // Tools
            StyledText::error_fg(),
            style::Print("|".repeat(tools_tiny)),
            style::Print("█".repeat(tools_width)),
            // Assistant responses
            StyledText::info_fg(),
            style::Print("|".repeat(assistant_tiny)),
            style::Print("█".repeat(assistant_width)),
            // User prompts
            StyledText::emphasis_fg(),
            style::Print("|".repeat(user_tiny)),
            style::Print("█".repeat(user_width)),
            StyledText::secondary_fg(),
            style::Print("█".repeat(left_over_width)),
            style::Print(" "),
            StyledText::reset(),
            style::Print(format!("{total_pct:.1}%")),
        )?;
    }

    execute!(session.stderr, style::Print("\n\n"))?;

    queue!(
        session.stderr,
        StyledText::brand_fg(),
        style::Print("█ Context files "),
        StyledText::secondary_fg(),
        style::Print(format!("{context_pct:.1}% (estimated)\n")),
        StyledText::error_fg(),
        style::Print("█ Tools "),
        StyledText::secondary_fg(),
        style::Print(format!("{tools_pct:.1}% (estimated)\n")),
        StyledText::info_fg(),
        style::Print("█ Kiro responses "),
        StyledText::secondary_fg(),
        style::Print(format!("{assistant_pct:.1}% (estimated)\n")),
        StyledText::emphasis_fg(),
        style::Print("█ Your prompts "),
        StyledText::secondary_fg(),
        style::Print(format!("{user_pct:.1}% (estimated)\n\n")),
        StyledText::reset(),
    )?;

    queue!(
        session.stderr,
        style::SetAttribute(Attribute::Bold),
        style::Print("\n💡 Pro Tips:\n"),
        StyledText::reset_attributes(),
        StyledText::secondary_fg(),
        style::Print("Run "),
        StyledText::reset(),
        style::SetForegroundColor(theme().ui.command_highlight),
        style::Print("/compact"),
        StyledText::reset(),
        StyledText::secondary_fg(),
        style::Print(" to replace the conversation history with its summary\n"),
        style::Print("Run "),
        StyledText::reset(),
        style::SetForegroundColor(theme().ui.command_highlight),
        style::Print("/clear"),
        StyledText::reset(),
        StyledText::secondary_fg(),
        style::Print(" to erase the entire chat history\n"),
        style::Print("Run "),
        StyledText::reset(),
        style::SetForegroundColor(theme().ui.command_highlight),
        style::Print("/context show"),
        StyledText::reset(),
        StyledText::secondary_fg(),
        style::Print(" to see usage per context file\n\n"),
        StyledText::reset(),
    )?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_adjust_component_gap_filling() {
        // Backend says 80%, estimates say 50% (underestimate)
        // Context: 5%, Tools: 15%, Assistant: 20%, User: 10% = 50%
        // Stable: 20%, Remaining: 60% distributed to assistant+user (2:1 ratio)
        let context_window = 10000;
        let context = TokenCount::from_raw(500); // 5%
        let tools = TokenCount::from_raw(1500); // 15%
        let assistant = TokenCount::from_raw(2000); // 20%
        let user = TokenCount::from_raw(1000); // 10%

        let adjusted = adjust_component_percentages(context, tools, assistant, user, context_window, 80.0);

        // Context and tools should stay the same
        assert!(
            (adjusted.context - 5.0).abs() < 0.01,
            "Context should be ~5%, got {}",
            adjusted.context
        );
        assert!(
            (adjusted.tools - 15.0).abs() < 0.01,
            "Tools should be ~15%, got {}",
            adjusted.tools
        );

        // Remaining 60% distributed 2:1 (assistant:user) = 40% + 20%
        assert!(
            (adjusted.assistant - 40.0).abs() < 0.1,
            "Expected ~40%, got {}",
            adjusted.assistant
        );
        assert!(
            (adjusted.user - 20.0).abs() < 0.1,
            "Expected ~20%, got {}",
            adjusted.user
        );

        // Sum should equal backend total
        assert!((adjusted.total() - 80.0).abs() < 0.001);
    }

    #[test]
    fn test_adjust_component_overestimate() {
        // Backend says 40%, estimates say 80% (overestimate from repetitive text)
        // Context: 10%, Tools: 30%, Assistant: 20%, User: 20% = 80%
        // Stable: 40%, Remaining: 0% means context+tools fill it all
        let context_window = 10000;
        let context = TokenCount::from_raw(1000); // 10%
        let tools = TokenCount::from_raw(3000); // 30%
        let assistant = TokenCount::from_raw(2000); // 20%
        let user = TokenCount::from_raw(2000); // 20%

        let adjusted = adjust_component_percentages(context, tools, assistant, user, context_window, 40.0);

        // Context and tools should stay stable
        assert!(
            (adjusted.context - 10.0).abs() < 0.01,
            "Context should be ~10%, got {}",
            adjusted.context
        );
        assert!(
            (adjusted.tools - 30.0).abs() < 0.01,
            "Tools should be ~30%, got {}",
            adjusted.tools
        );

        // Remaining is 0%, so user+assistant get nothing
        assert!((adjusted.assistant - 0.0).abs() < 0.1);
        assert!((adjusted.user - 0.0).abs() < 0.1);

        assert!((adjusted.total() - 40.0).abs() < 0.001);
    }

    #[test]
    fn test_adjust_component_zero_variable() {
        // Edge case: no user or assistant messages, remaining goes to user
        let context_window = 10000;
        let context = TokenCount::from_raw(500); // 5%
        let tools = TokenCount::from_raw(1000); // 10%
        let assistant = TokenCount::from_raw(0); // 0%
        let user = TokenCount::from_raw(0); // 0%

        let adjusted = adjust_component_percentages(context, tools, assistant, user, context_window, 60.0);

        // Context and tools stay stable
        assert!(
            (adjusted.context - 5.0).abs() < 0.01,
            "Context should be ~5%, got {}",
            adjusted.context
        );
        assert!(
            (adjusted.tools - 10.0).abs() < 0.01,
            "Tools should be ~10%, got {}",
            adjusted.tools
        );
        // Remaining (45%) goes to user
        assert!(
            (adjusted.assistant - 0.0).abs() < 0.01,
            "Assistant should be ~0%, got {}",
            adjusted.assistant
        );
        assert!(
            (adjusted.user - 45.0).abs() < 0.1,
            "User should get the remaining, got {}",
            adjusted.user
        );

        assert!((adjusted.total() - 60.0).abs() < 0.001);
    }

    #[test]
    fn test_adjust_component_zero_all() {
        // Edge case: all zeros
        let context_window = 10000;
        let adjusted = adjust_component_percentages(
            TokenCount::from_raw(0),
            TokenCount::from_raw(0),
            TokenCount::from_raw(0),
            TokenCount::from_raw(0),
            context_window,
            0.0,
        );

        assert_eq!(adjusted.context, 0.0);
        assert_eq!(adjusted.tools, 0.0);
        assert_eq!(adjusted.assistant, 0.0);
        assert_eq!(adjusted.user, 0.0);
    }

    #[test]
    fn test_adjust_component_realistic_underestimate() {
        // Real scenario: estimates are low, backend is high
        // Estimates: context 0.1%, tools 18.9%, assistant 0%, user 0% = 19%
        // Backend: 74.5%
        // Remaining 55.5% should go to user (assistant is 0)
        let context_window = 200000;
        let context = TokenCount::from_raw(200); // ~0.1%
        let tools = TokenCount::from_raw(37800); // ~18.9%
        let assistant = TokenCount::from_raw(0); // 0%
        let user = TokenCount::from_raw(0); // 0%

        let adjusted = adjust_component_percentages(context, tools, assistant, user, context_window, 74.5);

        // Context and tools should remain stable
        assert!(
            (adjusted.context - 0.1).abs() < 0.1,
            "Context should stay ~0.1%, got {}",
            adjusted.context
        );
        assert!(
            (adjusted.tools - 18.9).abs() < 0.5,
            "Tools should stay ~18.9%, got {}",
            adjusted.tools
        );

        // Remaining should go to user (since assistant is 0)
        assert!(
            adjusted.user > 50.0,
            "User should get most of the remaining, got {}%",
            adjusted.user
        );

        assert!((adjusted.total() - 74.5).abs() < 0.1);
    }

    #[test]
    fn test_adjust_component_realistic_overestimate() {
        // Real scenario from user report: repetitive text causes overestimate
        // Estimates: context 0.1%, tools 18.9%, assistant 0.1%, user 100% = 119.1%
        // Backend: 74.5%
        // Stable: 19%, Remaining: 55.5% distributed to assistant+user
        let context_window = 200000;
        let context = TokenCount::from_raw(250); // ~0.1%
        let tools = TokenCount::from_raw(37790); // ~18.9%
        let assistant = TokenCount::from_raw(180); // ~0.1%
        let user = TokenCount::from_raw(200060); // ~100% (inflated!)

        let adjusted = adjust_component_percentages(context, tools, assistant, user, context_window, 74.5);

        // Context and tools MUST remain stable (the key fix!)
        assert!(
            (adjusted.context - 0.1).abs() < 0.1,
            "Context should stay ~0.1%, got {}",
            adjusted.context
        );
        assert!(
            (adjusted.tools - 18.9).abs() < 0.5,
            "Tools should stay ~18.9%, got {}",
            adjusted.tools
        );

        // Remaining 55.5% distributed to assistant+user based on their ratio
        // Original ratio: 180:200060 ≈ 0:100, so almost all goes to user
        assert!(
            adjusted.user > 50.0,
            "User should get most of remaining, got {}%",
            adjusted.user
        );
        assert!(
            adjusted.assistant < 1.0,
            "Assistant should get minimal amount, got {}%",
            adjusted.assistant
        );

        assert!((adjusted.total() - 74.5).abs() < 0.1);
    }

    #[test]
    fn test_adjust_component_tools_stay_stable() {
        // Verify tools percentage doesn't drop when backend < estimate
        let context_window = 10000;
        let context = TokenCount::from_raw(100); // 1%
        let tools = TokenCount::from_raw(2000); // 20%
        let assistant = TokenCount::from_raw(500); // 5%
        let user = TokenCount::from_raw(400); // 4%
        // Total estimate: 30%

        let tools_estimate = calculate_context_percentage(tools, context_window);

        let adjusted = adjust_component_percentages(context, tools, assistant, user, context_window, 75.0);

        // Tools should NOT decrease (was the original problem)
        assert!(
            adjusted.tools >= tools_estimate - 0.01,
            "Tools should stay stable at {:.1}%, got {:.1}%",
            tools_estimate,
            adjusted.tools
        );
    }

    #[test]
    fn test_adjust_component_preserves_user_assistant_ratio() {
        // When filling remaining, maintain ratio between user and assistant
        let context_window = 10000;
        let context = TokenCount::from_raw(100); // 1%
        let tools = TokenCount::from_raw(1000); // 10%
        let assistant = TokenCount::from_raw(2000); // 20%
        let user = TokenCount::from_raw(1000); // 10%
        // Total estimate: 41%, backend: 81%, stable: 11%, remaining: 70%

        let adjusted = adjust_component_percentages(context, tools, assistant, user, context_window, 81.0);

        // Original assistant:user ratio is 2:1 (20%:10%)
        // After distributing remaining, ratio should be preserved
        let original_ratio = 20.0 / 10.0;
        let adjusted_ratio = adjusted.assistant / adjusted.user;

        assert!(
            (original_ratio - adjusted_ratio).abs() < 0.01,
            "Ratio should be preserved: expected {}, got {}",
            original_ratio,
            adjusted_ratio
        );
    }

    #[test]
    fn test_bar_width_calculation_with_backend_percentage() {
        // Test that bar widths match displayed percentages
        let progress_bar_width = 80;
        let context_window = 10000;

        let context = TokenCount::from_raw(100);
        let tools = TokenCount::from_raw(1000);
        let assistant = TokenCount::from_raw(2000);
        let user = TokenCount::from_raw(1000);

        let adjusted = adjust_component_percentages(context, tools, assistant, user, context_window, 74.5);

        // Bar widths calculated from percentages
        let context_width = ((adjusted.context / 100.0) * progress_bar_width as f32) as usize;
        let tools_width = ((adjusted.tools / 100.0) * progress_bar_width as f32) as usize;
        let assistant_width = ((adjusted.assistant / 100.0) * progress_bar_width as f32) as usize;
        let user_width = ((adjusted.user / 100.0) * progress_bar_width as f32) as usize;

        let total_bar_width = context_width + tools_width + assistant_width + user_width;
        let expected_total_width = ((74.5 / 100.0) * progress_bar_width as f32) as usize;

        // Bar width should match backend percentage (allow ±2 for rounding)
        assert!(
            (total_bar_width as i32 - expected_total_width as i32).abs() <= 2,
            "Bar width {} should match expected {}",
            total_bar_width,
            expected_total_width
        );
    }

    #[test]
    fn test_estimated_qualifier_with_backend_data() {
        // When backend data is available, no "(estimated)" qualifier should be added
        let backend_data = ContextWindowData {
            backend_total_percentage: Some(74.5),
            context_tokens: TokenCount::from_raw(100),
            tools_tokens: TokenCount::from_raw(1000),
            assistant_tokens: TokenCount::from_raw(2000),
            user_tokens: TokenCount::from_raw(1000),
            total_tokens: TokenCount::from_raw(4100),
            context_window_size: 10000,
            dropped_context_files: vec![],
        };

        // The qualifier should be empty when backend data exists
        let qualifier = if backend_data.backend_total_percentage.is_none() {
            " (estimated)"
        } else {
            ""
        };

        assert_eq!(
            qualifier, "",
            "Should not show (estimated) when backend data is available"
        );
    }

    #[test]
    fn test_estimated_qualifier_without_backend_data() {
        // When backend data is NOT available, "(estimated)" qualifier should be added
        let fallback_data = ContextWindowData {
            backend_total_percentage: None,
            context_tokens: TokenCount::from_raw(100),
            tools_tokens: TokenCount::from_raw(1000),
            assistant_tokens: TokenCount::from_raw(2000),
            user_tokens: TokenCount::from_raw(1000),
            total_tokens: TokenCount::from_raw(4100),
            context_window_size: 10000,
            dropped_context_files: vec![],
        };

        // The qualifier should be present when backend data doesn't exist
        let qualifier = if fallback_data.backend_total_percentage.is_none() {
            " (estimated)"
        } else {
            ""
        };

        assert_eq!(
            qualifier, " (estimated)",
            "Should show (estimated) when backend data is not available"
        );
    }
}
