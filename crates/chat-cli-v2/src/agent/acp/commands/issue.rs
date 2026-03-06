//! /issue command execution

use agent::tui_commands::CommandResult;

const ISSUE_TEMPLATE_URL: &str = "https://taskei.amazon.dev/tasks/create?template=c0312360-3f55-432d-a6d2-e3060ad2cc59";

pub async fn execute() -> CommandResult {
    match crate::util::open::open_url_async(ISSUE_TEMPLATE_URL).await {
        Ok(()) => CommandResult::success("Opening issue form in browser..."),
        Err(_) => CommandResult::success_with_data("", serde_json::json!({ "url": ISSUE_TEMPLATE_URL })),
    }
}

// TODO: Re-enable diagnostics prefill once URL length issues are resolved (403 errors).
// This would be used for GitHub public users.
//
// use super::CommandContext;
// use crate::os::diagnostics::Diagnostics;
//
// pub async fn execute_with_diagnostics(ctx: &CommandContext<'_>) -> CommandResult {
//     let diagnostics = Diagnostics::new(ctx.env).await;
//     let system_details = diagnostics.user_readable().unwrap_or_default();
//
//     let description = format!(
//         "### Feedback\n\n\n### Replication Steps/Screenshots\n\n\n### System
// Details\n\n```\n{system_details}\n```"     );
//
//     let url = format!(
//         "{ISSUE_TEMPLATE_URL}&description={}",
//         urlencoding::encode(&description)
//     );
//
//     match crate::util::open::open_url_async(&url).await {
//         Ok(()) => CommandResult::success("Opening issue form in browser..."),
//         Err(_) => CommandResult::success_with_data(
//             "Could not open browser. Copy the URL below.",
//             serde_json::json!({ "url": url }),
//         ),
//     }
// }
