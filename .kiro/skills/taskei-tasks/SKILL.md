---
name: taskei-tasks
description: SOP for querying Taskei tasks for Kiro CLI UX refresh bugs and feature work. Use when checking open bugs, cross-referencing Slack feedback with tracked tasks, or triaging Taskei items. Triggers on "taskei", "ux refresh", "taskei bugs", "open tasks", "task board".
---

# Taskei Tasks SOP

Query the Kiro CLI UX Refresh Bugs board on Taskei.

## Room & label

- Board ID: `0205a00e-4757-425d-bde0-e06884dce83e` — per the KiroLabs Backlog wiki this is the Bug Reports FOLDER of room `7c221a81-7ca7-436c-8f05-a7278949341b`. The API accepts the folder ID as a `roomId` and scopes results to this board; that folder-as-room form is what every query below uses (verified working)
- Label (ux-refresh-bugs): `9486ec82-fc33-43bf-92c5-066b139bfe1c`
- Board URL: `https://taskei.amazon.dev/rooms/0205a00e-4757-425d-bde0-e06884dce83e/tasks?f=labels%3A9486ec82-fc33-43bf-92c5-066b139bfe1c%20AND%20NOT%20status%3AClosed`

## Parameter shape (read this first)

`TaskeiListTasks` takes STRUCTURED parameters. It does NOT accept a `filter` query string (that grammar is web-UI-only), and it silently ignores unknown parameters — a call with `filter:`/`sort:`/`maxResults:` at top level runs the default unfiltered query and returns page 1 of open tasks, which makes a broken search look like a clean result. Check `tools/list` if in doubt.

## List open tasks (recently updated)

```
@builder-mcp/TaskeiListTasks
roomId: "0205a00e-4757-425d-bde0-e06884dce83e"
labels: ["9486ec82-fc33-43bf-92c5-066b139bfe1c"]
status: "Open"
sortBy: { attribute: "lastUpdatedDate" }
pagination: { maxResults: 100 }
```

Results are paginated (default page is 25). For a full scan, follow the `after` token in `pagination` until exhausted.

## Search by keyword

```
@builder-mcp/TaskeiListTasks
roomId: "0205a00e-4757-425d-bde0-e06884dce83e"
name: { queryOperator: "contains", value: "KEYWORD" }
status: "ALL"
pagination: { maxResults: 100 }
```

`name.queryOperator` supports `contains` / `doesNotContain`. Use `status: "ALL"` for dedup searches so closed duplicates surface too.

## Get task details

```
@builder-mcp/TaskeiGetTask
taskId: "<shortId>"
```

## Format

```
:pencil: *Taskei Open Tasks (top 5 recently updated)*
• <title> — <status>, updated <date> (<link>)
```

## Task links

Format: `https://taskei.amazon.dev/tasks/<shortId>`
