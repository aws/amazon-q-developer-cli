# Twinki — Design Document
## Page 4 of 5: E2E Testing Framework — Architecture and Specification

---

### 4.1 The Testing Problem

Testing terminal UIs is hard for a specific reason: the output is a stream of
bytes that only has meaning when interpreted by a terminal emulator. A string like
`"\x1b[31mHello\x1b[0m"` is not "red Hello" until a terminal processes it. Testing
frameworks that assert on raw strings miss this entirely.

The existing approaches and their failures:

**`@ink/testing-library`**: Renders to a string and strips ANSI codes. Tells you
what text is visible but nothing about colors, styles, cursor position, timing,
or whether the diff algorithm is working correctly. A test that passes here can
still produce a visually broken UI.

**Manual visual inspection**: Does not scale, does not catch regressions, cannot
measure timing.

**Real terminal recording** (asciinema, ttyrec): Coupled to the developer's machine,
terminal emulator, and system timing. Not reproducible in CI. Cannot be run
headlessly.

**Custom virtual terminal**: Requires implementing a terminal emulator from scratch.
This is a significant engineering effort and will inevitably have bugs that differ
from real terminal behavior.

The correct approach, which pi-tui already uses, is `@xterm/headless` — a headless
build of xterm.js that processes ANSI escape sequences exactly as a real terminal
would, without any rendering. Twinki's testing framework builds on this foundation
and extends it with frame capture, timing measurement, and assertion primitives.

---

### 4.2 The `VirtualTerminal` Class

`VirtualTerminal` implements the `Terminal` interface using `@xterm/headless` as
the underlying emulator. It is a drop-in replacement for `ProcessTerminal` in
tests.

```typescript
class VirtualTerminal implements Terminal {
    private xterm: XtermTerminal;
    private inputHandler?: (data: string) => void;
    private resizeHandler?: () => void;

    constructor(columns = 80, rows = 24) {
        this.xterm = new XtermTerminal({
            cols: columns,
            rows: rows,
            disableStdin: true,
            allowProposedApi: true,
        });
    }

    write(data: string): void {
        this.xterm.write(data);
    }

    async flush(): Promise<void> {
        return new Promise(resolve => this.xterm.write('', resolve));
    }

    getViewport(): string[] {
        const lines: string[] = [];
        const buffer = this.xterm.buffer.active;
        for (let i = 0; i < this.xterm.rows; i++) {
            const line = buffer.getLine(buffer.viewportY + i);
            lines.push(line ? line.translateToString(true) : '');
        }
        return lines;
    }

    getScrollBuffer(): string[] {
        const lines: string[] = [];
        const buffer = this.xterm.buffer.active;
        for (let i = 0; i < buffer.length; i++) {
            const line = buffer.getLine(i);
            lines.push(line ? line.translateToString(true) : '');
        }
        return lines;
    }

    getCellAttributes(row: number, col: number): CellAttributes {
        const buffer = this.xterm.buffer.active;
        const line = buffer.getLine(buffer.viewportY + row);
        const cell = line?.getCell(col);
        return {
            char: cell?.getChars() ?? '',
            fg: cell?.getFgColor() ?? 0,
            bg: cell?.getBgColor() ?? 0,
            bold: cell?.isBold() === 1,
            italic: cell?.isItalic() === 1,
            underline: cell?.isUnderline() === 1,
            dim: cell?.isDim() === 1,
        };
    }

    sendInput(data: string): void {
        this.inputHandler?.(data);
    }

    resize(columns: number, rows: number): void {
        this._columns = columns;
        this._rows = rows;
        this.xterm.resize(columns, rows);
        this.resizeHandler?.();
    }
}
```

The `flush()` method is critical. xterm.js processes writes asynchronously
internally. `flush()` uses xterm.js's write callback to guarantee that all
previously written data has been processed before any assertions are made. Without
this, tests have race conditions.

Memory: an 80×24 xterm.js buffer uses approximately 75KB (1,920 cells × ~40 bytes
per cell object). This is acceptable for test environments. For large terminals
(220×50 = 11,000 cells × 40 bytes = 440KB), tests should use smaller dimensions
unless specifically testing large-terminal behavior.

---

### 4.3 Frame Capture

Twinki extends `VirtualTerminal` with `FrameCapturingTerminal`, which intercepts
every `write()` call and captures a frame snapshot at each synchronized output
boundary.

```typescript
interface Frame {
    index: number;
    timestamp: bigint;          // process.hrtime.bigint() at capture time
    viewport: string[];         // visible lines (trailing spaces trimmed)
    scrollBuffer: string[];     // entire scrollback
    cursor: { x: number; y: number };
    writeBytes: number;         // bytes written in this frame
    isFull: boolean;            // true if this was a full rerender
}

class FrameCapturingTerminal extends VirtualTerminal {
    private frames: Frame[] = [];
    private frameIndex = 0;
    private pendingCapture = false;

    write(data: string): void {
        super.write(data);
        // Detect synchronized output end marker
        if (data.includes('\x1b[?2026l')) {
            this.pendingCapture = true;
            this.pendingBytes = data.length;
            this.pendingIsFull = data.includes('\x1b[3J');
        }
    }

    async flush(): Promise<void> {
        await super.flush();
        if (this.pendingCapture) {
            this.frames.push({
                index: this.frameIndex++,
                timestamp: process.hrtime.bigint(),
                viewport: this.getViewport(),
                scrollBuffer: this.getScrollBuffer(),
                cursor: this.getCursorPosition(),
                writeBytes: this.pendingBytes,
                isFull: this.pendingIsFull,
            });
            this.pendingCapture = false;
        }
    }

    getFrames(): Frame[] { return [...this.frames]; }
    getLastFrame(): Frame | undefined { return this.frames[this.frames.length - 1]; }
    clearFrames(): void { this.frames = []; this.frameIndex = 0; }
}
```

Frame timestamps use `process.hrtime.bigint()` for nanosecond precision. Because
the virtual terminal runs in the same process as the application, these timestamps
are accurate and comparable.

---

### 4.4 The `TestSession` API

`TestSession` is the primary test API. It wraps a React element, a
`FrameCapturingTerminal`, and a `TUI` instance, providing a clean interface for
test scenarios.

```typescript
class TestSession {
    private terminal: FrameCapturingTerminal;
    private tui: TUI;
    private started = false;

    constructor(
        private element: React.ReactElement,
        private options: { cols?: number; rows?: number } = {}
    ) {
        this.terminal = new FrameCapturingTerminal(
            options.cols ?? 80,
            options.rows ?? 24
        );
        this.tui = new TUI(this.terminal);
    }

    async start(): Promise<void> {
        render(this.element, { stdout: this.terminal as any });
        this.tui.start();
        await this.terminal.flush();
        this.started = true;
    }

    async stop(): Promise<void> {
        this.tui.stop();
        await this.terminal.flush();
    }

    sendKey(keyId: KeyId): void {
        // Convert KeyId to raw terminal bytes and send
        const raw = keyIdToRaw(keyId);
        this.terminal.sendInput(raw);
    }

    sendText(text: string): void {
        this.terminal.sendInput(text);
    }

    async waitForFrame(
        predicate: (frame: Frame) => boolean,
        timeoutMs = 1000
    ): Promise<Frame> {
        const deadline = Date.now() + timeoutMs;
        while (Date.now() < deadline) {
            const frame = this.terminal.getLastFrame();
            if (frame && predicate(frame)) return frame;
            await new Promise(r => setImmediate(r));
            await this.terminal.flush();
        }
        throw new Error(`waitForFrame timed out after ${timeoutMs}ms`);
    }

    async waitForText(
        text: string,
        timeoutMs = 1000
    ): Promise<Frame> {
        return this.waitForFrame(
            f => f.viewport.some(line => line.includes(text)),
            timeoutMs
        );
    }

    getFrames(): Frame[] { return this.terminal.getFrames(); }
    getLastFrame(): Frame | undefined { return this.terminal.getLastFrame(); }
    getViewport(): string[] { return this.terminal.getViewport(); }
    now(): bigint { return process.hrtime.bigint(); }
}
```

---

### 4.5 The Four Assertion Dimensions

#### Dimension 1: Collision Prevention

A collision occurs when an overlay writes to a cell that the base content also
wrote to, in a way that was not intended. The most common form: an overlay that
is wider than its declared `width`, overwriting content to its right.

```typescript
function analyzeCollisions(frames: Frame[]): CollisionReport {
    const collisions: CollisionEvent[] = [];
    for (const frame of frames) {
        // For each overlay in the frame, verify its bounds
        // by checking that cells outside the overlay region
        // match the base content (no overflow)
        // Implementation uses xterm.js cell-level access
    }
    return { collisions, clean: collisions.length === 0 };
}

// Usage:
const report = analyzeCollisions(session.getFrames());
expect(report.clean).toBe(true);
```

#### Dimension 2: Flicker Detection

A flicker is a cell that transitions through an unintended blank or wrong state
between two frames. Definition: cell at (row, col) is non-blank in frame N, blank
in frame N+1, non-blank in frame N+2.

```typescript
function analyzeFlicker(frames: Frame[]): FlickerReport {
    const events: FlickerEvent[] = [];
    for (let i = 1; i < frames.length - 1; i++) {
        const prev = frames[i - 1];
        const curr = frames[i];
        const next = frames[i + 1];
        for (let row = 0; row < curr.viewport.length; row++) {
            for (let col = 0; col < (curr.viewport[row]?.length ?? 0); col++) {
                const prevChar = prev.viewport[row]?.[col] ?? ' ';
                const currChar = curr.viewport[row]?.[col] ?? ' ';
                const nextChar = next.viewport[row]?.[col] ?? ' ';
                if (prevChar !== ' ' && currChar === ' ' && nextChar !== ' ') {
                    events.push({ frame: i, row, col, prevChar, nextChar });
                }
            }
        }
    }
    return { events, clean: events.length === 0 };
}

// Usage:
const report = analyzeFlicker(session.getFrames());
expect(report.clean).toBe(true);
```

With synchronized output correctly implemented, this should always pass. The test
is a regression guard: if synchronized output is accidentally disabled or broken,
flicker tests catch it immediately.

#### Dimension 3: Time to Render (TTR)

TTR measures the latency from a state change being triggered to the frame
containing that change being written to stdout.

```typescript
async function measureTTR(
    session: TestSession,
    trigger: () => void,
    predicate: (frame: Frame) => boolean
): Promise<{ ttr_ns: bigint; ttr_ms: number }> {
    const t0 = process.hrtime.bigint();
    trigger();
    const frame = await session.waitForFrame(predicate);
    const ttr_ns = frame.timestamp - t0;
    return { ttr_ns, ttr_ms: Number(ttr_ns) / 1_000_000 };
}

// Usage:
const { ttr_ms } = await measureTTR(
    session,
    () => setState({ count: count + 1 }),
    frame => frame.viewport[0]?.includes(String(count + 1)) ?? false
);
expect(ttr_ms).toBeLessThan(5); // well under one 60fps frame
```

Expected TTR for a `process.nextTick`-scheduled render: 0.05–2.05ms (scheduling
latency + full pipeline). Any TTR above 16ms (one 60fps frame) indicates a
performance regression.

#### Dimension 4: Input Responsiveness

Input responsiveness measures the latency from a simulated keypress to the frame
that reflects the UI change caused by that keypress.

```typescript
async function measureInputLatency(
    session: TestSession,
    key: KeyId,
    predicate: (frame: Frame) => boolean
): Promise<{ latency_ns: bigint; latency_ms: number }> {
    const t0 = process.hrtime.bigint();
    session.sendKey(key);
    const frame = await session.waitForFrame(predicate);
    const latency_ns = frame.timestamp - t0;
    return { latency_ns, latency_ms: Number(latency_ns) / 1_000_000 };
}

// Usage:
const { latency_ms } = await measureInputLatency(
    session,
    'return',
    frame => frame.viewport[1]?.includes('submitted') ?? false
);
expect(latency_ms).toBeLessThan(10);
```

---

### 4.6 Frame Serialization for Snapshot Testing

Frames serialize to a human-readable format for snapshot files:

```
Frame 0 (t=0ms, 2400B, full):
┌────────────────────────────────────────────────────────────────────────────────┐
│ Hello World                                                                    │
│                                                                                │
└────────────────────────────────────────────────────────────────────────────────┘

Frame 1 (t=2ms, 130B, diff):
┌────────────────────────────────────────────────────────────────────────────────┐
│ Hello World                                                                    │
│ > user input_                                                                  │
└────────────────────────────────────────────────────────────────────────────────┘
```

The header line includes: frame index, time since first frame in ms, bytes written,
and whether it was a full or differential render. This makes it immediately visible
in code review whether a change accidentally triggered unnecessary full rerenders.

Cell-level attributes (color, bold, italic) are preserved in a compact notation
when `includeStyles: true` is passed to the serializer:

```
[fg:31 bold]Hello[/] World
```

This uses ANSI color codes directly (31 = red) to keep the format compact and
unambiguous.

---

### 4.7 `@twinki/testing-library`: Ink Compatibility

`@twinki/testing-library` exports the same API as `@ink/testing-library`:

```typescript
import { render } from '@twinki/testing-library';

const { lastFrame, stdin, frames, unmount, rerender } = render(<MyApp />);

// Identical to @ink/testing-library:
expect(lastFrame()).toContain('Hello World');
stdin.write('q');

// Twinki extensions:
expect(frames.length).toBeGreaterThan(0);
expect(frames[0].isFull).toBe(true);
expect(frames[1].writeBytes).toBeLessThan(200);
```

`lastFrame()` returns the text content of the most recent viewport from the
`VirtualTerminal`, stripping ANSI codes — identical to what `@ink/testing-library`
returns. Existing Ink test suites work without modification.

The `frames` array is a Twinki extension, not present in `@ink/testing-library`.
It provides access to the full frame history for timing and regression analysis.

---

### 4.8 Test Execution Model

All tests run entirely in-process:
- No real terminal, no PTY, no `node-pty` dependency for unit tests.
- Time is injectable: `process.hrtime.bigint()` is mockable for deterministic
  timing tests.
- Tests run in CI without a TTY (`process.stdout.isTTY` is false, which is handled
  gracefully).
- Tests are deterministic: same input → same frames, because the virtual terminal
  has no external timing dependencies.
- Tests can be parallelized: each `TestSession` creates an independent
  `VirtualTerminal` instance with no shared state.

For integration tests that verify behavior in a real terminal (e.g., confirming
that synchronized output actually prevents flicker in Ghostty, or that the Kitty
keyboard protocol is correctly negotiated), the framework provides
`RealTerminalSession`. This spawns the application in a PTY via `node-pty`,
captures output, and provides the same assertion API. These tests are marked
`@integration` and run in a separate CI job that requires a real terminal.

The distinction is important: unit tests verify correctness of the rendering
algorithm. Integration tests verify correctness of the terminal protocol
implementation. Both are necessary; neither can substitute for the other.
