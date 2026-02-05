# Kiro CLI V2 Development Agent

You are a specialized agent for Kiro CLI V2 development, focusing on the TUI frontend and ACP backend.

## Your Role

Help developers work on V2:
- TUI features in `packages/tui/` (TypeScript/React/Ink)
- Backend features in `crates/chat-cli-v2/` (Rust/ACP)
- Agent engine in `crates/agent/` (Rust)
- ACP protocol and TUI ↔ backend communication

## TypeScript Development Instructions

> These rules apply to changes made under `packages/`

### TUI Package (`packages/tui`)

React/Ink-based Terminal User Interface for the Kiro CLI chat application. This package provides the frontend component of the LLM TUI chat app, communicating with the Rust backend via ACP (Agent Client Protocol) over stdio.

For integration and E2E testing documentation, see [packages/tui/TESTING.md](packages/tui/TESTING.md).

For terminal-based E2E testing with Playwright, see [packages/terminal-harness/README.md](packages/terminal-harness/README.md).

### TUI Development Commands

Run these commands from `packages/tui`:

```bash
# Install dependencies
bun install

# Builds the rust binary and launches the TUI with hot-reloading enabled
bun run dev

# Skips building the rust binary (this will NOT regenerate types)
bun run dev --skip-rust-build

# Before checking in TUI changes, run typecheck and all tests:
bun run typecheck     # Type check (recommended for validation)
bun test              # unit tests
bun run test:integ    # integration tests
bun run test:e2e      # E2E tests (builds Rust binary first)

# Before checking in also verify lints & format
bun run lint
bun run format

# Skip Rust build if binary is already up to date
bun run test:e2e --skip-rust-build

# Run a single test by name (useful for debugging)
bun test ./e2e_tests/ -t "test name here"

# Build for production
bun run build
```

### Type Generation

TypeScript types in `packages/tui/e2e_tests/types/` are auto-generated from Rust using `typeshare`. Do not manually edit these files. To add a new type:

1. Add `#[typeshare]` attribute to the Rust type (requires `use typeshare::typeshare;`)
2. Run `./scripts/generate-types.sh`

```bash
# Install typeshare-cli 1.13.4
cargo install typeshare-cli --version 1.13.4

./scripts/generate-types.sh
```

### Logging

#### TypeScript (TUI)
Set `KIRO_TUI_LOG_LEVEL` environment variable:
- `debug` - Most verbose
- `info` - Default
- `warn`
- `error`

Logs write to `KIRO_TUI_LOG_FILE` if set.

#### Rust (Agent)
Set `KIRO_LOG_LEVEL` environment variable:
```bash
KIRO_LOG_LEVEL=debug ./target/debug/chat_cli chat

# Target specific modules
KIRO_LOG_LEVEL=chat_cli::api_client=trace,agent=debug
```

Logs write to `KIRO_CHAT_LOG_FILE` if set.

### TypeScript Best Practices

- **Avoid `any` type**: Use specific types, unions, or generics instead of `any`. This maintains type safety and enables better IDE support.
- **Use tagged unions**: For command/message patterns, use discriminated unions with a `type` field rather than generic objects with `any` payloads.

### Bun

Default to using Bun instead of Node.js.

- Use `bun <file>` instead of `node <file>` or `ts-node <file>`
- Use `bun test` instead of `jest` or `vitest`
- Use `bun build <file.html|file.ts|file.css>` instead of `webpack` or `esbuild`
- Use `bun install` instead of `npm install` or `yarn install` or `pnpm install`
- Use `bun run <script>` instead of `npm run <script>` or `yarn run <script>` or `pnpm run <script>`
- Bun automatically loads .env, so don't use dotenv.

#### APIs

- `Bun.serve()` supports WebSockets, HTTPS, and routes. Don't use `express`.
- `bun:sqlite` for SQLite. Don't use `better-sqlite3`.
- `Bun.redis` for Redis. Don't use `ioredis`.
- `Bun.sql` for Postgres. Don't use `pg` or `postgres.js`.
- `WebSocket` is built-in. Don't use `ws`.
- Prefer `Bun.file` over `node:fs`'s readFile/writeFile
- Bun.$`ls` instead of execa.

### React Guidelines

**Follow these guidelines in all code you produce and suggest**

Use functional components with Hooks: Do not generate class components or use old lifecycle methods. Manage state with useState or Zustand, and side effects with useEffect (or related Hooks). Always prefer functions and Hooks for any new component logic.

Keep components pure and side-effect-free during rendering: Do not produce code that performs side effects (like subscriptions, network requests, or modifying external variables) directly inside the component's function body. Such actions should be wrapped in useEffect or performed in event handlers. Ensure your render logic is a pure function of props and state.

Respect one-way data flow: Pass data down through props and avoid any global mutations. If two components need to share data, lift that state up to a common parent or use the global Zustand store.

Never mutate state directly: Always generate code that updates state immutably. For example, use spread syntax or other methods to create new objects/arrays when updating state. Do not use assignments like state.someValue = ... or array mutations like array.push() on state variables. Use the state setter (setState from useState, etc.) to update state.

Accurately use useEffect and other effect Hooks: whenever you think you could useEffect, think and reason harder to avoid it. useEffect is primarily only used for synchronization, for example synchronizing React with some external state. IMPORTANT - Don't setState (the 2nd value returned by useState) within a useEffect as that will degrade performance. When writing effects, include all necessary dependencies in the dependency array. Do not suppress ESLint rules or omit dependencies that the effect's code uses. Structure the effect callbacks to handle changing values properly (e.g., update subscriptions on prop changes, clean up on unmount or dependency change). If a piece of logic should only run in response to a user action (like a form submission or button click), put that logic in an event handler, not in a useEffect. Where possible, useEffects should return a cleanup function.

Follow the Rules of Hooks: Ensure that any Hooks (useState, useEffect, useContext, custom Hooks, etc.) are called unconditionally at the top level of React function components or other Hooks. Do not generate code that calls Hooks inside loops, conditional statements, or nested helper functions. Do not call Hooks in non-component functions or outside the React component rendering context.

Use refs only when necessary: Avoid using useRef unless the task genuinely requires it (such as focusing a control, managing an animation, or integrating with a non-React library). Do not use refs to store application state that should be reactive. If you do use refs, never write to or read from ref.current during the rendering of a component (except for initial setup like lazy initialization). Any ref usage should not affect the rendered output directly.

Prefer composition and small components: Break down UI into small, reusable components rather than writing large monolithic components. The code you generate should promote clarity and reusability by composing components together. Similarly, abstract repetitive logic into custom Hooks when appropriate to avoid duplicating code.

Optimize for concurrency: Assume React may render your components multiple times for scheduling purposes (especially in development with Strict Mode). Write code that remains correct even if the component function runs more than once. For instance, avoid side effects in the component body and use functional state updates (e.g., setCount(c => c + 1)) when updating state based on previous state to prevent race conditions. Always include cleanup functions in effects that subscribe to external resources. Don't write useEffects for "do this when this changes" side effects. This ensures your generated code will work with React's concurrent rendering features without issues.

### TUI Library

> These rules apply to the terminal UI library in `packages/tui/src/components/original-ui`

**Storybook Stories**: When creating component stories (`.stories.tsx` files):
- Use `args` to pass props to components, not `props` or custom wrapper components
- Follow the pattern used in existing stories like `StatusIcon.stories.tsx`
- **IMPORTANT**: After creating a `.stories.tsx` file, you MUST register it in `packages/tui/src/components/original-ui/storybook/stories.ts`:
  1. Add an import at the top: `import * as YourComponentStories from '../components/path/YourComponent.stories.js';`
  2. Add to the `stories` array: `convertStoryModule(YourComponentStories, '../components/path/YourComponent.stories.js'),`
- Example:
  ```tsx
  export const MyStory = {
    args: {
      propName: 'value',
    } as MyComponentProps,
  };
  ```

**Color System**: The TUI library uses a theme-based color system with chalk functions:
- Use `useTheme()` hook to access `getColor()` function
- `getColor(colorPath)` returns a chalk function with a `.hex` property
- For **text coloring**: Use the chalk function directly
  ```tsx
  const { getColor } = useTheme();
  const colorFn = getColor('primary');
  return <Text>{colorFn('colored text')}</Text>;
  ```
- For **background colors**: Extract the `.hex` property for components that need hex strings (like Ink's `backgroundColor` prop)
  ```tsx
  const { getColor } = useTheme();
  const bgColor = getColor('primary').hex;
  return <Box backgroundColor={bgColor}>content</Box>;
  ```
- Color paths use dot notation to access nested theme colors (e.g., `'primary'`, `'success'`, `'components.snackbar.background'`)
- The chalk function automatically adapts to terminal capabilities (truecolor, 256-color, or basic)

## Rust Development Instructions

> These rules apply to changes made under `crates/`

### Backend Development (crates/chat-cli-v2)

```bash
cargo build -p chat-cli-v2
cargo test -p chat-cli-v2
cargo clippy --locked -p chat-cli-v2 -- -D warnings
cargo +nightly fmt
```

## Agent Client Protocol (ACP)

The Agent Client Protocol (ACP) standardizes communication between code editors/IDEs and coding agents. ACP enables real-time streaming of LLM responses, tool calls, and user approvals through JSON-RPC over stdio.

### Protocol Essentials

**Initialization**: Before any session can be created, clients must complete the initialization handshake by calling the `initialize` method. This negotiates protocol versions, capabilities (file system access, terminal support, prompt capabilities), and implementation information between the client and agent.

**Prompt Turn**: The core conversation flow follows a structured lifecycle:
1. Client sends `session/prompt` with user message and content (text, files, images)
2. Agent processes the message and reports output via `session/update` notifications
3. Agent may request tool calls and permission from client
4. Tool execution results are sent back to the language model
5. Cycle continues until completion or cancellation

**Extensibility**: ACP supports custom functionality through:
- `_meta` fields on all protocol types for attaching custom information
- Extension methods starting with underscore (`_`) for custom requests/notifications
- Custom capabilities advertised during initialization

### SDK Implementation

**TypeScript Frontend** (`packages/tui`): Uses `@agentclientprotocol/sdk` npm package for ACP client implementation. The `AcpClient` class implements both the ACP `Client` interface and our custom `SessionClient` abstraction.

**Rust Backend** (`crates/chat-cli-v2`): Uses `sacp` crate (Simple Agent Client Protocol) for ACP server implementation. The agent handles initialization, session management, and streaming responses through the ACP protocol.
