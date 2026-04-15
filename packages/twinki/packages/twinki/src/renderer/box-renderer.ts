import { Yoga, getComputedLayout, getBorderChars } from '../layout/yoga.js';
import { colorToAnsi } from '../utils/color-parser.js';
import { visibleWidth } from '../utils/visible-width.js';
import { sliceWithWidth } from '../utils/slice.js';
import { CONSTANTS, PROP_NAMES } from '../text/constants.js';
import type { TwinkiNode } from '../reconciler/types.js';

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
      // Truncate to innerWidth only when overflow is possible (Fix 2)
      if (resultWidth > innerWidth) {
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
function renderBoxFrame(
  width: number,
  border: any,
  borderColor: string,
  borderReset: string,
  bgCode: string,
  bgReset: string,
  pTop: number,
  pBottom: number,
  borderW: number,
  content: string[]
) {
  const lines: string[] = [];

  // Top border
  if (border) {
    lines.push(borderColor + border.topLeft + border.horizontal.repeat(width - 2) + border.topRight + borderReset);
  }

  // Top padding
  for (let i = 0; i < pTop; i++) {
    const fillWidth = (bgCode || border) ? width - borderW * 2 : 0;
    const padLine = bgCode + (border ? borderColor + border.vertical + borderReset : '') +
      ' '.repeat(fillWidth) +
      (border ? borderColor + border.vertical + borderReset : '') + bgReset;
    lines.push(padLine);
  }

  // Content lines
  lines.push(...content);

  // Bottom padding
  for (let i = 0; i < pBottom; i++) {
    const fillWidth = (bgCode || border) ? width - borderW * 2 : 0;
    const padLine = bgCode + (border ? borderColor + border.vertical + borderReset : '') +
      ' '.repeat(fillWidth) +
      (border ? borderColor + border.vertical + borderReset : '') + bgReset;
    lines.push(padLine);
  }

  // Bottom border
  if (border) {
    lines.push(borderColor + border.bottomLeft + border.horizontal.repeat(width - 2) + border.bottomRight + borderReset);
  }

  return lines;
}

export function renderBox(node: TwinkiNode, width: number, height: number, renderNodeFn: RenderNodeFn): string[] {
  const props = node.props as any;
  const hasBorder = !!props.borderStyle;
  const border = hasBorder ? getBorderChars(props.borderStyle) : null;

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

  return renderBoxFrame(width, border, borderColor, borderReset, bgCode, bgReset, pTop, pBottom, borderW, content);
}
