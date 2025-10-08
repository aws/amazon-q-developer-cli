# Visual Diagrams (Queue-Based)

## Component Relationships

```
┌─────────────────────────────────────────────────────────────┐
│                        ChatArgs                             │
│                      .execute()                             │
└────────────────────────┬────────────────────────────────────┘
                         │
                         ↓
┌─────────────────────────────────────────────────────────────┐
│                   AgentEnvTextUi                            │
│  ┌──────────────────────────────────────────────────────┐   │
│  │  Main Loop                                           │   │
│  │  • Dequeue prompt request                            │   │
│  │  • Read user input                                   │   │
│  │  • Launch job with continuation                      │   │
│  │  • Continuation re-queues prompt                     │   │
│  └──────────────────────────────────────────────────────┘   │
│                                                             │
│  ┌──────────────┐  ┌──────────────┐  ┌──────────────┐       │
│  │ PromptQueue  │  │ InputHandler │  │ CtrlCHandler │       │
│  └──────────────┘  └──────────────┘  └──────────────┘       │
└────────────────────────┬────────────────────────────────────┘
                         │
                         ↓
┌─────────────────────────────────────────────────────────────┐
│                       Session                               │
│  ┌──────────────────────────────────────────────────────┐   │
│  │  Workers: [Worker1, Worker2, ...]                    │   │
│  │  Jobs:    [Job1, Job2, Job3, ...]                    │   │
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
│ Enqueue Initial     │
│ Prompt              │
└──────┬──────────────┘
       │
       ↓
┌─────────────────────┐
│ Check Shutdown?     │◄──────────────────┐
└──────┬──────────────┘                   │
       │                                  │
       ├─→ Yes ──────────→ [Cleanup]      │
       │                                  │
       ↓                                  │
┌─────────────────────┐                   │
│ Dequeue Prompt      │                   │
│ Request             │                   │
└──────┬──────────────┘                   │
       │                                  │
       ├─→ None ──→ Sleep 100ms ──────────┤
       │                                  │
       ↓                                  │
┌─────────────────────┐                   │
│ Read User Input     │                   │
└──────┬──────────────┘                   │
       │                                  │
       ├─→ Error ────────→ [Cleanup]      │
       │                                  │
       ↓                                  │
┌─────────────────────┐                   │
       │                                  │
│ Empty? ──→ Re-queue ─────────────────┤
       │                                  │
       ↓                                  │
┌─────────────────────┐                   │
│ /quit? ──────────────→ [Cleanup]        │
       │                                  │
       ↓                                  │
┌─────────────────────┐                   │
│ Push to             │                   │
│ Conversation        │                   │
└──────┬──────────────┘                   │
       │                                  │
       ↓                                  │
┌─────────────────────┐                   │
│ Launch AgentLoop    │                   │
│ with Continuation   │                   │
└──────┬──────────────┘                   │
       │                                  │
       ↓                                  │
┌─────────────────────┐                   │
│ Continuation        │                   │
│ Re-queues Prompt    │                   │
│ (when job completes)│                   │
└──────┬──────────────┘                   │
       │                                  │
       └──────────────────────────────────┘
```

## Continuation Flow

```
┌─────────────────────┐
│ AgentLoop Job       │
│ Completes           │
└──────┬──────────────┘
       │
       ↓
┌─────────────────────┐
│ Continuation        │
│ Executes            │
└──────┬──────────────┘
       │
       ├─→ Normal ──────┐
       ├─→ Cancelled ───┤
       ├─→ Failed ──────┤
       │                │
       ↓                ↓
┌─────────────────────┐
│ Display Status      │
│ Message             │
└──────┬──────────────┘
       │
       ↓
┌─────────────────────┐
│ Re-queue Prompt     │
│ for Worker          │
└──────┬──────────────┘
       │
       ↓
┌─────────────────────┐
│ Main Loop Will      │
│ Process Next        │
└─────────────────────┘
```

## Ctrl+C Handling

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
        │  (read_line)  │       │              │
        └───────┬───────┘       └──────┬───────┘
                │                      │
                ↓                      ↓
        ┌───────────────┐       ┌──────────────────┐
        │  InputHandler │       │  First Ctrl+C?   │
        │  Returns Error│       │                  │
        └───────┬───────┘       └──────┬───────────┘
                │                      │
                ↓               ┌──────┴──────┐
        ┌───────────────┐       │             │
        │  Main Loop    │       ↓             ↓
        │  Exits        │   ┌───────┐   ┌─────────┐
        └───────────────┘   │  YES  │   │   NO    │
                            │       │   │         │
                            │Cancel │   │< 1 sec? │
                            │ Jobs  │   └────┬────┘
                            └───────┘        │
                                      ┌──────┴──────┐
                                      │             │
                                      ↓             ↓
                                  ┌───────┐   ┌─────────┐
                                  │  YES  │   │   NO    │
                                  │       │   │         │
                                  │Trigger│   │ Cancel  │
                                  │Shutdown   │ Jobs    │
                                  └───────┘   └─────────┘
```

## Prompt Queue (Multi-Worker)

```
Worker1 Job Completes
    ↓
Continuation Re-queues Prompt1
    ↓
┌─────────────────────────────────┐
│      PromptQueue (FIFO)         │
│                                 │
│  [Prompt1] [Prompt2]            │
│     ↑         ↑                 │
│     │         │                 │
│  Worker1   Worker2              │
└─────────────────────────────────┘
    ↓
Main Loop Dequeues Prompt1
    ↓
Display: "Worker1> "
    ↓
User enters input for Worker1
    ↓
Launch Job for Worker1
    ↓
(Meanwhile Worker2's prompt waits in queue)
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
│  Save History   │
└────────┬────────┘
         │
         ↓
┌─────────────────┐
│  Return from    │
│  run()          │
└────────┬────────┘
         │
         ↓
┌─────────────────┐
│  CLI Exits      │
└─────────────────┘
```

## Memory Layout (Prompt Queue)

### Single Worker
```
PromptQueue:
┌─────────────────────────────────────────┐
│ [Worker1 Request]                       │
└─────────────────────────────────────────┘
```

### Multiple Workers (Future)
```
PromptQueue:
┌─────────────────────────────────────────┐
│ [Worker1] [Worker2] [Worker1] [Worker3] │
│     ↑         ↑         ↑         ↑     │
│     │         │         │         │     │
│  Job1     Job2      Job3      Job4      │
│  Done     Done     Done      Done       │
└─────────────────────────────────────────┘

Main loop processes FIFO:
1. Prompt Worker1
2. Prompt Worker2
3. Prompt Worker1 (again)
4. Prompt Worker3
```

## Timeline

```
Time  │ Main Loop      │ PromptQueue    │ AgentLoop      │ Continuation
──────┼────────────────┼────────────────┼────────────────┼──────────────
  0   │ Start          │ Empty          │                │
  1   │ Enqueue        │ [Worker1]      │                │
  2   │ Dequeue        │ Empty          │                │
  3   │ Read input     │                │                │
  4   │ Launch job     │                │ Start          │
  5   │ Loop back      │                │ Running        │
  6   │ Dequeue        │ Empty          │ Running        │
  7   │ Sleep 100ms    │                │ Running        │
  8   │ Loop back      │                │ Running        │
  9   │ Dequeue        │ Empty          │ Complete       │ Execute
 10   │ Sleep 100ms    │ [Worker1]      │                │ Re-queue
 11   │ Loop back      │ [Worker1]      │                │
 12   │ Dequeue        │ Empty          │                │
 13   │ Read input     │                │                │
```
