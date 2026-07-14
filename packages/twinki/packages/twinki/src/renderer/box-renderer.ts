import { Yoga, getComputedLayout, getBorderChars } from '../layout/yoga.js';
import { colorToAnsi } from '../utils/color-parser.js';
import { visibleWidth } from '../utils/visible-width.js';
import { sliceWithWidth } from '../utils/slice.js';
import { PROP_NAMES } from '../text/constants.js';
import type { TwinkiNode } from '../reconciler/types.js';
import type { ComponentProps } from '../types/props.js';

/**
 * Type for the renderNode function that will be injected
 */
type RenderNodeFn = (node: TwinkiNode, maxWidth: number) => string[];

/**
 * Renders box children with position-aware compositing.
 * 
 * Handles both simple column layout (vertical stacking) and complex
 * position-based compositing for row layouts and absolute positioning.
 * 
 * @param node - Box node containing children
 * @param innerWidth - Available inner width
 * @param innerHeight - Available inner height  
 * @param clipOverflow - Whether to clip content that exceeds bounds
 * @param contentOffsetLeft - Left offset for content positioning
 * @param contentOffsetTop - Top offset for content positioning
 * @param renderNodeFn - Function to render individual nodes
 * @returns Array of terminal lines representing the composed content
 */
export function renderBoxChildren(
  node: TwinkiNode,
  innerWidth: number,
  innerHeight: number,
  clipOverflow: boolean,
  contentOffsetLeft = 0,
  contentOffsetTop = 0,
  renderNodeFn: RenderNodeFn,
  scrollTop = 0
): Array<{ text: string; width: number }> {
  // Check if any child has non-zero left/top (row layout or absolute positioning)
  let needsComposite = false;
  for (const child of node.children) {
    if (!child.yogaNode) continue;
    const layout = getComputedLayout(child.yogaNode);
    if (Math.floor(layout.left) - contentOffsetLeft !== 0 || Math.floor(layout.top) - contentOffsetTop !== 0) {
      needsComposite = true;
      break;
    }
  }

  if (!needsComposite) {
    // Column layout: simple vertical concatenation (width unknown, use -1 sentinel)
    const lines: Array<{ text: string; width: number }> = [];
    for (const child of node.children) {
      const childLines = renderNodeFn(child, innerWidth);
      if (child.yogaNode) {
        const top = Math.floor(getComputedLayout(child.yogaNode).top) - contentOffsetTop;
        if (top < 0) {
          // Negative margin: skip the first |top| lines from this child's output
          for (const l of childLines.slice(-top)) lines.push({ text: l, width: -1 });
          continue;
        }
      }
      for (const l of childLines) lines.push({ text: l, width: -1 });
    }
    if (clipOverflow) return lines.slice(scrollTop, scrollTop + innerHeight);
    return lines;
  }

  // Position-based compositing (row layout, etc.)
  // Yoga may undercount height when children contain wrapped text (text wrapping
  // happens after layout). Start with the Yoga height but grow dynamically so
  // wrapped lines are never clipped. clipOverflow still honours the Yoga height.
  const grid: Array<{ text: string; width: number }> = new Array(innerHeight)
    .fill(null)
    .map(() => ({ text: '', width: 0 }));

  for (const child of node.children) {
    if (!child.yogaNode) continue;
    const layout = getComputedLayout(child.yogaNode);
    const childLeft = Math.floor(layout.left) - contentOffsetLeft;
    const childTop = Math.floor(layout.top) - contentOffsetTop - scrollTop;
    // Clamp the available width for this child so it doesn't render wider
    // than the space between its left edge and the container's right edge.
    // Yoga's measure-func can over-report width when margin isn't fully
    // accounted for in the flex algorithm, causing text to wrap one column
    // too late and the last visible character to be truncated during
    // compositing.
    const childAvailableWidth = Math.max(0, innerWidth - Math.max(0, childLeft));
    const childLines = renderNodeFn(child, childAvailableWidth);

    // Detect whether this child (or any descendant text) uses `wrap="overflow"`.
    // Lines from overflow-wrapped text are allowed to exceed the container
    // width — the terminal will soft-wrap them visually. For any other wrap
    // mode, content that exceeds innerWidth is truncated to prevent broken
    // layouts.
    const childAllowsOverflow = hasOverflowDescendant(child);

    for (let i = 0; i < childLines.length; i++) {
      const row = childTop + i;
      if (row < 0) continue;
      // When overflow is clipped, respect the Yoga-computed height
      if (clipOverflow && row >= innerHeight) continue;
      // Grow grid to fit rendered content that exceeds Yoga height
      // (e.g. text wrapping produces more lines than Yoga predicted)
      while (row >= grid.length) {
        grid.push({ text: '', width: 0 });
      }
      const line = childLines[i]!;
      const lineWidth = visibleWidth(line);
      const base = grid[row]!;
      let result = base.text;
      let resultWidth = base.width;
      // Pad base to reach childLeft if needed
      if (resultWidth < childLeft) {
        result += ' '.repeat(childLeft - resultWidth);
        resultWidth = childLeft;
      } else if (resultWidth > childLeft) {
        const sliced = sliceWithWidth(result, 0, childLeft);
        result = sliced.text;
        resultWidth = sliced.width;
      }
      result += line;
      resultWidth += lineWidth;
      // Preserve base content to the RIGHT of this child (overlays: an
      // absolutely-positioned dialog must not blank the rest of the row).
      if (base.width > resultWidth) {
        const tail = sliceWithWidth(base.text, resultWidth, base.width - resultWidth);
        result += tail.text;
        resultWidth += tail.width;
      }
      // Truncate to innerWidth unless this child explicitly opted into
      // overflow wrapping via `wrap="overflow"`. That keeps standard-mode
      // layouts intact while allowing wrapDisabled content to extend past
      // the container width for terminal-native soft-wrap.
      if (resultWidth > innerWidth && !childAllowsOverflow) {
        const sliced = sliceWithWidth(result, 0, innerWidth);
        result = sliced.text;
        resultWidth = sliced.width;
      }
      grid[row] = { text: result, width: resultWidth };
    }
  }

  return grid;
}

/**
 * Returns true if any descendant text node (or a Box's overflow prop) opts
 * into overflow wrapping. Used by the box compositor to decide whether to
 * let content extend past the container width.
 * Result is cached on the node and invalidated when tree structure changes.
 */
function hasOverflowDescendant(node: TwinkiNode): boolean {
  if (node._hasOverflow !== undefined) return node._hasOverflow;
  if (node.props?.wrap === 'overflow') { node._hasOverflow = true; return true; }
  for (const child of node.children ?? []) {
    if (hasOverflowDescendant(child)) { node._hasOverflow = true; return true; }
  }
  node._hasOverflow = false;
  return false;
}

/**
 * Renders a box node with borders, padding, and background.
 * 
 * Handles:
 * - Border rendering with configurable styles and colors
 * - Padding application (top, bottom, left, right)
 * - Background color application
 * - Content positioning and compositing
 * - Overflow clipping when enabled
 * 
 * @param node - Box node to render
 * @param width - Total box width
 * @param height - Total box height
 * @param renderNodeFn - Function to render individual nodes
 * @returns Array of terminal lines representing the rendered box
 */
/**
 * Calculates border and padding dimensions for a box.
 */
function calculateBoxDimensions(node: TwinkiNode, width: number, height: number, hasBorder: boolean) {
  const pTop = node.yogaNode!.getComputedPadding(Yoga.EDGE_TOP);
  const pBottom = node.yogaNode!.getComputedPadding(Yoga.EDGE_BOTTOM);
  const pLeft = node.yogaNode!.getComputedPadding(Yoga.EDGE_LEFT);
  const pRight = node.yogaNode!.getComputedPadding(Yoga.EDGE_RIGHT);

  const borderW = hasBorder ? 1 : 0;
  const innerWidth = Math.max(0, width - pLeft - pRight - borderW * 2);
  const innerHeight = Math.max(0, height - pTop - pBottom - borderW * 2);

  return { pTop, pBottom, pLeft, pRight, borderW, innerWidth, innerHeight };
}

/**
 * Renders border and padding lines for a box.
 */
/** Box-drawing character set for one border style (see layout/yoga.ts). */
type BorderChars = ReturnType<typeof getBorderChars>;

/** Interior width between the two corner characters (0 for degenerate widths). */
function borderSpan(width: number): number {
  return Math.max(0, width - 2);
}

/** Truncate a title to fit, appending an ellipsis when cut. */
function fitTitle(title: string, maxLen: number): string {
  if (title.length <= maxLen) return title;
  return title.slice(0, Math.max(0, maxLen - 1)) + '…';
}

/** A plain border line: corner + horizontal fill + corner. */
function plainBorderLine(
  left: string, horizontal: string, right: string,
  width: number, color: string, reset: string,
): string {
  return color + left + horizontal.repeat(borderSpan(width)) + (width > 1 ? right : '') + reset;
}

/** A top border with an embedded title: `╭─ title ──────╮`. */
function titledBorderLine(
  border: BorderChars, width: number, color: string, reset: string,
  title: string, titleColor: string,
): string {
  const span = borderSpan(width);
  const label = fitTitle(title, span - 4); // room for "─ ", " ", and ≥1 trailing "─"
  const trailingFill = Math.max(0, span - (2 + label.length + 1));
  return (
    color + border.topLeft + border.horizontal + reset +
    ' ' + titleColor + label + reset + ' ' +
    color + border.horizontal.repeat(trailingFill) + (width > 1 ? border.topRight : '') + reset
  );
}

/** Render the top border line, embedding a title when one fits. */
function renderTopBorder(
  border: BorderChars, width: number, borderColor: string, borderReset: string,
  title?: string, titleColor?: string,
): string {
  const titleFits = title && borderSpan(width) >= 5;
  return titleFits
    ? titledBorderLine(border, width, borderColor, borderReset, title, titleColor || borderColor)
    : plainBorderLine(border.topLeft, border.horizontal, border.topRight, width, borderColor, borderReset);
}

/** Render the bottom border line. */
function renderBottomBorder(border: BorderChars, width: number, borderColor: string, borderReset: string): string {
  return plainBorderLine(border.bottomLeft, border.horizontal, border.bottomRight, width, borderColor, borderReset);
}

function renderBoxFrame(
  width: number,
  border: BorderChars | null,
  borderColor: string,
  borderReset: string,
  bgCode: string,
  bgReset: string,
  pTop: number,
  pBottom: number,
  borderW: number,
  content: string[],
  borderTitle?: string,
  borderTitleColor?: string
) {
  const lines: string[] = [];

  if (border) {
    lines.push(renderTopBorder(border, width, borderColor, borderReset, borderTitle, borderTitleColor));
  }

  // Top padding
  for (let i = 0; i < pTop; i++) {
    const fillWidth = (bgCode || border) ? Math.max(0, width - borderW * 2) : 0;
    const padLine = bgCode + (border ? borderColor + border.vertical + borderReset : '') +
      ' '.repeat(fillWidth) +
      (border ? borderColor + border.vertical + borderReset : '') + bgReset;
    lines.push(padLine);
  }

  // Content lines
  lines.push(...content);

  // Bottom padding
  for (let i = 0; i < pBottom; i++) {
    const fillWidth = (bgCode || border) ? Math.max(0, width - borderW * 2) : 0;
    const padLine = bgCode + (border ? borderColor + border.vertical + borderReset : '') +
      ' '.repeat(fillWidth) +
      (border ? borderColor + border.vertical + borderReset : '') + bgReset;
    lines.push(padLine);
  }

  if (border) {
    lines.push(renderBottomBorder(border, width, borderColor, borderReset));
  }

  return lines;
}

/**
 * Render a single Box node to terminal lines: composites its children (via
 * {@link renderBoxChildren}) then wraps them in the box's border, padding, and
 * background color, padding to the full Yoga-computed frame so a bordered or
 * background-painted box never collapses to content height.
 *
 * @param node - Box node to render (reads borderStyle/padding/colors from props)
 * @param width - Total available width in columns
 * @param height - Total available height in rows
 * @param renderNodeFn - Function to render individual child nodes
 * @returns Array of terminal lines representing the composed box
 */
export function renderBox(node: TwinkiNode, width: number, height: number, renderNodeFn: RenderNodeFn): string[] {
  const props = node.props as ComponentProps;
  const hasBorder = props.borderStyle !== undefined;
  const border = props.borderStyle !== undefined ? getBorderChars(props.borderStyle) : null;

  const { pTop, pBottom, pLeft, pRight, borderW, innerWidth, innerHeight } =
    calculateBoxDimensions(node, width, height, hasBorder);

  // Render children with position-aware compositing
  const childContent = renderBoxChildren(
    node,
    innerWidth,
    innerHeight,
    props.overflow === PROP_NAMES.HIDDEN,
    borderW + pLeft,
    borderW + pTop,
    renderNodeFn,
    props.scrollTop ?? 0
  );

  // Apply colors
  const ESC = String.fromCharCode(0x1b);
  const bgCode = props.backgroundColor ? `${ESC}[${colorToAnsi(props.backgroundColor, true)}m` : '';
  const bgReset = bgCode ? `${ESC}[0m` : '';
  const borderColor = props.borderColor ? `${ESC}[${colorToAnsi(props.borderColor, false)}m` : '';
  const borderReset = borderColor ? `${ESC}[0m` : '';

  // Pad content to fill the Yoga-computed frame height.
  if (border || bgCode) {
    while (childContent.length < innerHeight) childContent.push({ text: '', width: 0 });
  }

  // Format content lines with padding
  const leftPad = ' '.repeat(pLeft);
  // Trailing fill and right padding are only needed when a background color
  // or border must be painted across the full width. Without them the spaces
  // are invisible and pollute terminal selection / clipboard copies.
  const needsTrailingFill = !!(bgCode || border);
  const rightPad = needsTrailingFill ? ' '.repeat(pRight) : '';
  const content = childContent.map(({ text: line, width: lineW }) => {
    // Re-apply bgCode after any \x1b[0m (full reset) in child content
    // so the background color survives chalk/ANSI resets in text children.
    const RESET = String.fromCharCode(0x1b) + '[0m';
    const safeLine = bgCode ? line.replaceAll(RESET, RESET + bgCode) : line;
    const w = lineW >= 0 ? lineW : visibleWidth(line);
    const fill = needsTrailingFill ? Math.max(0, innerWidth - w) : 0;
    return bgCode +
      (border ? borderColor + border.vertical + borderReset : '') +
      leftPad + safeLine + ' '.repeat(fill) + rightPad +
      (border ? borderColor + border.vertical + borderReset : '') + bgReset;
  });

  const titleColorCode = props.borderTitleColor ? `${ESC}[${colorToAnsi(props.borderTitleColor, false)}m` : '';
  return renderBoxFrame(width, border, borderColor, borderReset, bgCode, bgReset, pTop, pBottom, borderW, content, props.borderTitle, titleColorCode ? titleColorCode : undefined);
}
