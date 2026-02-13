# Card Component

The `Card` component provides a container with a vertical bar and smooth line-by-line content revelation. It uses a **context-based system** for positioning, bar control, and content pacing.

## Features

- **Line-by-line content revelation** - Content appears progressively for smooth typewriter effect
- **Context-based positioning** - Child components automatically get their line positions
- **Per-line bar control** - Child components can customize individual bar segments
- **Range-based bar control** - Components spanning multiple lines get coordinated colors
- **Background color theming** - Active/inactive states with proper spacing
- **Content-driven animation** - Animation speed matches actual content rendering pace

## Basic Usage

```tsx
import { Card } from './Card';
import { Message, MessageType } from '../messages/Message';

function MyComponent() {
  return (
    <Card active={true}>
      <Message content="Hello world" type={MessageType.AGENT} />
      <Message content="npm install" type={MessageType.DEVELOPER} />
    </Card>
  );
}
```

## Context-Based Positioning System

The `Card` provides three contexts for child components:

### 1. Position Context (`useCardPosition`)

Automatically assigns line indices to child components:

```tsx
import { useCardPosition } from '../layout/Card';

function MyComponent() {
  const { getNextIndex } = useCardPosition();
  const lineIndex = getNextIndex(); // Returns 0, 1, 2, etc. automatically

  // Use lineIndex for bar control...
}
```

### 2. Bar Control Context (`useBarControl`)

Provides methods to control bar appearance:

```tsx
import { useBarControl } from '../layout/Card';

function MyComponent() {
  const { setBarSegment, setBarRange, clearBarSegment, clearBarRange } =
    useBarControl();

  // Control bar colors and characters...
}
```

### 3. Line Revelation Context (`useLineRevelation`)

Provides the current line budget for progressive content revelation:

```tsx
import { useLineRevelation } from '../layout/Card';

function MyComponent() {
  const { maxVisibleLines } = useLineRevelation();

  // Only render content up to maxVisibleLines for smooth animation
  const visibleContent = content.slice(0, maxVisibleLines);
}
```

## Bar Control Methods

- **`setBarSegment(lineIndex, color?)`** - Set color for a single line (hex string, e.g., '#ff0000')
- **`setBarRange(startLine, endLine, color?)`** - Set color for a range of lines (hex string)
- **`clearBarSegment(lineIndex)`** - Reset a single line to default
- **`clearBarRange(startLine, endLine)`** - Reset a range of lines to default

### Bar Implementation

The vertical bar uses **background colors** instead of colored characters:

- Each bar segment is a space character with a colored background
- Colors are specified as hex strings (e.g., `'#8b5cf6'` for purple)
- Creates a solid, continuous bar with no gaps between lines
- Consistent appearance across different terminal fonts and sizes

### Terminal Optimization

The Card automatically optimizes for different terminal capabilities:

**Visual Consistency:**

- **iTerm2**: Adds a 1-character left margin to prevent background color bleeding into terminal margins
- **Other terminals**: No margin, bar positioned at the absolute left edge for maximum space

**Animation Performance:**

- **Smooth terminals** (macOS Terminal): Fine-grained animation with smaller batches
- **Synchronized output terminals** (iTerm2, Alacritty): Larger batches with flicker-free rendering
- **Automatic detection**: Uses synchronized output protocol when supported for optimal performance

**Synchronized Output:**

- Automatically enables synchronized output for supported terminals (iTerm2, Alacritty, WezTerm, Kitty)
- Large content updates are wrapped in synchronized update blocks for flicker-free rendering
- Maintains smooth bar segment updates while optimizing large content reveals

## Component Integration Examples

### Message Component (Automatic Positioning)

```tsx
export function Message({ content, type }: MessageProps) {
  const { getColor } = useTheme();
  const [lineCount, setLineCount] = useState(1);

  // Get contexts automatically
  let barControl, position, lineIndex;
  try {
    barControl = useBarControl();
    position = useCardPosition();
    lineIndex = position.getNextIndex(); // Automatic positioning!
  } catch {
    // Not within a Card, graceful fallback
    barControl = null;
  }

  // Get bar color based on message type
  const getBarColor = () => {
    switch (type) {
      case MessageType.DEVELOPER:
        return getColor('info'); // Blue for developer
      case MessageType.AGENT:
        return getColor('brand'); // Purple for agent
      default:
        return getColor('brand');
    }
  };

  useEffect(() => {
    if (barControl && lineIndex !== undefined && lineCount > 0) {
      barControl.setBarRange(
        lineIndex,
        lineIndex + lineCount - 1,
        getBarColor()
      );
    }

    return () => {
      if (barControl && lineIndex !== undefined && lineCount > 0) {
        barControl.clearBarRange(lineIndex, lineIndex + lineCount - 1);
      }
    };
  }, [barControl, lineIndex, lineCount]);

  // ... component render
}
```

### CodeDiff Component (Per-Line Control)

```tsx
export function CodeDiff({ lines }: CodeDiffProps) {
  const { getColor } = useTheme();

  let barControl, position, lineIndex;
  try {
    barControl = useBarControl();
    position = useCardPosition();
    lineIndex = position.getNextIndex();
  } catch {
    barControl = null;
  }

  useEffect(() => {
    if (barControl && lineIndex !== undefined) {
      lines.forEach((line, i) => {
        const color =
          line.type === 'added'
            ? getColor('success')
            : line.type === 'removed'
              ? getColor('error')
              : getColor('muted');

        // Set EACH line individually with different colors
        barControl.setBarSegment(lineIndex + i, color.hex);
      });
    }

    return () => {
      if (barControl && lineIndex !== undefined) {
        barControl.clearBarRange(lineIndex, lineIndex + lines.length - 1);
      }
    };
  }, [lines, lineIndex, barControl]);

  // ... component render
}
```

## Props

```tsx
interface CardProps {
  children: React.ReactNode[] | React.ReactNode;
  active?: boolean;
  animated?: boolean;
}
```

- **`children`** - React components to render (no manual props needed!)
- **`active`** - Whether the card is active (affects background color)
- **`animated`** - Whether to animate the bar growth (default: true)

## Background Colors

The Card automatically applies background colors based on state:

- **Active cards**: `backgroundElevated` (lighter background)
- **Inactive cards**: `background` (darker background)

## Line Revelation Animation

The Card controls content pacing through progressive line revelation with adaptive batching:

1. **Adaptive batching**: Small content gets smooth line-by-line animation, large content uses efficient batching
2. **Reduced flicker**: Batched updates minimize render frequency for better terminal compatibility
3. **Perfect synchronization**: Bar segments appear exactly when their corresponding content lines are revealed
4. **Content-aware timing**: Animation speed and batch size automatically adjust based on content size
5. **Graceful fallback**: Non-animated cards show all content immediately

### Animation Behavior

- **Small content** (1-20 lines): Smooth line-by-line animation (1 line per 25ms)
- **Medium content** (21-50 lines): Efficient 3-line batches (3 lines per 20ms)
- **Large content** (50+ lines): Fast 5-line batches (5 lines per 15ms)
- **Inactive cards**: No animation, full content visible immediately

### Performance Benefits

- **100-line diff**: Completes in ~300ms (20 updates) instead of 2.5 seconds (100 updates)
- **Reduced flickering**: Fewer screen updates prevent terminal flicker, especially in iTerm2
- **Scalable animation**: Performance improves automatically as content size increases
- **Terminal compatibility**: Works smoothly across all terminal applications

## Error Handling

Components gracefully handle cases where they're not within a `Card`:

```tsx
let barControl, position, lineRevelation, lineIndex;
try {
  barControl = useBarControl();
  position = useCardPosition();
  lineRevelation = useLineRevelation();
  lineIndex = position.getNextIndex();
} catch {
  // Component is not within a Card, that's okay
  barControl = null;
  lineRevelation = null;
}

// Use line budget if available, otherwise show all content
const maxLines = lineRevelation?.maxVisibleLines ?? Infinity;
```

This allows components to work both inside and outside of `Card` containers.

## Key Benefits of Context Approach

1. **No prop drilling** - Child components get positioning and line budgets automatically
2. **Clean component tree** - No React.cloneElement() complexity
3. **Type safety** - No `as any` casts needed
4. **Better debugging** - Clear component hierarchy in React DevTools
5. **Automatic positioning** - No manual `lineIndex` prop management
6. **Coordinated animation** - All child components animate in perfect sync
7. **Content-driven pacing** - Animation speed naturally matches content complexity

## Examples

See `Card.stories.ts` for comprehensive examples including:

- Message conversations with different bar colors
- Large code diffs with smooth line-by-line revelation
- Mixed content scenarios with coordinated animation
- Active/inactive states with proper spacing
- Line revelation timing demonstrations
