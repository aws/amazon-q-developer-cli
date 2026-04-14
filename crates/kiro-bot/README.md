# kiro-bot

Bot runtime for Kiro CLI. Manages ACP-backed agent instances as background daemons with pluggable frontends — interactive Slack bots, scheduled headless jobs, or one-shot CLI tasks.

```
┌──────────┐
│  Slack   │──┐
│  (WS)    │  │     ┌────────────────┐     ┌──────────┐
└──────────┘  │     │                │     │          │
┌──────────┐  ├────▶│   kiro-bot     │────▶│ kiro-cli │
│  Cron    │──┤     │                │     │   acp    │
│ (schedule)  │     └────────────────┘     └──────────┘
└──────────┘  │      dispatch + authz        Agent
┌──────────┐  │
│  CLI     │──┘
│ (stdin)  │
└──────────┘
```

### Frontends

| Frontend | Type | Use case |
|----------|------|----------|
| **Slack** | `type = "slack"` | Interactive bot — Socket Mode, threads, @mentions, tool approval via reactions |
| **Cron** | `type = "cron"` | Scheduled or one-shot headless jobs — run a prompt on an interval or cron expression |
| **CLI** | `kiro-bot chat` | Interactive terminal — for local testing without Slack |

## Quick Start

```bash
# 1. Build
cargo build -p kiro-bot

# 2. Create a config directory (see Config below)
mkdir my-bot && cd my-bot
# ... create config.toml and policies/agents.cedar

# 3. Install (prompts for Slack tokens)
kiro-bot install ./my-bot

# 4. Start
kiro-bot start my-bot

# 5. Check status
kiro-bot status
```

## Commands

| Command | Description |
|---------|-------------|
| `kiro-bot install <path>` | Install from a config directory (copies config, prompts for secrets) |
| `kiro-bot uninstall <name>` | Remove an instance (stops if running, deletes config) |
| `kiro-bot start <name>` | Start as a background daemon |
| `kiro-bot start --all` | Start all installed instances |
| `kiro-bot start --foreground <name>` | Run attached to terminal (for debugging) |
| `kiro-bot stop <name>` | Stop a running instance |
| `kiro-bot stop --all` | Stop all running instances |
| `kiro-bot status` | List all instances with running/stopped state |
| `kiro-bot chat <name>` | Interactive CLI chat (no Slack needed) |

## Slack App Setup

### 1. Create the App

Go to [api.slack.com/apps](https://api.slack.com/apps) → **Create New App** → **From scratch**.

### 2. Enable Socket Mode

- **Settings → Socket Mode** → Toggle **Enable Socket Mode**
- Generate an **App-Level Token** with the `connections:write` scope
- Save the token (`xapp-...`) — you'll need it during install

### 3. Add Bot Token Scopes

**OAuth & Permissions → Scopes → Bot Token Scopes:**

| Scope | Purpose |
|-------|---------|
| `app_mentions:read` | Detect @mentions in channels |
| `chat:write` | Send and update messages |
| `im:history` | Read DM message history (for conversation context) |
| `im:read` | View DM metadata |
| `im:write` | Open DMs with users |
| `reactions:read` | Read emoji reactions (for tool approval) |
| `reactions:write` | Add emoji reactions to messages |
| `channels:history` | Read channel messages (for conversation context) |

### 4. Subscribe to Bot Events

**Event Subscriptions → Subscribe to bot events:**

| Event | Purpose |
|-------|---------|
| `app_mention` | Respond when @mentioned in channels |
| `message.im` | Respond to direct messages |
| `reaction_added` | Tool approval via ✅/❌/🔓 reactions |
| `reaction_removed` | Handle approval retraction |

### 5. Install to Workspace

- **Install App** → **Install to Workspace**
- Copy the **Bot User OAuth Token** (`xoxb-...`)

### 6. Get the Bot's Member ID

- Invite the bot to any channel
- @mention it, then click the bot's name in the message
- Copy the **Member ID** (starts with `U`) from the profile popup
- This goes in `bot_member_id` in your config

## Config

Each instance is a directory with `config.toml`, `secrets.toml`, and optionally `policies/`.

### config.toml

```toml
name = "my-bot"
working_directory = "/path/to/agent/workspace"

[frontend]
type = "slack"
bot_name = "MyBot"
bot_member_id = "U0AHNCJD8MV"    # from step 6 above
conversation_history = 10          # include N recent messages as context

[agent]
command = "kiro-cli acp"
model = "claude-opus-4.6-1m"
default_mode = "my-agent"          # optional: default agent/mode
approval_policy = "approve"        # approve | ask | deny
max_workers = 5                    # concurrent ACP sessions
idle_timeout_secs = 300            # reap idle workers after N seconds
mcp_wait_ms = 2000                 # wait for MCP servers to initialize

[authorization]
cedar_policy_file = "policies/agents.cedar"

# Map Slack user IDs to human-readable names (used in Cedar policies)
[users]
W017EQCFCUE = "alice"
U029HUMU2TG = "bob"

# Response policies control when and where the bot replies
[[response_policies]]
conversation = "dm:*"              # all DMs
trigger = "always"                 # always respond
reply = "inline"                   # reply in the same conversation

[[response_policies]]
conversation = "*"                 # everything else
trigger = "directed_only"          # only when @mentioned
reply = "thread"                   # reply in a thread
```

### secrets.toml

Created automatically during `kiro-bot install`. Never version-control this file.

```toml
[slack]
bot_token = "xoxb-..."
app_token = "xapp-..."
```

### policies/agents.cedar

Cedar policies control who can use the bot. Without policies, everyone has access.

```cedar
// Allow a specific user everywhere
@id("allow-alice")
permit(
  principal == User::"alice",
  action == Action::"use_bot",
  resource
);

// Allow anyone in a specific channel
@id("allow-channel")
permit(
  principal,
  action == Action::"use_bot",
  resource == Conversation::"channel:C0123456789"
);
```

**Actions:** `use_bot`, `use_agent`, `use_model`
**Resource types:** `Conversation` (`"dm:alice"`, `"channel:C123"`), `Agent`, `Model`

## Bot Commands (in Slack)

| Command | Description |
|---------|-------------|
| `!help` | Show available commands |
| `!new` | Start a new session |
| `!agent <name>` | Switch agent/mode |
| `!model <name>` | Switch model |
| `!status` | Show current agent, model, session info |
| `!agents` | List available agents |
| `!cancel` | Cancel in-progress request |

Any other message is sent to the agent as a prompt.

## Tool Approval

When `approval_policy = "ask"`, the bot posts a permission request before running tools:

```
🔐 Permission request
`read_file("/src/main.rs")`
Options: Allow once / Always allow / Deny

React: ✅ allow · ❌ deny · 🔓 trust
```

- ✅ `white_check_mark` → allow this one time
- 🔓 `unlock` → trust for the rest of the session
- ❌ `x` → deny

## Storage Layout

```
~/.kiro/bots/<name>/
    config.toml              # instance config
    secrets.toml             # Slack tokens (chmod 600)
    policies/agents.cedar    # Cedar authorization policies
    state/
        bot.log              # daemon log output
        bot.pid              # daemon PID file
```

## Logs

```bash
# Follow live
tail -f ~/.kiro/bots/my-bot/state/bot.log

# Check last few lines
tail -20 ~/.kiro/bots/my-bot/state/bot.log
```

## Troubleshooting

**Bot doesn't respond to @mentions** — Check `bot_member_id` in config matches the bot's actual Slack member ID. Look for `"Filtered by response policy"` in the log.

**"Already running"** — Run `kiro-bot status` to see the PID. If the process is dead but PID file remains, `kiro-bot stop <name>` will clean it up.

**ACP worker fails to spawn** — Make sure `kiro-cli` is in PATH and `kiro-cli acp` works standalone.

**Cedar access denied** — Check the log for `"Bot access denied for user X"`. The principal name comes from the `[users]` table — if no mapping exists, the raw Slack ID is used.

## Examples

### Interactive Slack Bot

Full-featured bot with Cedar auth, tool approval, threaded conversations.

```toml
name = "team-assistant"
working_directory = "~/workplace/my-project"

[frontend]
type = "slack"
bot_name = "TeamBot"
bot_member_id = "U0AHNCJD8MV"
conversation_history = 10

[agent]
command = "kiro-cli acp"
model = "claude-opus-4.6-1m"
default_mode = "my-agent"
approval_policy = "ask"
max_workers = 10

[authorization]
cedar_policy_file = "policies/agents.cedar"

[users]
W017EQCFCUE = "alice"

[[response_policies]]
conversation = "dm:*"
trigger = "always"
reply = "inline"

[[response_policies]]
conversation = "*"
trigger = "directed_only"
reply = "thread"
```

```bash
kiro-bot install ./team-assistant
kiro-bot start team-assistant
```

### Scheduled Job → Slack

Run a prompt on a schedule, post results to a Slack channel. Needs `secrets.toml` with bot token.

```toml
name = "daily-digest"
working_directory = "~/workplace/kiro_reviewer"

[frontend]
type = "cron"
prompt = "Generate the daily feedback digest for the last 24 hours"
schedule = "0 0 15 * * * *"    # daily at 3pm UTC

[frontend.output]
type = "slack"
channel = "C0AS5PJPSKH"

[agent]
command = "kiro-cli acp"
model = "claude-sonnet-4-20250514"
default_mode = "feedback-digest"
```

```bash
kiro-bot install ./daily-digest    # prompts for Slack tokens
kiro-bot start daily-digest
```

### Scheduled Job → Logs Only

Pure headless — no Slack, no secrets. Output goes to the log file.

```toml
name = "security-scan"
working_directory = "~/workplace/my-project"

[frontend]
type = "cron"
prompt = "Scan the codebase for security vulnerabilities and report findings"
every = "6h"

[frontend.output]
type = "stdout"

[agent]
command = "kiro-cli acp"
model = "claude-sonnet-4-20250514"
```

```bash
kiro-bot install ./security-scan    # no token prompt
kiro-bot start security-scan
tail -f ~/.kiro/bots/security-scan/state/bot.log
```

### One-Shot Task (CI / Scripts)

Run once, print to terminal, exit. No daemon, no schedule.

```toml
name = "changelog"
working_directory = "~/workplace/my-project"

[frontend]
type = "cron"
prompt = "Generate changelog entries from the last 10 git commits"

[frontend.output]
type = "stdout"

[agent]
command = "kiro-cli acp"
model = "claude-sonnet-4-20250514"
```

```bash
kiro-bot run changelog              # runs once, prints to stdout, exits
kiro-bot run changelog >> CHANGELOG.md   # append to file
```

### Time-Bounded Job

Run every minute during business hours, auto-stop at end of day.

```toml
name = "standup-reminder"
working_directory = "/tmp"

[frontend]
type = "cron"
prompt = "Give a short motivational standup reminder"
every = "1m"
start_at = "2026-04-15T14:00:00Z"
stop_at = "2026-04-15T14:30:00Z"

[frontend.output]
type = "slack"
channel = "C0AJGQYQCGH"

[agent]
command = "kiro-cli acp"
model = "claude-sonnet-4-20250514"
```

```bash
kiro-bot start standup-reminder     # waits for start_at, runs, auto-exits at stop_at
```

## Capacity Planning

### Per-Worker Resources

| Resource | Estimate | Notes |
|----------|----------|-------|
| Memory | ~150 MB | One `kiro-cli acp` process with MCP servers loaded |
| CPU | ~0.1 core idle, 1 core burst | Burst during agent thinking + tool execution |
| File descriptors | ~20 | Stdin/stdout pipes, MCP server connections |
| Startup time | ~6s | ACP init + MCP server handshake |

### Fleet Sizing

| Fleet size | Workers | RAM needed | Host recommendation |
|------------|---------|------------|---------------------|
| Solo dev bot | 5 workers | 1 GB | Any dev box |
| Team bot (10 users) | 10-15 workers | 2-3 GB | m5.large or equivalent |
| Org bot (50 users) | 30-50 workers | 8-10 GB | m5.2xlarge |
| Multi-bot fleet | 3-5 instances × 10 workers | 6-10 GB | m5.2xlarge |

### Napkin Math

```
1 user sends ~20 messages/hour
Each message takes ~10-30s agent time
→ 1 worker handles ~120-360 messages/hour
→ 10 workers handle a team of 10 with headroom

Memory: 10 workers × 150 MB = 1.5 GB + 200 MB bot overhead = ~1.7 GB
Idle workers get reaped after idle_timeout_secs (default 300s)
→ Steady-state memory is much lower than peak
```

### Cron Job Resources

Cron instances are lighter — one worker, no Slack WebSocket:

| Resource | Estimate |
|----------|----------|
| Memory | ~200 MB (one ACP process) |
| Duration | 10s-5min per run (depends on prompt complexity) |
| Idle | 0 MB between runs (process exits or sleeps) |

## Scheduling

Cron instances support two scheduling modes:

### Fixed Interval

```toml
[frontend]
type = "cron"
prompt = "Check for new tickets"
every = "5m"                          # 30s, 5m, 1h, 2h30m
```

### Cron Expression

Standard 7-field cron syntax: `sec min hour day month weekday year`

```toml
[frontend]
type = "cron"
prompt = "Generate the daily feedback digest"
schedule = "0 0 15 * * * *"           # daily at 3pm UTC (7am PST)
```

Common patterns:

| Expression | Meaning |
|------------|---------|
| `0 */5 * * * * *` | Every 5 minutes |
| `0 0 15 * * * *` | Daily at 3pm UTC |
| `0 0 9 * * 1-5 *` | Weekdays at 9am UTC |
| `0 30 */2 * * * *` | Every 2 hours at :30 |
| `0 0 0 * * 1 *` | Every Monday at midnight |

### Time Bounds

```toml
start_at = "2026-04-15T09:00:00Z"    # don't start before this time
stop_at = "2026-04-15T17:00:00Z"     # auto-exit after this time
```

### One-Shot

Omit both `every` and `schedule` — use `kiro-bot run <name>` for manual/CI execution.

## Fleet Configuration

Run multiple bot instances on one host — each with its own config, agent, and Slack app (or shared app with different channels).

### Example: Team Fleet

```bash
# Interactive reviewer bot
kiro-bot start reviewer

# Oncall ticket watcher (cron, every 5 min)
kiro-bot start oncall-watcher

# Daily feedback digest (cron, once per day)
kiro-bot start daily-digest

# Check everything
kiro-bot status
# NAME                 STATUS     PID      AGENT
# reviewer             running    12345    reviewer
# oncall-watcher       running    12346    oncall
# daily-digest         running    12347    feedback-digest
```

### Shared Slack App

Multiple bot instances can share one Slack app if they use different channels:

```toml
# reviewer/config.toml
[frontend]
type = "slack"
bot_name = "KiroBot"
bot_member_id = "U0AHNCJD8MV"

[[response_policies]]
conversation = "channel:C0ARG53D0NL"   # #kiro-cli-pr-reviews
trigger = "directed_only"
reply = "thread"
```

```toml
# oncall-watcher/config.toml
[frontend]
type = "cron"
prompt = "Check for new Sev-2 tickets and post a summary"
every = "5m"

[frontend.output]
type = "slack"
channel = "C0ONCALL123"
```

### Start/Stop All

```bash
kiro-bot start --all     # start every installed instance
kiro-bot stop --all      # stop everything
kiro-bot status          # see the fleet
```

