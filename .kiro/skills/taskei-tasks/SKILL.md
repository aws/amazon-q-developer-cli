---
name: taskei-tasks
description: SOP for querying Taskei tasks for Kiro CLI UX refresh bugs and feature work. Use when checking open bugs, cross-referencing Slack feedback with tracked tasks, or triaging Taskei items. Triggers on "taskei", "ux refresh", "taskei bugs", "open tasks", "task board".
---

# Taskei Tasks SOP

Query the Kiro CLI UX Refresh Bugs board on Taskei.

## Room & label

- Room ID: `0205a00e-4757-425d-bde0-e06884dce83e`
- Label (ux-refresh-bugs): `9486ec82-fc33-43bf-92c5-066b139bfe1c`
- Board URL: `https://taskei.amazon.dev/rooms/0205a00e-4757-425d-bde0-e06884dce83e/tasks?f=labels%3A9486ec82-fc33-43bf-92c5-066b139bfe1c%20AND%20NOT%20status%3AClosed`

## List open tasks (recently updated)

```
@builder-mcp/TaskeiListTasks
roomId: "0205a00e-4757-425d-bde0-e06884dce83e"
filter: "labels:9486ec82-fc33-43bf-92c5-066b139bfe1c AND NOT status:Closed"
sort: "lastUpdatedDate desc"
```

## Search by keyword

```
@builder-mcp/TaskeiListTasks
roomId: "0205a00e-4757-425d-bde0-e06884dce83e"
filter: "labels:9486ec82-fc33-43bf-92c5-066b139bfe1c AND NOT status:Closed AND title:KEYWORD"
```

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
