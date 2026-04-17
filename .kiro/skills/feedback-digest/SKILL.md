---
name: feedback-digest
description: Generate a categorized Kiro CLI feedback digest from community Slack channels. Supports 2hr, daily, and weekly modes. Use when asked to generate a digest, check feedback trends, or analyze community sentiment.
---

# Feedback Digest Skill

Categorized digest from Kiro CLI community channels, cross-referenced with GitHub issues, Taskei tasks, and TCORP tickets.

## Sources

| Channel | ID |
|---------|-----|
| #kiro-cli-internal-software-builders | C064EVBE0LR |
| #amazon-builder-genai-power-users | C08GJKNC3KM |

## Target

Post to `#kiro-cli-feedback-digest` (`C0AS5PJPSKH`).

## Categories

- 🎉 **Praises** — positive feedback, success stories
- 🐛 **Issues** — bug reports, regressions, errors
- 💡 **Innovation** — creative uses, tools built on Kiro
- 📋 **Feedback Requests** — feature requests, UX suggestions

## Modes

| Mode | Time range | Thread depth | Extra sections |
|------|-----------|--------------|----------------|
| **2hr** | Last 2 hours | Top-level only | Taskei hot tasks, new GitHub issues, active Sev-2 tickets |
| **daily** | Yesterday 6:30am PT → today 6:30am PT | 3+ replies | Top 10 GitHub issues (7d), Taskei open tasks, Sev-2 tickets |
| **weekly** | Last Monday 6:30am PT → today | 5+ replies | Top 10 GitHub issues (30d), Taskei open tasks, Sev-2 tickets, trends from memory |

On Mondays, daily mode covers Friday→Monday.

## Cross-reference SOPs

For Issues and Feedback Requests, cross-reference with:

1. **GitHub Issues**: `gh api "search/issues?q=repo:kirodotdev/Kiro+is:issue+is:open+KEYWORD&per_page=5"` — exclude labels `ide`, `theme:ide-performance`, `theme:ide` (see `gh-issues` skill)
2. **Taskei tasks**: follow the `taskei-tasks` skill SOP
3. **TCORP tickets**: follow the `tcorp-tickets` skill SOP

Flag items with no matching issue/ticket as 🆕 New.

## Sev-2 Ticket Search

Resolver group is `Amazon Q for CLI`:
```
assignedGroup: ["Amazon Q for CLI"]
status: ["Assigned", "Researching", "Work In Progress", "Pending"]
currentSeverity: ["2", "2.5"]
```

## Workflow

1. Search memory for prior context (recurring issues, trends)
2. Fetch messages from both channels for the target time range
3. Fetch full thread replies for any message with 3+ replies — thread context is required for accurate classification and summaries
4. Classify each message into exactly one category
5. Cross-reference Issues and Feedback Requests (GitHub + Taskei + TCORP)
6. Fetch mode-specific extra sections (see SOPs above)
7. Format digest per the Output Format below
8. Write digest to a temp file and verify character count with `wc -c` — LLMs cannot reliably count characters
9. Save digest summary to memory for trend tracking

## Constraints

- You MUST fetch thread replies for messages with 3+ replies before classifying — thread context changes how items are categorized and summarized
- You MUST use `@login` format for all authors (email minus `@amazon.com` / `@amazon.co.jp` etc., prepended with `@`)
- You MUST NOT use superlatives or inflated language (comprehensive, critical, significant, essential, crucial) unless a post has 20+ reactions
- You MUST use neutral, descriptive language when summarizing
- You MUST include ALL required sections even if empty (show header with "(0)" or ":white_check_mark: No items")
- You MUST write the final digest to a temp file and run `wc -c` to verify it is under 12,000 characters
- If over 12,000 characters, you MUST trim verbose items (shorten bullet points, compress descriptions) and re-check — do NOT remove entire sections or items
- You MUST include a `(view post)` link for EVERY item in the digest
- You MUST include these sections in every digest regardless of mode:
  - Category sections (Praises, Issues, Innovation, Feedback Requests)
  - Useful Resources Shared — links, tools, repos, docs shared in messages or threads
  - Key Insights — patterns, trends, or takeaways from the window
  - Interesting Questions — unanswered or novel questions not already covered in categories
- You MUST NOT include basic support questions that were already answered in threads (e.g., "how do I disable X" with a one-line answer) in the Interesting Questions section
- You MUST NOT duplicate items across categories or sections — pick the best fit
- You SHOULD extract key discussion points from threads, not just summarize the top-level post

## Output Format

### Header

```
<date range> · <#C064EVBE0LR|kiro-cli-internal-software-builders> · <#C08GJKNC3KM|amazon-builder-genai-power-users>
```

### Item Format

```
• <summary> — <@login> (<link to message|view post>)
_<optional context or quote>_
:link: <link|#issue> · <link|Taskei ID>
```

Or if no tracked issue:
```
• <summary> — <@login> (<link to message|view post>)
:new: No tracked issue
```

- Author: `@login` (email minus @amazon.com)
- Cross-reference line only for Issues and Feedback Requests
- Keep each item to 1-2 lines max

### Category Sections

Each category gets an emoji header with count:

```
:tada: *Praises (<count>)*

• <summary> — <@login> (<link|view post>)

:bug: *Issues (<count>)*

• <summary> — <@login> (<link|view post>)
:link: <https://github.com/kirodotdev/Kiro/issues/N|#N> · <https://taskei.amazon.dev/tasks/ID|Taskei ID>

:bulb: *Innovation (<count>)*

• <summary> — <@login> (<link|view post>)
_<what they built>_

:clipboard: *Feedback Requests (<count>)*

• <summary> — <@login> (<link|view post>)
:new: No tracked issue
```

If a category has 0 items, show the header with "(0)" and no items.

### Always-Present Sections

These sections MUST appear in every digest, after the category sections and before the extra sections:

```
:link: *Useful Resources Shared (<count>)*

• <resource name/description> — shared by <@login> (<link|view post>)

:question: *Interesting Questions (<count>)*

• <question summary> — <@login> (<link|view post>)
_<context or why it's interesting>_

:brain: *Key Insights*

• <insight or pattern observed from this window>
```

If a section has 0 items, show: `:white_check_mark: No <items>`

### Extra Sections (mode-dependent)

**2hr mode:**
```
:github: *New GitHub Issues (last 2hrs)*
• <https://github.com/kirodotdev/Kiro/issues/N|#N> <title> — 👍 <count>

:pencil: *Taskei UX Refresh (new + high priority)*
• :new: <title> — created <time> (<https://taskei.amazon.dev/tasks/ID|ID>)

:rotating_light: *Active Sev-2 Tickets*
• <ticket_id> <title> — <status>, <age>d old (<https://t.corp.amazon.com/ID|view>)
```

**daily/weekly mode:**
```
:github: *Top GitHub Issues (last 7/30 days)*
• <https://github.com/kirodotdev/Kiro/issues/N|#N> <title> — 👍 <count>

:pencil: *Taskei Open Tasks (top 5 recently updated)*
• <title> — <status>, updated <date> (<https://taskei.amazon.dev/tasks/ID|ID>)

:rotating_light: *Active Sev-2 Tickets*
• <ticket_id> <title> — <status>, <age>d old (<https://t.corp.amazon.com/ID|view>)
```

If a section is empty: `:white_check_mark: No <items>`

### Footer

```
_<N> messages from 2 channels · :robot_face: kiro-cli-feedback-digest agent_
```

### Size Constraint

Total output must be under 12,000 characters. Verify with `wc -c` and trim verbose items if needed.

## Rules

- NEVER duplicate items across categories — pick the best fit
- Skip bot messages, automated posts, digest cross-posts
- Skip zero-engagement messages unless they contain clear bug reports


