import React, {
  useCallback,
  useEffect,
  useLayoutEffect,
  useMemo,
  useReducer,
  useRef,
  useState,
} from 'react';
import { Box, Input, useMouse, useTwinkiContext } from '../../renderer.js';
import { Text } from '../ui/text/Text.js';
import { commentsForDocument, useAppStore } from '../../stores/app-store.js';
import { useTheme } from '../../hooks/useThemeContext.js';
import { useGlyphs } from '../../hooks/useGlyphs.js';
import { useTerminalSize } from '../../hooks/useTerminalSize.js';
import { useKeypress } from '../../hooks/useKeypress.js';
import { setMouseCaptureEnabled } from '../../utils/mouse-capture.js';
import { readBoolSetting, updateCliSetting } from '../../utils/cli-settings.js';
import { Settings } from '../../constants/settings.js';
import { layoutRows } from '../../utils/spec-review/layout.js';
import type { ReviewAction } from '../../utils/spec-review/review-actions.js';
import {
  mapReviewKey,
  REVIEW_HINT_KEY_WIDTH,
  REVIEW_KEY_HINTS,
  reviewFooterHints,
} from '../../utils/spec-review/keymap.js';

/**
 * Rows the surface spends on itself outside the body: the header, the blank line
 * under it, and the blank line above the footer. Fixed, so the body can't
 * overflow into a terminal scroll.
 */
const CHROME_ROWS = 3;
/** Stable identity, so the row layout isn't rebuilt on every render. */
const NO_COMMENTS: readonly ReviewAction[] = [];
/**
 * Reads a spec document and stages comments against it. Opened from a phase
 * checkpoint (ctrl+X) or from the `/spec view` panel (Enter); `esc` returns to
 * whatever opened it.
 */
export const SpecReviewScreen: React.FC = () => {
  const { getColor } = useTheme();
  const glyphs = useGlyphs();
  const { height, width } = useTerminalSize();
  const { tui } = useTwinkiContext();
  const review = useAppStore((s) => s.specReviewView);
  const actions = useAppStore((s) =>
    s.specReviewView
      ? commentsForDocument(
          s,
          s.specReviewView.featureName,
          s.specReviewView.document
        )
      : NO_COMMENTS
  );
  const moveCursor = useAppStore((s) => s.moveSpecReviewCursor);
  const setCursor = useAppStore((s) => s.setSpecReviewCursor);
  const moveToSection = useAppStore((s) => s.moveSpecReviewCursorToSection);
  const moveToComment = useAppStore((s) => s.moveSpecReviewCursorToComment);
  const moveToEdge = useAppStore((s) => s.moveSpecReviewCursorToEdge);
  const startComment = useAppStore((s) => s.startSpecReviewComment);
  const cancelComment = useAppStore((s) => s.cancelSpecReviewComment);
  const commitComment = useAppStore((s) => s.commitSpecReviewComment);
  const removeComment = useAppStore((s) => s.removeSpecReviewCommentAtCursor);
  const closeReview = useAppStore((s) => s.closeSpecReview);

  const composing = review?.composing ?? null;
  const [helpOpen, setHelpOpen] = useState(false);
  const onComment = !!review?.cursor.commentId;
  const footerHints = useMemo(() => reviewFooterHints(onComment), [onComment]);

  // Mouse support: scroll wheel + click-to-position, toggled with `m`.
  const [mouseEnabled, setMouseEnabled] = useState(() =>
    readBoolSetting(Settings.SPEC_REVIEW_MOUSE, true)
  );

  // The footer is one row normally, one per binding plus a closing hint while
  // help is open; the body gets what's left so nothing spills into scrollback.
  const footerRows = helpOpen ? REVIEW_KEY_HINTS.length + 1 : composing ? 2 : 1;
  const viewportRows = Math.max(1, height - CHROME_ROWS - footerRows);

  const inputRef = useRef<Input | null>(null);
  const composingRef = useRef(false);
  composingRef.current = !!composing;

  // Double-click detection: two clicks resolving to the same cursor within 300ms.
  const lastClickRef = useRef<{ key: string; time: number }>({
    key: '',
    time: 0,
  });

  const openCommentEditor = useCallback(() => {
    const draft = startComment();
    const editor = new Input();
    editor.focused = true;
    editor.setValue(draft);
    editor.onSubmit = (value: string) => {
      inputRef.current = null;
      commitComment(value);
    };
    inputRef.current = editor;
  }, [startComment, commitComment]);

  useEffect(() => {
    setMouseCaptureEnabled(mouseEnabled);
    return () => setMouseCaptureEnabled(false);
  }, [mouseEnabled]);

  const toggleMouse = useCallback(() => {
    const next = !mouseEnabled;
    setMouseEnabled(next);
    setMouseCaptureEnabled(next);
    updateCliSetting(Settings.SPEC_REVIEW_MOUSE, next).catch(() => {});
  }, [mouseEnabled]);

  // The Input renders from its own buffer, so keystrokes need an explicit
  // rerender — the store doesn't see them until the comment is submitted.
  const [, rerenderInput] = useReducer((n: number) => n + 1, 0);

  useLayoutEffect(
    () =>
      tui.addInputListener((data: string) => {
        if (!composingRef.current) return;
        inputRef.current?.handleInput(data);
        rerenderInput();
      }),
    [tui, rerenderInput]
  );

  const bullet = `${glyphs.diamond} `;
  const rows = useMemo(
    () => layoutRows(review?.lines ?? [], actions, width, bullet),
    [review?.lines, actions, width, bullet]
  );

  /** First row of whatever the cursor is on, so the window can follow it. */
  const cursorRow = useMemo(() => {
    if (!review) return 0;
    const { lineIndex, commentId } = review.cursor;
    const at = rows.findIndex((row) =>
      commentId
        ? row.commentId === commentId && !row.continues
        : row.kind === 'line' && row.lineIndex === lineIndex && !row.continues
    );
    return at < 0 ? 0 : at;
  }, [review, rows]);

  // The window follows the cursor rather than being stored: it depends on the
  // terminal's size, which the store has no business knowing. When the cursor
  // is already visible, the window stays put (avoids a jarring re-center on
  // mouse click); it only scrolls when the cursor moves out of view.
  // NOTE: ref written in useMemo — safe under twinki's synchronous renderer;
  // would need rework if the renderer ever gains concurrent-mode semantics.
  const scrollOffsetRef = useRef(0);
  const scrollOffset = useMemo(() => {
    const last = Math.max(0, rows.length - viewportRows);
    const prev = scrollOffsetRef.current;
    let next = prev;
    if (cursorRow < prev) {
      // Cursor moved above the viewport — scroll up to keep it visible.
      next = cursorRow;
    } else if (cursorRow >= prev + viewportRows) {
      // Cursor moved below the viewport — scroll down to keep it visible.
      next = cursorRow - viewportRows + 1;
    }
    next = Math.max(0, Math.min(next, last));
    scrollOffsetRef.current = next;
    return next;
  }, [cursorRow, rows.length, viewportRows]);

  useMouse(
    useCallback(
      (event: { type: string }) => {
        if (composingRef.current || helpOpen) return;
        if (event.type === 'scrollup') {
          moveCursor(-3);
        } else if (event.type === 'scrolldown') {
          moveCursor(3);
        }
      },
      [moveCursor, helpOpen]
    ),
    { isActive: mouseEnabled }
  );

  useKeypress((input, key) => {
    if (!review) return;
    if (composing) {
      if (key.escape) {
        inputRef.current = null;
        cancelComment();
      }
      return;
    }
    const intent = mapReviewKey(input, key);
    if (!intent) return;
    if (helpOpen) {
      // The help overlay owns the next keystroke so a key pressed to dismiss it
      // doesn't also move the cursor underneath.
      if (intent.type === 'help' || intent.type === 'close') setHelpOpen(false);
      return;
    }
    switch (intent.type) {
      case 'close':
        closeReview();
        return;
      case 'help':
        setHelpOpen(true);
        return;
      case 'move-line':
        moveCursor(intent.delta);
        return;
      case 'move-page':
        moveCursor(
          intent.direction * Math.max(1, Math.floor(viewportRows / 2))
        );
        return;
      case 'move-edge':
        moveToEdge(intent.edge);
        return;
      case 'jump-section':
        moveToSection(intent.direction);
        return;
      case 'jump-comment':
        moveToComment(intent.direction);
        return;
      case 'comment': {
        if (review.error) return;
        openCommentEditor();
        return;
      }
      case 'comment-delete':
        if (onComment) removeComment();
        return;
      case 'toggle-mouse':
        toggleMouse();
        return;
    }
  });

  if (!review) return null;

  const primary = getColor('primary');
  const secondary = getColor('secondary');
  const success = getColor('success');
  const accent = getColor('warning');

  const visible = rows.slice(scrollOffset, scrollOffset + viewportRows);
  const staged = actions.length;

  return (
    <Box flexDirection="column" width="100%">
      <Box>
        <Text>
          {primary(`Review ${review.document}.md`)}
          {secondary(` ${glyphs.smallDot} ${review.featureName}`)}
          {staged > 0 &&
            success(
              ` ${glyphs.smallDot} ${staged} comment${
                staged === 1 ? '' : 's'
              } staged`
            )}
        </Text>
      </Box>

      {review.error ? (
        <Box marginTop={1}>
          <Text>{getColor('error')(review.error)}</Text>
        </Box>
      ) : (
        <Box flexDirection="column" marginTop={1}>
          {visible.map((row, offset) => {
            const focused =
              row.kind === 'comment'
                ? review.cursor.commentId === row.commentId
                : !review.cursor.commentId &&
                  row.lineIndex === review.cursor.lineIndex;
            const marker =
              focused && !row.continues ? primary(glyphs.chevron) : ' ';
            const body = `${row.bullet}${row.text}`;
            const paint =
              row.kind === 'comment'
                ? focused
                  ? accent.bold
                  : accent
                : focused
                  ? primary
                  : secondary;
            const rowIndex = scrollOffset + offset;
            return (
              <Box
                key={`${rowIndex}`}
                width={width}
                onClick={
                  mouseEnabled
                    ? () => {
                        if (composingRef.current || helpOpen) return;
                        const targetLine = row.lineIndex;
                        const targetComment =
                          row.kind === 'comment'
                            ? (row.commentId ?? null)
                            : null;
                        const now = Date.now();
                        const last = lastClickRef.current;
                        const clickKey = `${targetLine}:${targetComment ?? ''}`;
                        const isDoubleClick =
                          last.key === clickKey && now - last.time < 300;
                        lastClickRef.current = { key: clickKey, time: now };

                        setCursor(targetLine, targetComment);

                        if (isDoubleClick && !review?.error) {
                          openCommentEditor();
                        }
                      }
                    : undefined
                }
              >
                <Text wrap="truncate-end">
                  {marker}
                  {row.indent}
                  {paint(body || ' ')}
                </Text>
              </Box>
            );
          })}
        </Box>
      )}

      <Box marginTop={1} flexDirection="column">
        {composing ? (
          <>
            <Box>
              <Text>
                {accent(`${glyphs.diamond} `)}
                {(
                  inputRef.current?.render(Math.max(1, width - 8))[0] ?? ''
                ).slice(2)}
              </Text>
            </Box>
            <Box>
              <Text>
                {primary(glyphs.enter)} {secondary('save')}
                {secondary(` ${glyphs.smallDot} `)}
                {primary('esc')} {secondary('cancel')}
              </Text>
            </Box>
          </>
        ) : helpOpen ? (
          <>
            {REVIEW_KEY_HINTS.map((hint) => (
              <Box key={hint.keys}>
                <Text>
                  {primary(hint.keys.padEnd(REVIEW_HINT_KEY_WIDTH))}
                  {secondary(hint.action)}
                </Text>
              </Box>
            ))}
            <Box>
              <Text>
                {primary('?')} {secondary('or')} {primary('esc')}{' '}
                {secondary('to close this list')}
              </Text>
            </Box>
          </>
        ) : (
          <Box>
            <Text wrap="truncate-end">
              {footerHints.map((hint, i) => {
                const isLast = i === footerHints.length - 1;
                return (
                  <React.Fragment key={hint.action}>
                    {i > 0 && secondary(` ${glyphs.smallDot} `)}
                    {isLast && (
                      <>
                        {primary('m')}{' '}
                        {secondary(`mouse:${mouseEnabled ? 'on' : 'off'}`)}
                        {secondary(` ${glyphs.smallDot} `)}
                      </>
                    )}
                    {primary(
                      hint.glyph === 'arrows'
                        ? `${glyphs.arrowUp}${glyphs.arrowDown}`
                        : hint.glyph === 'enter'
                          ? glyphs.enter
                          : hint.keys
                    )}{' '}
                    {secondary(hint.action)}
                  </React.Fragment>
                );
              })}
            </Text>
          </Box>
        )}
      </Box>
    </Box>
  );
};
