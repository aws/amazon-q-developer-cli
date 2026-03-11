# Twinki — Design Document
## Page 5 of 5: Terminal Protocols, Input Pipeline, and Roadmap

---

### 5.1 The Input Pipeline: Complete Architecture

Input in a terminal application is a stream of bytes arriving on stdin. The
challenge is that these bytes are ambiguous: `\x1b` alone could be the Escape key
or the start of a multi-byte escape sequence. `\x09` is both Tab and Ctrl+I.
`\x0d` is both Enter and Ctrl+M. The legacy terminal protocol, designed for
teletypes in the 1970s, has never been fully resolved.

Twinki's input pipeline has four stages:

**Stage 1: Raw stdin → StdinBuffer**

`ProcessTerminal.start()` sets stdin to raw mode (`process.stdin.setRawMode(true)`),
enables bracketed paste mode (`\x1b[?2004h`), and queries for Kitty keyboard
protocol support (`\x1b[?u`). Raw stdin data events are piped into `StdinBuffer`.

**Stage 2: StdinBuffer → complete sequences**

`StdinBuffer` accumulates bytes and emits complete escape sequences. The
completeness check uses a state machine:

- CSI sequences (`\x1b[...`): complete when the final byte is in `[0x40, 0x7E]`
  (letters and several symbols). Special case: SGR mouse sequences (`\x1b[<...M/m`)
  require the full `<digits;digits;digits[Mm]` pattern.
- OSC sequences (`\x1b]...`): complete when terminated by BEL (`\x07`) or ST
  (`\x1b\\`).
- DCS sequences (`\x1bP...`): complete when terminated by ST.
- APC sequences (`\x1b_...`): complete when terminated by ST. Used for the cursor
  marker and Kitty graphics responses.
- SS3 sequences (`\x1bO`): complete after one more character.
- Meta sequences (`\x1b` + single char): complete immediately.

A 10ms timeout flushes incomplete sequences. This handles the ESC ambiguity: if
`\x1b` arrives alone and no more bytes arrive within 10ms, it is emitted as the
Escape key. If more bytes arrive within 10ms, they are accumulated into the
sequence.

Bracketed paste is handled specially: content between `\x1b[200~` and `\x1b[201~`
is accumulated and emitted as a single `paste` event, not as individual character
events. This prevents paste content from being interpreted as keyboard shortcuts.

**Stage 3: Kitty protocol detection and enablement**

`StdinBuffer`'s `data` event handler checks each emitted sequence against the
Kitty protocol response pattern (`/^\x1b\[\?(\d+)u$/`). On match:

1. Set `_kittyProtocolActive = true` globally.
2. Enable the protocol with flags 1+2+4: `\x1b[>7u`
   - Flag 1: disambiguate escape codes (Ctrl+I ≠ Tab, Ctrl+M ≠ Enter)
   - Flag 2: report event types (press=1, repeat=2, release=3)
   - Flag 4: report alternate keys (shifted key, base layout key for non-Latin keyboards)
3. Do not forward the response sequence to the TUI.

On `stop()`, the protocol is disabled with `\x1b[<u` (pop flags). On `drainInput()`,
the protocol is disabled first to prevent key release events from leaking to the
parent shell over slow SSH connections.

**Stage 4: TUI input dispatch**

Complete sequences arrive at `TUI.handleInput()`. The dispatch order:

1. Check `inputListeners` (registered via `addInputListener()`). Each listener
   can consume the event or transform the data.
2. Check for cell size response (`CSI 6 ; height ; width t`) if a cell size query
   is pending.
3. Check for the global debug key (`Shift+Ctrl+D`).
4. Verify the focused component's overlay is still visible (visibility can change
   due to terminal resize).
5. Filter key release events unless the focused component has `wantsKeyRelease = true`.
6. Forward to `focusedComponent.handleInput(data)`.
7. Call `requestRender()`.

---

### 5.2 The `matchesKey` API: Type-Safe Key Matching

`matchesKey(data, keyId)` is the primary input matching API. It takes raw terminal
bytes and a typed `KeyId` string and returns true if they match.

```typescript
type KeyId =
    | BaseKey
    | `ctrl+${BaseKey}`
    | `shift+${BaseKey}`
    | `alt+${BaseKey}`
    | `ctrl+shift+${BaseKey}`
    // ... all modifier combinations

// Usage:
if (matchesKey(data, 'ctrl+c')) { ... }
if (matchesKey(data, Key.ctrl('c'))) { ... }  // same, with autocomplete
if (matchesKey(data, 'shift+enter')) { ... }
if (matchesKey(data, 'ctrl+shift+p')) { ... }
```

The matching logic handles three protocol layers simultaneously:

**Legacy sequences**: Arrow keys (`\x1b[A`-`\x1b[D`), function keys (`\x1bOP`,
`\x1b[11~`-`\x1b[24~`), home/end/insert/delete/pageup/pagedown, and their
modifier variants. These are hardcoded lookup tables.

**Kitty protocol sequences**: CSI u format (`\x1b[<codepoint>;<modifier>:<event>u`).
The modifier is a bitmask: shift=1, alt=2, ctrl=4. The event type is 1=press,
2=repeat, 3=release. Alternate keys (shifted key, base layout key) are encoded
as additional colon-separated fields.

**xterm modifyOtherKeys**: `CSI 27 ; modifier ; keycode ~`. Used by terminals that
support modifier reporting but not the full Kitty protocol.

The base layout key fallback deserves special attention. When a user has a non-Latin
keyboard layout (Cyrillic, Arabic, Hebrew), pressing the key in the Ctrl+C position
sends a Cyrillic codepoint, not `c`. The Kitty protocol's base layout key field
reports the key's position in the standard PC-101 layout, allowing `matchesKey(data,
'ctrl+c')` to work correctly regardless of the user's keyboard layout. However,
this fallback is only applied when the primary codepoint is not already a recognized
Latin letter or symbol — to prevent remapped layouts (Dvorak, Colemak) from causing
false matches.

---

### 5.3 Terminal Capability Detection and Protocol Negotiation

At startup, `ProcessTerminal` performs a capability negotiation sequence:

```
→ \x1b[?2004h          Enable bracketed paste mode
→ \x1b[?u              Query Kitty keyboard protocol support
→ \x1b[16t             Query cell dimensions (pixels per cell)
```

Responses arrive on stdin and are parsed by `StdinBuffer` and `TUI.parseCellSizeResponse()`.

For image rendering, capabilities are detected from environment variables:

```
KITTY_WINDOW_ID or TERM_PROGRAM=kitty    → images: "kitty"
GHOSTTY_RESOURCES_DIR                    → images: "kitty"
WEZTERM_PANE                             → images: "kitty"
ITERM_SESSION_ID                         → images: "iterm2"
TERM_PROGRAM=vscode                      → images: null
COLORTERM=truecolor|24bit                → trueColor: true
```

This environment-variable approach is pragmatic: it works without any round-trip
to the terminal and covers the terminals that actually support image protocols.
The alternative (querying via `CSI ? 4 c` or similar) is unreliable because many
terminals respond to capability queries incorrectly.

---

### 5.4 Terminal Compatibility Matrix

| Feature | Ghostty | Kitty | WezTerm | iTerm2 | Alacritty | Win Terminal | macOS Term | xterm |
|---|---|---|---|---|---|---|---|---|
| Sync output (?2026) | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✗ | ✗ |
| Kitty keyboard | ✓ | ✓ | ✓ | ✗ | ✗ | ✗ | ✗ | ✗ |
| Bracketed paste | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ |
| True color | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ |
| Kitty graphics | ✓ | ✓ | ✓ | ✗ | ✗ | ✗ | ✗ | ✗ |
| Sixel graphics | ✗ | ✗ | ✓ | ✓ | ✗ | ✗ | ✗ | ✓ |
| OSC 8 hyperlinks | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✗ | ✗ |

The baseline (everything works, no advanced features) is any terminal with true
color and bracketed paste support — which covers every terminal in active use in
2026. Synchronized output is the most impactful missing feature on macOS Terminal.app;
users on that terminal will see the same behavior as Ink (no flicker on fast
machines, occasional flicker on slow ones or over SSH).

---

### 5.5 The Scrollback-Safe Rendering Model: Formal Invariants

The scrollback-safe model has three invariants that must hold at all times:

**Invariant 1**: `previousLines` contains exactly the lines in the live region.
Static content (written via `<Static>` or `console.log`) is never in
`previousLines`. It flows to the scrollback buffer and is never touched again.

**Invariant 2**: `maxLinesRendered` is the high-water mark. It equals the maximum
value of `previousLines.length` across all renders since the last full clear.
`viewportTop = max(0, maxLinesRendered - terminalHeight)`.

**Invariant 3**: The hardware cursor is always within the visible viewport after
a render completes. Specifically: `hardwareCursorRow ∈ [viewportTop, viewportTop + terminalHeight)`.

These invariants are maintained by the render algorithm. Violation of Invariant 3
is the most common source of rendering bugs: if the hardware cursor is above the
viewport (the user scrolled up), cursor movement commands will write to the wrong
lines. The algorithm detects this case (`firstChanged < previousContentViewportTop`)
and triggers a full clear+rerender.

---

### 5.6 Memory Model: Why Sessions Don't Leak

A common concern with long-running TUI sessions (coding agents that run for hours)
is memory growth. The rendering model is designed to prevent this:

`previousLines` stores only the live region — typically 5–20 lines for a coding
agent UI. Each line is a JS string: 32 bytes header + 2 bytes per character (UTF-16)
+ ~50 bytes of ANSI codes = ~292 bytes per line. For 20 lines: ~5.8KB. This is
constant regardless of session length.

Static content (completed messages, tool outputs) flows to the scrollback buffer,
which is managed by the terminal emulator, not by Twinki. The terminal emulator
has its own scrollback limit (typically 1,000–10,000 lines). Twinki has no
reference to this content after writing it.

`maxLinesRendered` is a single integer. It grows monotonically but is reset on
full clears (width changes). For a session that never changes terminal width, it
equals `viewportTop + terminalHeight` — a small constant.

The React component tree and Yoga layout nodes are the other memory consumers.
For a typical coding agent UI (50 components), this is approximately 50 × 500
bytes = 25KB. Components that cache their render output (completed markdown blocks)
hold their line arrays in memory until `invalidate()` is called. This is the
application's responsibility to manage.

---

### 5.7 Roadmap: Phased Implementation

**Phase 1: Core Engine** (current focus)

The minimum viable Twinki: a working React renderer with Ink API compatibility,
the differential rendering engine, synchronized output, and the xterm.js-based
test framework.

Deliverables:
- `packages/twinki`: React reconciler + Yoga layout + line-based renderer
- `packages/testing`: `VirtualTerminal`, `FrameCapturingTerminal`, `TestSession`
- `packages/testing-library`: `@twinki/testing-library` (Ink-compatible API)
- Full Ink API surface: `render`, `Box`, `Text`, `Static`, `Newline`, `Spacer`,
  `Transform`, `useInput`, `useApp`, `useStdin`, `useStdout`, `useStderr`,
  `useFocus`, `useFocusManager`
- E2E test suite covering all four assertion dimensions
- Verified compatibility with Claude Code, Amp, and Codex (import swap test)

**Phase 2: Enhanced Input**

Kitty keyboard protocol (already in pi-tui, needs integration with React hooks),
bracketed paste as a first-class `usePaste` hook, SGR mouse reporting via
`useMouse` hook, and `useFullscreen` hook for applications that want alternate
screen.

**Phase 3: Advanced Rendering**

Kitty graphics protocol via `<Image>` component, Sixel fallback, adaptive frame
pacing with display-rate detection, and `@twinki/devtools` — a profiler panel
that shows frame timing, diff statistics, and dirty region visualization.

**Phase 4: Ecosystem**

Storybook-style component explorer for Twinki components, migration codemods
(`npx twinki-migrate`), documentation site, and npm publication.

---

### 5.8 The V8 Moment

The V8 JavaScript engine, released with Chrome in 2008, did not invent JavaScript.
It did not change the language. It changed what was possible with the language by
making the runtime dramatically faster — JIT compilation, hidden classes, inline
caches. Applications that were previously too slow to be practical became fast
enough to be excellent.

Twinki's relationship to Ink is analogous. Twinki does not invent React or the
terminal. It does not change the component model that developers already know. It
changes what is possible with React in the terminal by replacing the rendering
layer with one that is correct, fast, and testable.

The specific improvements are measurable:

- **Flicker**: eliminated by synchronized output and single-write frames.
- **Frame rate**: uncapped, event-driven, limited only by terminal throughput.
  At 1KB differential frames: ~867 frames/sec theoretical maximum.
- **Memory**: O(live_region_height) regardless of session length, versus Ink's
  O(total_rendered_lines) in some configurations.
- **Testability**: frame-accurate, timing-measurable, headless, CI-compatible.
- **Input**: Kitty keyboard protocol support, bracketed paste, key release events.

The terminal deserves a V8 moment. The architecture is sound, the math checks out,
and the reference implementation (pi-tui) proves the model works in production.
Twinki is the version of that model that the entire Ink ecosystem can adopt.
