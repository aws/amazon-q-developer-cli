# @twinki/testing-library

Utilities for testing Twinki apps. Drop-in replacement for `@ink/testing-library`.

## Install

```bash
npm install --save-dev @twinki/testing-library
```

## Usage

```tsx
import React from 'react';
import { Text } from 'twinki';
import { render } from '@twinki/testing-library';

const Counter = ({ count }) => <Text>Count: {count}</Text>;

const { lastFrame, rerender } = render(<Counter count={0} />);
expect(lastFrame()).toContain('Count: 0');

rerender(<Counter count={1} />);
expect(lastFrame()).toContain('Count: 1');
```

## API

### render(element, options?)

Renders a React element into a virtual terminal and returns test utilities.

#### Parameters

- `element` (ReactElement) — React component to render
- `options` (optional)
  - `columns` (number) — Terminal width in columns (default: 80)
  - `rows` (number) — Terminal height in rows (default: 24)

#### Returns

An object with the following methods and properties:

##### lastFrame()

Returns the last rendered frame as a string (viewport content with trailing empty lines trimmed).

```tsx
const { lastFrame } = render(<Text>Hello</Text>);
expect(lastFrame()).toBe('Hello');
```

##### frames

Array of all captured frames. Each frame is a `Frame` object with:
- `index` — Frame number
- `timestamp` — Nanosecond timestamp
- `viewport` — Array of terminal lines
- `writeBytes` — Bytes written to terminal
- `isFull` — Whether it was a full redraw

```tsx
const { frames } = render(<Counter count={0} />);
expect(frames.length).toBeGreaterThan(0);
expect(frames[0].isFull).toBe(true);
```

##### stdin

Object with a `write(data)` method for simulating user input.

```tsx
import { useInput, Text } from 'twinki';

const Test = () => {
  useInput((input) => {
    console.log(input); // 'hello'
  });
  return <Text>Ready</Text>;
};

const { stdin } = render(<Test />);
stdin.write('hello');
```

##### rerender(element)

Re-renders the component with new props or replaces it with a different component.

```tsx
const { rerender, lastFrame } = render(<Counter count={0} />);
expect(lastFrame()).toContain('Count: 0');

rerender(<Counter count={1} />);
expect(lastFrame()).toContain('Count: 1');
```

##### unmount()

Unmounts the component and cleans up resources.

```tsx
const { unmount } = render(<Test />);
unmount();
```

## Differences from @ink/testing-library

Twinki's testing library is designed to be compatible with Ink's, but has some differences:

1. **Richer frame data** — `frames` is an array of `Frame` objects (not just strings), giving you access to timestamps, byte counts, and full/diff indicators for performance testing.

2. **No stdout/stderr separation** — Ink has `stdout.lastFrame()` and `stderr.lastFrame()`. Twinki only has top-level `lastFrame()` since the rendering engine doesn't distinguish between streams.

3. **ANSI codes not stripped** — `lastFrame()` returns raw terminal output including ANSI escape codes. Use `.includes()` or `.toContain()` for assertions instead of exact string matches if your components use colors.

## Example Test

```tsx
import { render } from '@twinki/testing-library';
import { Text, Box } from 'twinki';

test('renders counter', () => {
  const Counter = ({ count }) => (
    <Box>
      <Text>Count: {count}</Text>
    </Box>
  );

  const { lastFrame, rerender } = render(<Counter count={0} />);
  
  expect(lastFrame()).toContain('Count: 0');
  
  rerender(<Counter count={5} />);
  expect(lastFrame()).toContain('Count: 5');
});

test('handles user input', () => {
  let receivedInput = '';
  
  const InputTest = () => {
    useInput((input) => {
      receivedInput = input;
    });
    return <Text>Type something</Text>;
  };

  const { stdin } = render(<InputTest />);
  stdin.write('hello');
  
  expect(receivedInput).toBe('hello');
});
```

## License

MIT
