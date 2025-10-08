# Visual Diagrams

## Component Relationships

```
┌─────────────────────────────────────────────────────────────┐
│                        ChatArgs                             │
│                      .execute()                             │
└────────────────────────┬────────────────────────────────────┘
                         │
                         ↓
┌─────────────────────────────────────────────────────────────┐
│                      AgentEnvUi                             │
│  ┌──────────────────────────────────────────────────────┐   │
│  │  Main Loop                                           │   │
│  │  • Prompt user                                       │   │
│  │  • Spawn job                                         │   │
│  │  • Wait for completion                               │   │
│  │  • Cleanup old jobs                                  │   │
│  └──────────────────────────────────────────────────────┘   │
│                                                             │
│  ┌──────────────┐  ┌──────────────┐  ┌──────────────┐       │
│  │ InputHandler │  │ CtrlCHandler │  │  Shutdown    │       │
│  │              │  │              │  │ Coordinator  │       │
│  └──────────────┘  └──────────────┘  └──────────────┘       │
└────────────────────────┬────────────────────────────────────┘
                         │
                         ↓
┌─────────────────────────────────────────────────────────────┐
│                       Session                               │
│  ┌──────────────────────────────────────────────────────┐   │
│  │  Workers: [Worker1, Worker2, ...]                    │   │
│  │  Jobs:    [Job1, Job2, Job3, ...]                    │   │
│  │           (max 3 inactive)                           │   │
│  └──────────────────────────────────────────────────────┘   │
└─────────────────────────────────────────────────────────────┘
```

## Main Loop Flow

```
┌─────────────┐
│   Start     │
└──────┬──────┘
       │
       ↓
┌─────────────────────┐
│ Initialize Session  │
│ Initialize UI       │
│ Start Ctrl+C        │
│ Handler             │
└──────┬──────────────┘
       │
       ↓
┌─────────────────────┐
│ Enter Prompt        │◄──────────────────┐
│ Context             │                   │
└──────┬──────────────┘                   │
       │                                  │
       ↓                                  │
┌─────────────────────┐                   │
│ Read User Input     │                   │
│ (with cancellation) │                   │
└──────┬──────────────┘                   │
       │                                  │
       ├─→ None/Cancelled ──→ [Shutdown]  │
       │                                  │
       ↓                                  │
┌─────────────────────┐                   │
│ Check Special       │                   │
│ Commands            │                   │
└──────┬──────────────┘                   │
       │                                  │
       ├─→ /quit ──────────→ [Shutdown]   │
       │                                  │
       ↓                                  │
┌─────────────────────┐                   │
│ Cleanup Old Jobs    │                   │
│ (keep max 3)        │                   │
└──────┬──────────────┘                   │
       │                                  │
       ↓                                  │
┌─────────────────────┐                   │
│ Spawn New Job       │                   │
└──────┬──────────────┘                   │
       │                                  │
       ↓                                  │
┌─────────────────────┐                   │
│ Enter Job Context   │                   │
└──────┬──────────────┘                   │
       │                                  │
       ↓                                  │
┌─────────────────────┐                   │
│ Wait for Job        │                   │
│ Completion          │                   │
└──────┬──────────────┘                   │
       │                                  │
       ├─→ Shutdown? ────→ [Shutdown]     │
       │                                  │
       ↓                                  │
┌─────────────────────┐                   │
│ Exit Job Context    │                   │
└──────┬──────────────┘                   │
       │                                  │
       ↓                                  │
┌─────────────────────┐                   │
│ Display Result      │                   │
└──────┬──────────────┘                   │
       │                                  │
       └──────────────────────────────────┘
```

## Ctrl+C Handling State Machine

```
                    ┌──────────────┐
                    │  Ctrl+C      │
                    │  Pressed     │
                    └───────┬──────┘
                            │
                ┌───────────┴───────────┐
                │                       │
                ↓                       ↓
        ┌───────────────┐       ┌──────────────┐
        │  In Prompt?   │       │  In Job?     │
        │               │       │              │
        │  YES          │       │  YES         │
        └───────┬───────┘       └──────┬───────┘
                │                      │
                ↓                      ↓
        ┌───────────────┐       ┌──────────────────┐
        │  Exit App     │       │  First Ctrl+C?   │
        │  Immediately  │       │                  │
        └───────────────┘       └──────┬───────────┘
                                       │
                        ┌──────────────┴──────────────┐
                        │                             │
                        ↓                             ↓
                ┌───────────────┐           ┌─────────────────┐
                │  YES          │           │  NO             │
                │               │           │                 │
                │  Cancel Job   │           │  < 1 sec since  │
                │  Show Message │           │  last Ctrl+C?   │
                └───────────────┘           └────────┬────────┘
                                                     │
                                    ┌────────────────┴────────────────┐
                                    │                                 │
                                    ↓                                 ↓
                            ┌───────────────┐               ┌─────────────────┐
                            │  YES          │               │  NO             │
                            │               │               │                 │
                            │  Exit App     │               │  Cancel Job     │
                            │  (Force)      │               │  Reset Counter  │
                            └───────────────┘               └─────────────────┘
```

## Job Lifecycle

```
┌─────────────┐
│  Job        │
│  Created    │
└──────┬──────┘
       │
       ↓
┌─────────────┐
│  Job        │
│  Launched   │
│  (Active)   │
└──────┬──────┘
       │
       ├─────────────────────────────────┐
       │                                 │
       ↓                                 ↓
┌─────────────┐                   ┌─────────────┐
│  Job        │                   │  Job        │
│  Completes  │                   │  Cancelled  │
│  (Inactive) │                   │  (Inactive) │
└──────┬──────┘                   └──────┬──────┘
       │                                 │
       └─────────────┬───────────────────┘
                     │
                     ↓
              ┌─────────────┐
              │  Cleanup    │
              │  Triggered  │
              └──────┬──────┘
                     │
                     ↓
              ┌─────────────┐
              │  Keep Last  │
              │  3 Inactive │
              │  Jobs       │
              └─────────────┘
```

## Shutdown Sequence

```
┌─────────────────┐
│  Shutdown       │
│  Triggered      │
│  (Ctrl+C, /quit)│
└────────┬────────┘
         │
         ↓
┌─────────────────┐
│  Set Shutdown   │
│  Token          │
└────────┬────────┘
         │
         ↓
┌─────────────────┐
│  Main Loop      │
│  Exits          │
└────────┬────────┘
         │
         ↓
┌─────────────────┐
│  Cancel All     │
│  Active Jobs    │
└────────┬────────┘
         │
         ↓
┌─────────────────┐
│  Wait for Jobs  │
│  to Complete    │
└────────┬────────┘
         │
         ↓
┌─────────────────┐
│  Save History   │
└────────┬────────┘
         │
         ↓
┌─────────────────┐
│  Cleanup        │
│  Resources      │
└────────┬────────┘
         │
         ↓
┌─────────────────┐
│  Return from    │
│  AgentEnvUi.run │
└────────┬────────┘
         │
         ↓
┌─────────────────┐
│  Return from    │
│  ChatArgs       │
│  .execute()     │
└────────┬────────┘
         │
         ↓
┌─────────────────┐
│  CLI Exits      │
└─────────────────┘
```

## Component Interaction Timeline

```
Time  │ AgentEnvUi │ InputHandler │ CtrlCHandler │ Session │ WorkerJob
──────┼────────────┼──────────────┼──────────────┼─────────┼──────────
  0   │ Initialize │              │              │         │
  1   │            │ Create       │              │         │
  2   │            │              │ Create       │         │
  3   │            │              │ Start Listen │         │
  4   │ Enter      │              │              │         │
      │ Prompt     │              │              │         │
  5   │            │ Read Input   │              │         │
  6   │            │ [User types] │              │         │
  7   │            │ Return Input │              │         │
  8   │ Cleanup    │              │              │ Cleanup │
      │ Jobs       │              │              │ Old Jobs│
  9   │ Spawn Job  │              │              │ Create  │ Create
 10   │            │              │              │ Launch  │ Launch
 11   │ Enter Job  │              │              │         │
 12   │ Wait       │              │              │         │ Running
 13   │            │              │              │         │ Running
 14   │            │              │              │         │ Complete
 15   │ Exit Job   │              │              │         │
 16   │ Display    │              │              │         │
      │ Result     │              │              │         │
 17   │ Loop Back  │              │              │         │
      │ to Step 4  │              │              │         │
```

## Memory Layout (Job Cleanup)

### Before Cleanup (5 inactive jobs)
```
Jobs List:
┌─────────────────────────────────────────┐
│ Job 1 (Inactive - Completed)            │  ← Oldest
├─────────────────────────────────────────┤
│ Job 2 (Inactive - Completed)            │
├─────────────────────────────────────────┤
│ Job 3 (Inactive - Cancelled)            │
├─────────────────────────────────────────┤
│ Job 4 (Inactive - Completed)            │
├─────────────────────────────────────────┤
│ Job 5 (Inactive - Completed)            │  ← Newest
└─────────────────────────────────────────┘
```

### After Cleanup (3 inactive jobs kept)
```
Jobs List:
┌─────────────────────────────────────────┐
│ Job 3 (Inactive - Cancelled)            │
├─────────────────────────────────────────┤
│ Job 4 (Inactive - Completed)            │
├─────────────────────────────────────────┤
│ Job 5 (Inactive - Completed)            │  ← Newest
└─────────────────────────────────────────┘

Removed: Job 1, Job 2
```

### With Active Jobs (1 active + 4 inactive)
```
Before Cleanup:
┌─────────────────────────────────────────┐
│ Job 1 (Inactive - Completed)            │
├─────────────────────────────────────────┤
│ Job 2 (Inactive - Completed)            │
├─────────────────────────────────────────┤
│ Job 3 (Active)                          │  ← Never removed
├─────────────────────────────────────────┤
│ Job 4 (Inactive - Completed)            │
├─────────────────────────────────────────┤
│ Job 5 (Inactive - Completed)            │
└─────────────────────────────────────────┘

After Cleanup:
┌─────────────────────────────────────────┐
│ Job 3 (Active)                          │  ← Preserved
├─────────────────────────────────────────┤
│ Job 2 (Inactive - Completed)            │
├─────────────────────────────────────────┤
│ Job 4 (Inactive - Completed)            │
├─────────────────────────────────────────┤
│ Job 5 (Inactive - Completed)            │
└─────────────────────────────────────────┘

Removed: Job 1 only (oldest inactive)
```

## Cancellation Token Flow

```
┌──────────────────────────────────────────────────────────┐
│                    Shutdown Token                        │
│              (Global, created by                         │
│              ShutdownCoordinator)                        │
└────────────────────┬─────────────────────────────────────┘
                     │
                     ├─→ Passed to InputHandler
                     │   (cancels read_prompt)
                     │
                     ├─→ Passed to CtrlCHandler
                     │   (triggers on Ctrl+C)
                     │
                     └─→ Checked in main loop
                         (exits loop when cancelled)

┌──────────────────────────────────────────────────────────┐
│                    Job Token                             │
│              (Per-job, created by Session)               │
└────────────────────┬─────────────────────────────────────┘
                     │
                     ├─→ Passed to WorkerTask
                     │   (cancels task execution)
                     │
                     ├─→ Passed to CtrlCHandler
                     │   (cancelled on first Ctrl+C)
                     │
                     └─→ Checked by WorkerTask
                         (stops work when cancelled)
```
