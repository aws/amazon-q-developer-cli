# Terminal Harness

A web-based terminal interface for testing CLI applications using **bun-pty** + **xterm.js** + **Playwright**.

## Architecture

```
Shell (bash/zsh)
    ↓
bun-pty (PTY backend)
    ↓
Bun HTTP + WebSocket server
    ↓
xterm.js (browser) - renders terminal + provides API
    ↓
Playwright - automated testing
```

## Quick Start

### 1. Install dependencies

```bash
cd packages/terminal-harness
bun install
```

### 2. Start the server

```bash
bun run start
```

Server runs at: **http://localhost:3000**

### 3. Open in browser

Navigate to: **http://localhost:3000/shell.html**

You'll see a live terminal running your shell. Type any commands!

### 4. Run tests

```bash
bun run test
```


## Tips for Writing Tests

1. Create a folder for your test under `tests/` (e.g., `tests/test-my-feature/`)
2. Add custom agents at `tests/test-my-feature/.kiro/agents/`
3. In your test, `cd` to that folder before launching the CLI
4. To test patched code, use `../../../../target/debug/chat_cli` instead of `kiro-cli`
5. ALWAYS Run with `--headed` to see the browser: `bun run test tests/my-test.spec.ts --headed`

See `tests/test-pretooluse-hook/` for a working example.
