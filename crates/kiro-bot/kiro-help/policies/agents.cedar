// kiro-help bot Cedar policy.
//
// Phase 6: replace the Phase 1/2 "allow-all" placeholder with a tight
// allowlist. The bot can answer in:
//
// 1. DMs — any Conversation entity whose EID starts with "dm:". Anyone DMing
//    the bot is asking on purpose, no extra gating.
// 2. The kiro-help beta channel.
// 3. The kiro-help prod channel.
//
// Anything else is denied. Adding a new channel = redeploy. SSM-backed dynamic
// allowlisting (no redeploy on channel add) is deferred to v1.5 per the
// roadmap. Channel-IDs below are placeholders — replace with the real Slack
// channel IDs before the first prod deploy.

@id("allow-kiro-help-dms")
permit(
  principal,
  action == Action::"use_bot",
  resource
) when {
  context.is_dm == true
};

@id("allow-kiro-help-beta-channel")
permit(
  principal,
  action == Action::"use_bot",
  resource == Conversation::"channel:C0KIROHELPBETA"
);

@id("allow-kiro-help-prod-channel")
permit(
  principal,
  action == Action::"use_bot",
  resource == Conversation::"channel:C0KIROHELPPROD"
);
