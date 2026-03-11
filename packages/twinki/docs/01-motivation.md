# Twinki — Design Document
## Page 1 of 5: Motivation, History, and the Problem Space

---

### 1.1 The Terminal in 2026

The terminal was supposed to die. In 2001, when Windows XP shipped with a polished
GUI and the web was becoming the dominant application platform, the conventional
wisdom was that the command line would fade into a niche tool for system
administrators. That prediction was wrong in a way that matters for this document.

By 2026, the terminal is the primary interface for an entire generation of
developers. Every major AI coding agent — Claude Code, Codex, Amp, Gemini CLI,
pi — runs in the terminal. Every CI pipeline, every cloud CLI, every package
manager lives there. The terminal is not legacy infrastructure. It is the substrate
of modern software development, and it is more actively used today than at any
point in the last twenty years.

The tools we use to build terminal interfaces have not kept pace with this reality.

---

### 1.2 The Ink Era: What It Got Right

In 2017, Vadim Demedes released Ink, a React renderer that targets the terminal
instead of the DOM. The idea was precise and elegant: use React's component model,
reconciler, and hooks to build interactive CLI applications, with layout handled by
Yoga, Facebook's Flexbox engine. Developers who already knew React could build rich
terminal UIs without learning a new paradigm.

Ink was genuinely revolutionary. It brought component-based thinking, declarative
state management, and a massive ecosystem of React tooling to the terminal. By 2026,
Ink has over 28,000 GitHub stars and is used by GitHub Copilot CLI, Gatsby, Prisma,
Shopify's CLI, and hundreds of other tools. It is the dominant Node.js TUI framework.

The core insight Ink validated — that React's component model is the right
abstraction for terminal UIs — is one Twinki inherits completely. The component
model is not the problem.

---

### 1.3 The Rendering Problem: What Ink Gets Wrong

Ink's rendering model, unchanged in its essentials since 2017, works as follows:

1. React reconciles the component tree and produces a new virtual DOM.
2. Ink serializes the entire output to a string of ANSI escape sequences.
3. Ink moves the cursor to the top of the rendered region.
4. Ink writes the entire string to stdout.

Steps 3 and 4 are the problem. Between the cursor-move escape sequence and the
completion of the write, the terminal displays a partially-overwritten state. On a
fast local terminal with a small UI, this window is imperceptible. On a slower
machine, in VS Code's integrated terminal, over SSH, or under CPU load, it becomes
the signature flicker that every user of Claude Code, Amp, or any Ink-based tool
has seen.

The fix has been known since at least 2022: **synchronized output**, standardized
as DEC private mode `?2026`. Bracketing writes with `CSI ?2026h` (begin) and
`CSI ?2026l` (end) tells the terminal to buffer the entire update and present it
atomically. Ink did not adopt this until very recently, and even then the underlying
write-everything-every-frame model remained.

Anthropic's team, building Claude Code on top of Ink, hit this wall hard enough
that they rewrote the renderer from scratch — keeping React as the component model
but replacing Ink's rendering layer entirely. That rewrite is the clearest possible
signal: the component model is correct, the rendering engine is not.

---

### 1.4 The Alt-Screen Trap

The industry's response to Ink's flickering has largely been to reach for alternate
screen mode (`smcup`/`rmcup`). This switches the terminal to a separate buffer,
giving the application full control over the viewport. Ratatui (Rust), Textual
(Python), and most game-like TUIs use this approach. Amp switched to it in late
2025. Google's Gemini CLI launched with it.

Alternate screen solves flickering. It also destroys the terminal's most valuable
properties:

- **Scrollback buffer** — gone. The user cannot scroll up to see previous output.
- **Native text selection** — broken or requires custom implementation.
- **Terminal search** (Cmd+F / Ctrl+Shift+F) — does not work on content that has
  scrolled off.
- **Right-click → paste** — often broken.
- **Screen readers** — severely degraded.

Google launched their alt-screen Gemini CLI to immediate user backlash and rolled
it back within a week. The lesson is not that alt-screen is always wrong — it is
excellent for dashboards, games, and full-screen editors. The lesson is that for
applications that are fundamentally conversational and linear (coding agents, REPLs,
long-running processes), alt-screen is the wrong trade. It solves a rendering
problem by destroying the terminal's core value proposition.

---

### 1.5 The 60 FPS Ceiling and Why It Is Artificial

Textual, the most sophisticated Python TUI framework, targets 60 FPS as its
baseline and treats anything beyond that as unnecessary. This made sense in 2022.
In 2026, displays running at 120 Hz are standard on MacBooks, high-end monitors,
and gaming hardware. The terminal emulators on these machines — Ghostty, Kitty,
WezTerm, iTerm2 — are GPU-accelerated and can composite frames at the display's
native refresh rate.

The 60 FPS cap is not a physical constraint. It is an architectural assumption
baked into a fixed-rate render loop. Twinki's math shows why this assumption is
wrong:

A typical differential frame (10 lines changed in an 80-column terminal) is
approximately 1,000 bytes. At Alacritty's measured throughput of 867 KB/s, that
write takes 1.15ms. The full pipeline — React reconcile, Yoga layout, line diff,
buffer assembly, write, terminal processing — totals approximately 2.05ms. The
frame budget at 240 Hz is 4.17ms. There is 2.12ms of headroom. The 60 FPS cap
is not protecting against a real constraint. It is leaving performance on the table.

The correct model is event-driven, not timer-driven. A state change schedules a
render via `process.nextTick` (latency: ~50μs). The render completes in ~2ms. The
effective frame rate is `min(state_change_rate, terminal_throughput_fps)`. At 1KB
differential frames, the terminal can sustain approximately 867 frames per second.
The practical limit is the display's refresh rate, not an artificial cap.

---

### 1.6 The Forgotten History: DOS-Era TUI Techniques

The history of terminal UI is not a straight line from dumb terminals to modern
TUIs. There was a golden age of TUI sophistication in the late 1980s and early
1990s — Borland's Turbo Pascal IDE, Norton Commander, Lotus 1-2-3, WordPerfect —
that was abandoned almost overnight when Windows 95 arrived. The techniques
developed in that era were not wrong. They were made irrelevant by the GUI
revolution.

In 2026, we are in a position to rediscover and apply those techniques with modern
hardware and modern protocols:

**Double buffering**: Every game engine uses double buffering. The front buffer is
what the user sees; the back buffer is where the next frame is being prepared. Swap
is atomic. pi-tui implements this at the line level: `previousLines` is the front
buffer, the newly rendered lines are the back buffer, and the diff determines what
to write.

**Dirty region tracking**: DOS-era TUI frameworks tracked which regions of the
screen had changed and only redrew those regions. pi-tui does this at the line
level: only lines where `previousLines[i] !== newLines[i]` are written. Twinki
extends this with React's reconciler identifying dirty subtrees before rendering.

**Retained mode rendering**: Borland's Object Windows Library used retained mode
rendering in 1991. The component tree persists across frames; only changed
components re-render. React's Fiber reconciler is a modern, sophisticated version
of this idea.

The ideas were right. The timing was wrong. Now the timing is right again.

---

### 1.7 What Twinki Is

Twinki is a TUI rendering engine for Node.js/TypeScript that:

1. Keeps React as the component model with full Ink API compatibility.
2. Replaces Ink's rendering layer with a line-based differential renderer.
3. Never uses alternate screen — this is a first-class non-goal.
4. Uses synchronized output, single-write frames, and event-driven scheduling
   to go beyond 60 FPS on capable terminals.
5. Degrades gracefully on older terminals.
6. Ships with an E2E testing framework built on xterm.js headless that captures
   frames, measures timing, and detects flickering — decoupled from any real
   terminal.

The name is a nod to Twinkle — the idea that a terminal UI should be as smooth
and precise as a single point of light, not a flickering fluorescent tube.
