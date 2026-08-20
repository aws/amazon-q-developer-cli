// kiro-help bot Cedar policy. The bot answers in DMs and in exactly one
// channel: whichever id the CDK stack injects as ALLOWED_CHANNEL_ID for the
// stage. Everything else is denied.
//
// This is a TEMPLATE, and it does parse as-is: the placeholder below sits inside
// a Cedar string literal, where `$`, `{` and `}` are ordinary characters. So an
// unsubstituted policy fails SILENTLY rather than loudly — it denotes a channel
// id no Slack channel can equal, and the bot answers in DMs only. The grep pair
// in docker/entrypoint.sh is what makes that fail closed, before and after
// substitution; there is no parse-time safety net. Rust tests substitute it the
// same way (see engine::authz). Adding a channel is still a redeploy; SSM-backed
// dynamic allowlisting is deferred to v1.5 per the roadmap.
//
// The placeholder token deliberately appears exactly once, in the policy body:
// the entrypoint substitutes globally, so a second copy in a comment would be
// rewritten into a confusing channel id in the deployed file.

@id("allow-kiro-help-dms")
permit(
  principal,
  action == Action::"use_bot",
  resource
) when {
  context.is_dm == true
};

@id("allow-kiro-help-allowed-channel")
permit(
  principal,
  action == Action::"use_bot",
  resource == Conversation::"channel:${ALLOWED_CHANNEL_ID}"
);
