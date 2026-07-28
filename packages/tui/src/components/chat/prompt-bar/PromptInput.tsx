import { Box, CURSOR_MARKER } from './../../../renderer.js';
import React, {
  useEffect,
  useRef,
  useState,
  useLayoutEffect,
  useCallback,
  useMemo,
} from 'react';
import { useStore } from 'zustand';
import { useTheme } from '../../../hooks/useThemeContext.js';
import { useKeypress, type Key } from '../../../hooks/useKeypress.js';
import { useGlyphs, useAllowAsciiArt } from '../../../hooks/useGlyphs.js';
import { getBarRamp } from '../../../utils/glyphs.js';
import { Text } from '../../ui/text/Text.js';
import { useAppStore } from '../../../stores/app-store.js';
import { chalk } from '../../../utils/color.js';
import { workflowStore } from '../../../stores/workflow-store.js';
import { PastedChip, shouldCollapsePaste } from './PastedChip.js';
import { FileChip } from './FileChip.js';
import {
  normalizeLineEndings,
  isPrintable,
  unescapeShellPath,
  stripNonPrintable,
} from '../../../utils/index.js';
import { computeInputSpans } from '../../../utils/input-syntax.js';
import {
  isCommandVisibleInUiMode,
  atMenuShowsPrompts,
} from '../../ui/command-menu-utils.js';
import { completePathAtCursor } from '../../../utils/path-completion.js';
import { logger } from '../../../utils/logger.js';
import { inputMetrics } from '../../../utils/inputMetrics.js';
import {
  useCommandState,
  useCommandActions,
  useFileAttachmentState,
  useFileAttachmentActions,
  useKiroClient,
  useImageAttachmentActions,
} from '../../../stores/selectors.js';
import { useKeybindings } from '../../../hooks/useKeybindings.js';
import {
  type Segment,
  type FileSegment,
  type PasteSegment,
  type ImageSegment,
  segmentWidth,
  totalWidth,
  getVisibleText,
  locateCursor,
  normalizeSegments,
  deleteWordBackward,
  deleteWordForward,
  deleteForward,
  moveWordForward,
  moveWordBackward,
  transposeChars,
  uppercaseWord,
  lowercaseWord,
  capitalizeWord,
  transposeWords,
  isVisuallyMultiLine,
  moveCursorUpVisual,
  moveCursorDownVisual,
  moveToLogicalLineStart,
  moveToLogicalLineEnd,
  killToLogicalLineEnd,
  killToLogicalLineBeginning,
  expandPasteSegment,
} from '../../../utils/input-editing.js';
import { CommandHistory } from '../../../utils/command-history.js';
import {
  type ReverseSearchState,
  createReverseSearchState,
  enterSearch,
  appendQuery,
  backspaceQuery,
  cycleOlder,
  exitSearch,
  abortSearch,
} from '../../../utils/reverse-search.js';
import {
  navigateQueueUp,
  navigateQueueDown,
  commitQueueRestore,
  buildUnifiedQueueEntries,
  type QueueRestoreState,
  type QueueReplace,
  type UnifiedQueueEntry,
} from '../../../utils/queue-navigation.js';
// TODO: Long-term, PromptInput should migrate to use Twinki's Input/TextInput
// component (or a segment-aware extension of it) instead of reimplementing
// editing logic. For now we import just the KillRing utility.
import { KillRing } from 'twinki';
import { useTerminalSize } from '../../../hooks/useTerminalSize.js';
import {
  startPTTRecording,
  type PTTSession,
} from '../../../commands/voice-helper.js';

/** Visual cursor (inverse block) + hardware cursor marker (APC sequence for twinki IME positioning). */
const EXPAND_HINT = 'Press Tab to expand';
const CursorBlock = ({
  char = ' ',
  suppressMarker = false,
}: {
  char?: string;
  suppressMarker?: boolean;
}) => (
  <>
    {!suppressMarker && <Text>{CURSOR_MARKER}</Text>}
    <Text inverse>{char}</Text>
  </>
);

export interface TriggerRule {
  key: string;
  type: 'start' | 'inline';
}

export interface TriggerInfo {
  key: string;
  position: number;
  type: 'start' | 'inline';
}

export interface PromptInputProps {
  onSubmit: (command: string) => void;
  isProcessing: boolean;
  triggerRules?: TriggerRule[];
  onTriggerDetected?: (trigger: TriggerInfo | null) => void;
  placeholder?: string;
  /**
   * When true, unmodified ↑/↓ are forwarded to whatever component owns the
   * focus instead of editing the input or navigating history. Used by lite
   * mode's subagent panel so the user can scroll/cycle while the input stays
   * mounted for typing.
   */
  suppressArrows?: boolean;
}

// buildContent is defined and exported here in PromptInput.tsx so tests
// can import it directly. Keep the implementation tiny: join the segments
// verbatim; no whitespace collapse or trim. Collapsing runs of spaces or
// trimming leading whitespace would destroy indentation in pasted code.
// Empty-submit is guarded at the call sites via `content.trim()`.
export const buildContent = (segments: Segment[]): string => {
  const parts = segments.map((s) => {
    if (s.type === 'text') return s.value;
    if (s.type === 'file') return ` @file:${s.filePath} `;
    if (s.type === 'paste') return s.content;
    // Images are handled separately via extractImages
    return '';
  });
  return parts.join('');
};

// Detect trigger patterns
const detectTrigger = (
  text: string,
  cursor: number,
  rules: TriggerRule[]
): TriggerInfo | null => {
  for (const rule of rules) {
    if (rule.type === 'start' && text.startsWith(rule.key)) {
      // Only trigger when cursor is within the command name (before first space)
      const spaceIndex = text.indexOf(' ');
      if (spaceIndex === -1 || cursor <= spaceIndex) {
        return { key: rule.key, position: 0, type: rule.type };
      }
    }
    if (rule.type === 'inline') {
      const lastIndex = text.slice(0, cursor).lastIndexOf(rule.key);
      if (lastIndex !== -1) {
        return { key: rule.key, position: lastIndex, type: rule.type };
      }
    }
  }
  return null;
};

export const PromptInput = React.memo(function PromptInput({
  onSubmit,
  isProcessing,
  triggerRules = [],
  onTriggerDetected,
  placeholder,
  suppressArrows = false,
}: PromptInputProps) {
  const {
    activeTrigger,
    filePickerHasResults,
    commandInputValue,
    promptHint,
    slashCommands,
    activeCommand,
    commandShadowText,
  } = useCommandState();
  const {
    setCommandInput,
    clearCommandInput,
    setPromptHint,
    setActiveCommand,
  } = useCommandActions();
  const toggleInterruptMode = useAppStore((s) => s.toggleInterruptMode);
  // Panels that own the keyboard via their own useInput (repo picker, session
  // picker, source-provider gate). While one is open the prompt must not also
  // consume keystrokes, or typing (e.g. the picker's type-to-search) echoes in
  // both places at once.
  const appInputPanelOpen = useAppStore(
    (s) =>
      s.showRepoPicker ||
      s.showSessionPicker ||
      s.showSourceProviderGate ||
      s.activityTrayExpanded
  );
  const specDescriptionPending = useAppStore(
    (s) => s.pendingSpecDescription !== null
  );
  const cancelSpecDescription = useAppStore(
    (s) => s.cancelPendingSpecDescription
  );
  const workflowHistoryOpen = useStore(
    workflowStore,
    (state) => state.history.isOpen
  );
  const inputPanelOpen = appInputPanelOpen || workflowHistoryOpen;
  const keybindings = useKeybindings();
  const glyphs = useGlyphs();
  const { allowAsciiArt } = useAllowAsciiArt();
  const resolvedPlaceholder =
    placeholder ?? `ask a question, or describe a task ${glyphs.enter}`;
  const voiceStop = useAppStore((s) => s.voiceStop);
  const voiceLevel = useAppStore((s) => s.voiceLevel);
  const voiceAutoSubmit = useAppStore((s) => s.voiceAutoSubmit);
  const voiceHintIndex = useAppStore((s) => s.voiceHintIndex);
  const pendingVoiceText = useAppStore((s) => s.pendingVoiceText);
  const setVoiceLevel = useAppStore((s) => s.setVoiceLevel);
  const setVoicePartialText = useAppStore((s) => s.setVoicePartialText);
  const voicePartialText = useAppStore((s) => s.voicePartialText);
  const setVoiceStop = useAppStore((s) => s.setVoiceStop);
  const setVoiceCancel = useAppStore((s) => s.setVoiceCancel);
  const setPendingVoiceText = useAppStore((s) => s.setPendingVoiceText);
  const incrementVoiceHint = useAppStore((s) => s.incrementVoiceHint);
  const showTransientAlert = useAppStore((s) => s.showTransientAlert);
  const sendMessage = useAppStore((s) => s.sendMessage);
  const { pendingFileAttachment } = useFileAttachmentState();
  const { consumePendingFileAttachment } = useFileAttachmentActions();
  const { kiro } = useKiroClient();
  const { addPendingImage } = useImageAttachmentActions();
  const [segments, _setSegments] = useState<Segment[]>([
    { type: 'text', value: '' },
  ]);
  const [cursor, _setCursor] = useState(0);
  const [pathCandidates, setPathCandidates] = useState<string[]>([]);
  const setStoreReverseSearchActive = useAppStore(
    (state) => state.setReverseSearchActive
  );
  const reverseSearchRef = useRef<ReverseSearchState>(
    createReverseSearchState()
  );
  const [_reverseSearchActive, setReverseSearchActive] = useState(false);

  // Clean up store flag if component unmounts while reverse search is active
  useEffect(() => {
    return () => setStoreReverseSearchActive(false);
  }, [setStoreReverseSearchActive]);

  // Queue-aware ↑/↓ navigation (lite mode only): ↑ pulls a queued message back
  // into the buffer for editing while leaving its slot in place, so Kiro still
  // processes it in order. State machine: utils/queue-navigation.ts.
  const queueRestoreRef = useRef<QueueRestoreState | null>(null);
  const isLiteMode = useAppStore((state) => state.uiMode === 'lite');
  const queuedMessagesRef = useRef<readonly string[]>([]);
  queuedMessagesRef.current = useAppStore((state) => state.queuedMessages);
  // Unified ↑/↓ nav source: steer lines (mid-turn backend message, echoed as
  // pendingSteerContent) come FIRST because the backend injects the steer
  // before the local queue drains, then the editable queuedMessages. The
  // nav/commit machine routes each entry to its own transport by `kind`, so
  // a steer edit becomes a clear-and-resteer — never a queuedMessages append
  // (which would double-send: once via processQueue, once via the backend's
  // own injection). See utils/queue-navigation.ts.
  const pendingSteerContentRef = useRef<string | null>(null);
  pendingSteerContentRef.current = useAppStore(
    (state) => state.pendingSteerContent
  );
  const unifiedEntriesRef = useRef<readonly UnifiedQueueEntry[]>([]);
  unifiedEntriesRef.current = buildUnifiedQueueEntries(
    pendingSteerContentRef.current,
    queuedMessagesRef.current
  );
  const replaceQueuedMessage = useAppStore(
    (state) => state.replaceQueuedMessage
  );
  const replaceSteerMessage = useAppStore((state) => state.replaceSteerMessage);
  const clearSteerMessage = useAppStore((state) => state.clearSteerMessage);
  // Mirror queue-restore state into the store (the source of truth for the
  // "editing queued #N" header) so external index flips stay in sync.
  const setEditingQueueIndex = useAppStore(
    (state) => state.setEditingQueueIndex
  );
  const setEditingSteerLineIndex = useAppStore(
    (state) => state.setEditingSteerLineIndex
  );
  const removeQueuedMessage = useAppStore((state) => state.removeQueuedMessage);

  // Reflect a restore-state transition into the store's editing-chevron flags.
  // null → clear both; queue → editingQueueIndex; steer → editingSteerLineIndex.
  const setEditingEntry = useCallback(
    (state: QueueRestoreState | null) => {
      if (state == null) {
        setEditingQueueIndex(null);
        setEditingSteerLineIndex(null);
        return;
      }
      if (state.kind === 'queue') {
        setEditingQueueIndex(state.queueIndex ?? null);
      } else {
        setEditingSteerLineIndex(state.index);
      }
    },
    [setEditingQueueIndex, setEditingSteerLineIndex]
  );

  // Apply a dirty-commit instruction emitted by the nav machine when stepping
  // away from an edited entry. Queue → in-place replace; steer → resteer.
  const applyQueueReplace = useCallback(
    (replace: QueueReplace) => {
      if (replace.kind === 'queue') {
        replaceQueuedMessage(replace.queueIndex, replace.text);
      } else {
        replaceSteerMessage(replace.text, replace.targetLine);
      }
    },
    [replaceQueuedMessage, replaceSteerMessage]
  );

  // Refs shadow the latest state so input handlers never read stale closures.
  // Without these, keypresses arriving faster than React re-renders would
  // read the old segments/cursor and overwrite each other's edits.
  const segmentsRef = useRef(segments);
  const cursorRef = useRef(cursor);
  const shellEscapeActive = useAppStore((state) => state.isShellEscape);
  const setSegments = useCallback(
    (s: Segment[] | ((prev: Segment[]) => Segment[])) => {
      if (typeof s === 'function') {
        _setSegments((prev) => {
          const next = s(prev);
          segmentsRef.current = next;
          return next;
        });
      } else {
        segmentsRef.current = s;
        _setSegments(s);
      }
    },
    []
  );
  const setCursor = useCallback((c: number | ((prev: number) => number)) => {
    if (typeof c === 'function') {
      _setCursor((prev) => {
        const next = c(prev);
        cursorRef.current = next;
        return next;
      });
    } else {
      cursorRef.current = c;
      _setCursor(c);
    }
  }, []);

  const killRingRef = useRef(new KillRing());
  const _lastKillActionRef = useRef<'kill' | null>(null);
  const lastYankRef = useRef<{ start: number; length: number } | null>(null);
  // Tracks whether we set promptHint — cleared on next keypress in useKeypress
  const expandHintActive = useRef(false);
  const undoStack = useRef<Array<{ segments: Segment[]; cursor: number }>>([]);
  const lastUndoPushTime = useRef(0);

  const { getColor, getUserPromptColor } = useTheme();
  const { width: termWidth } = useTerminalSize();
  const prevTriggerRef = useRef<TriggerInfo | null>(null);
  const suppressNextTriggerRef = useRef(false);

  // Push-to-talk refs
  const SPACE_HOLD_MS = Number(process.env.KIRO_PTT_HOLD_MS ?? 1500);
  const SPACE_RELEASE_TIMEOUT_MS = 550;
  const spaceHoldStartRef = useRef<number | null>(null);
  const spaceReleaseTimerRef = useRef<ReturnType<typeof setTimeout> | null>(
    null
  );
  const spaceActivateTimerRef = useRef<ReturnType<typeof setTimeout> | null>(
    null
  );
  const pttSessionRef = useRef<PTTSession | null>(null);
  const pttActiveRef = useRef(false);
  const voiceCancelledRef = useRef(false);

  const primaryColor = useMemo(
    () => getUserPromptColor(),
    [getUserPromptColor]
  );
  const brandColor = useMemo(() => getColor('brand'), [getColor]);
  const linkColor = useMemo(() => getColor('link'), [getColor]);
  const isShellEscape = useMemo(
    () => getVisibleText(segments).startsWith('!'),
    [segments]
  );
  // Only colorize recognized commands so a leading path (`/tmp/foo`) doesn't
  // get the command color and read as two-toned.
  const knownSlashNames = useMemo(
    () => new Set(slashCommands.map((c) => c.name)),
    [slashCommands]
  );
  // Path tokens stay primary on purpose — cyan was hard to read on light
  // themes and the attachment chip already disambiguates real attachments.
  const stylePromptText = useCallback(
    (text: string): string => {
      if (!text) return '';
      const spans = computeInputSpans(text, knownSlashNames).filter(
        (s) => s.kind !== 'path'
      );
      if (spans.length === 0) return primaryColor(text);
      const out: string[] = [];
      let i = 0;
      for (const span of spans) {
        if (span.start > i) out.push(primaryColor(text.slice(i, span.start)));
        const tokenText = text.slice(span.start, span.end);
        if (span.kind === 'slash') {
          out.push(brandColor(tokenText));
        } else if (span.kind === 'url') {
          // Underline so URLs read as links even when the theme link
          // color matches surrounding text.
          out.push(chalk.underline(linkColor(tokenText)));
        }
        i = span.end;
      }
      if (i < text.length) out.push(primaryColor(text.slice(i)));
      return out.join('');
    },
    [primaryColor, brandColor, linkColor, knownSlashNames]
  );
  const styleInputText = useCallback(
    (text: string, isFirstSegment: boolean) => {
      if (isShellEscape && isFirstSegment && text.length > 0) {
        if (text.startsWith('!')) {
          return brandColor('!') + stylePromptText(text.slice(1));
        }
      }
      return stylePromptText(text);
    },
    [isShellEscape, brandColor, stylePromptText]
  );
  const placeholderColor = useMemo(() => getColor('muted'), [getColor]);

  const voiceRamp = getBarRamp(allowAsciiArt);
  const voiceCursorChar =
    voiceLevel !== null
      ? (voiceRamp[Math.min(voiceLevel, 7)] ?? voiceRamp[0])
      : null;

  const VOICE_HINTS = [
    'hold SPACE for quick push-to-talk',
    `auto-submit is ${voiceAutoSubmit ? 'ON' : 'OFF'} ${glyphs.smallDot} /settings voice.autoSubmit`,
    `text appears as you pause ${glyphs.smallDot} /settings voice.partialPause (ms)`,
    `silence auto-stops after 5s ${glyphs.smallDot} /settings voice.silenceTimeout (s)`,
    '/settings voice.modelSize base|small',
    'Ctrl+C cancels recording',
  ];
  const voiceHint = VOICE_HINTS[voiceHintIndex % VOICE_HINTS.length] ?? null;

  useEffect(() => {
    if (localSyncRef.current) {
      localSyncRef.current = false;
      return;
    }
    const visibleText = getVisibleText(segments);
    const firstSeg = segments[0];
    if (
      commandInputValue !== visibleText &&
      segments.length === 1 &&
      firstSeg?.type === 'text'
    ) {
      setSegments([{ type: 'text', value: commandInputValue }]);
      setCursor(commandInputValue.length);
    }
  }, [commandInputValue]);

  // Consume pending file attachment
  useEffect(() => {
    if (pendingFileAttachment) {
      const pending = consumePendingFileAttachment();
      if (pending) {
        const { path: filePath, triggerPosition } = pending;
        const fs = require('fs');
        try {
          const content = fs.readFileSync(filePath, 'utf-8');
          const lines = content.split(/\r\n|\r|\n/);
          const fileSegment: FileSegment = {
            type: 'file',
            filePath,
            content,
            lineCount: lines.length,
          };

          // Use stored trigger position to find where @query starts
          const { segIdx, offset } = locateCursor(segments, triggerPosition);
          const { segIdx: endSegIdx, offset: endOffset } = locateCursor(
            segments,
            cursor
          );
          const seg = segments[segIdx];

          if (seg?.type === 'text' && segIdx === endSegIdx) {
            // Replace @query with file chip
            const newSegs = [
              ...segments.slice(0, segIdx),
              { type: 'text' as const, value: seg.value.slice(0, offset) },
              fileSegment,
              { type: 'text' as const, value: seg.value.slice(endOffset) },
              ...segments.slice(segIdx + 1),
            ];
            const normalized = normalizeSegments(newSegs);
            setSegments(normalized);
            // Position cursor after the chip
            let newCursor = 0;
            for (const s of normalized) {
              if (
                s === fileSegment ||
                (s.type === 'file' && s.filePath === filePath)
              ) {
                newCursor += 1;
                break;
              }
              newCursor += segmentWidth(s);
            }
            setCursor(newCursor);
            syncToStore(normalized);
          } else {
            // Fallback: insert at trigger position
            if (seg?.type === 'text') {
              const newSegs = [
                ...segments.slice(0, segIdx),
                { type: 'text' as const, value: seg.value.slice(0, offset) },
                fileSegment,
                { type: 'text' as const, value: seg.value.slice(offset) },
                ...segments.slice(segIdx + 1),
              ];
              const normalized = normalizeSegments(newSegs);
              setSegments(normalized);
              setCursor(triggerPosition + 1);
              syncToStore(normalized);
            }
          }
        } catch {
          // ignore
        }
      }
    }
  }, [pendingFileAttachment]);

  // Trigger detection
  useEffect(() => {
    if (!onTriggerDetected) return;
    // Skip trigger detection when content was restored from history navigation
    if (suppressNextTriggerRef.current) {
      suppressNextTriggerRef.current = false;
      onTriggerDetected(null);
      prevTriggerRef.current = null;
      return;
    }
    const text = getVisibleText(segments);
    const trigger = detectTrigger(text, cursor, triggerRules);
    const prev = prevTriggerRef.current;
    const changed =
      (trigger === null) !== (prev === null) ||
      (trigger &&
        prev &&
        (trigger.key !== prev.key || trigger.position !== prev.position));
    if (changed) {
      onTriggerDetected(trigger);
      prevTriggerRef.current = trigger;
    }
  }, [segments, cursor, triggerRules, onTriggerDetected]);

  useLayoutEffect(() => {
    inputMetrics.markRenderComplete();
  });

  const localSyncRef = useRef(false);

  const syncToStore = useCallback(
    (segs: Segment[]) => {
      const text = getVisibleText(segs);
      localSyncRef.current = true;
      setCommandInput(text);

      // Clear promptHint when user starts typing args (space after command)
      if (promptHint && text.startsWith('/') && text.includes(' ')) {
        setPromptHint(null);
      }
    },
    [setCommandInput, setPromptHint, promptHint]
  );

  // Insert voice transcription text at cursor position
  const insertVoiceText = useCallback(
    (text: string) => {
      const segs = segmentsRef.current;
      const cur = cursorRef.current;
      if (totalWidth(segs) === 0) {
        const newSegs: Segment[] = [{ type: 'text', value: text }];
        setSegments(newSegs);
        setCursor(text.length);
        syncToStore(newSegs);
      } else {
        const { segIdx, offset } = locateCursor(segs, cur);
        const seg = segs[segIdx];
        if (seg?.type === 'text') {
          const needsLeadingSpace = offset > 0 && seg.value[offset - 1] !== ' ';
          const insert = (needsLeadingSpace ? ' ' : '') + text;
          const newValue =
            seg.value.slice(0, offset) + insert + seg.value.slice(offset);
          const newSegs = [...segs];
          newSegs[segIdx] = { type: 'text', value: newValue };
          setSegments(newSegs);
          setCursor(cur + insert.length);
          syncToStore(newSegs);
        }
      }
    },
    [syncToStore, setSegments, setCursor]
  );

  // Handle pendingVoiceText from /voice command dispatcher (uses store as relay)
  useEffect(() => {
    if (pendingVoiceText !== null) {
      insertVoiceText(pendingVoiceText);
      setPendingVoiceText(null);
    }
  }, [pendingVoiceText, insertVoiceText, setPendingVoiceText]);

  const clearAll = () => {
    setSegments([{ type: 'text', value: '' }]);
    setCursor(0);
    clearCommandInput();
  };

  const insertText = (raw: string) => {
    const text = stripNonPrintable(raw);
    if (!text) return;
    pushUndo();
    inputMetrics.markStateUpdate();
    const segs = segmentsRef.current;
    const cur = cursorRef.current;
    const { segIdx, offset } = locateCursor(segs, cur);
    const seg = segs[segIdx];

    if (seg?.type === 'text') {
      const newValue =
        seg.value.slice(0, offset) + text + seg.value.slice(offset);
      const newSegs = [...segs];
      newSegs[segIdx] = { type: 'text', value: newValue };
      setSegments(newSegs);
      setCursor(cur + text.length);
      syncToStore(newSegs);
    } else if (seg) {
      // On a chip - insert text after it
      const newSegs = [
        ...segs.slice(0, segIdx + 1),
        { type: 'text' as const, value: text },
        ...segs.slice(segIdx + 1),
      ];
      setSegments(normalizeSegments(newSegs));
      setCursor(cur + text.length);
      syncToStore(newSegs);
    }
  };

  const handlePaste = (pastedText: string) => {
    // Unescape shell-escaped file paths from drag-and-drop (e.g. macOS Finder)
    const unescaped = unescapeShellPath(pastedText);
    const normalized = normalizeLineEndings(unescaped);
    const result = shouldCollapsePaste(normalized);

    if (result.shouldCollapse) {
      const pasteSegment: PasteSegment = {
        type: 'paste',
        content: normalized,
        lineCount: result.lineCount,
        charCount: normalized.length,
      };
      const segs = segmentsRef.current;
      const cur = cursorRef.current;
      const { segIdx, offset } = locateCursor(segs, cur);
      const seg = segs[segIdx];

      if (seg?.type === 'text') {
        pushUndo();
        const newSegs = normalizeSegments([
          ...segs.slice(0, segIdx),
          { type: 'text', value: seg.value.slice(0, offset) },
          pasteSegment,
          { type: 'text', value: seg.value.slice(offset) },
          ...segs.slice(segIdx + 1),
        ]);
        setSegments(newSegs);
        setCursor(cur + 1);
        syncToStore(newSegs);
        setPromptHint(EXPAND_HINT);
        expandHintActive.current = true;
      } else if (seg) {
        // Cursor on a chip — insert paste chip at cursor position
        pushUndo();
        const insertIdx = offset === 0 ? segIdx : segIdx + 1;
        const newSegs = normalizeSegments([
          ...segs.slice(0, insertIdx),
          pasteSegment,
          ...segs.slice(insertIdx),
        ]);
        setSegments(newSegs);
        setCursor(cur + 1);
        syncToStore(newSegs);
        setPromptHint(EXPAND_HINT);
        expandHintActive.current = true;
      }
      return;
    }

    insertText(normalized);
  };

  const handlePasteImage = async () => {
    try {
      const result = await kiro.executeCommand({
        command: 'paste',
        args: {},
      });
      if (!result.success) {
        // No image in clipboard or error — fall back to normal paste
        return false;
      }
      const data = result.data as {
        data: string;
        mimeType: string;
        width: number;
        height: number;
        sizeBytes: number;
      };
      const imageSegment: ImageSegment = {
        type: 'image',
        base64: data.data,
        mimeType: data.mimeType,
        width: data.width,
        height: data.height,
        sizeBytes: data.sizeBytes,
      };
      // Insert image segment at cursor
      const { segIdx, offset } = locateCursor(
        segmentsRef.current,
        cursorRef.current
      );
      const seg = segmentsRef.current[segIdx];
      if (seg?.type === 'text') {
        const newSegs = normalizeSegments([
          ...segmentsRef.current.slice(0, segIdx),
          { type: 'text', value: seg.value.slice(0, offset) },
          imageSegment,
          { type: 'text', value: seg.value.slice(offset) },
          ...segmentsRef.current.slice(segIdx + 1),
        ]);
        setSegments(newSegs);
        // Position cursor after the chip
        let newCursor = 0;
        for (const s of newSegs) {
          newCursor += segmentWidth(s);
          if (
            s === imageSegment ||
            (s.type === 'image' && s.base64 === data.data)
          ) {
            break;
          }
        }
        setCursor(newCursor);
      }
      // Also add to store so sendMessage includes it
      addPendingImage({
        base64: data.data,
        mimeType: data.mimeType,
        width: data.width,
        height: data.height,
        sizeBytes: data.sizeBytes,
      });
      return true;
    } catch (e) {
      logger.error('[PromptInput] handlePasteImage error:', e);
      return false;
    }
  };

  const handleBackspace = () => {
    pushUndo();
    inputMetrics.markStateUpdate();
    const cur = cursorRef.current;
    const segs = segmentsRef.current;
    if (cur === 0) return;

    const { segIdx, offset } = locateCursor(segs, cur);
    const seg = segs[segIdx];

    if (seg?.type === 'text' && offset > 0) {
      // Delete char in text
      const newValue = seg.value.slice(0, offset - 1) + seg.value.slice(offset);
      const newSegs = [...segs];
      newSegs[segIdx] = { type: 'text', value: newValue };
      setSegments(normalizeSegments(newSegs));
      setCursor(cur - 1);
      syncToStore(newSegs);
    } else if (offset === 0 && segIdx > 0) {
      // At start of segment - delete previous segment/char
      const prevSeg = segs[segIdx - 1];
      if (prevSeg?.type === 'text') {
        // Delete last char of previous text
        const newSegs = [...segs];
        newSegs[segIdx - 1] = {
          type: 'text',
          value: prevSeg.value.slice(0, -1),
        };
        setSegments(normalizeSegments(newSegs));
        setCursor(cur - 1);
        syncToStore(newSegs);
      } else if (prevSeg) {
        // Delete the chip
        const newSegs = [...segs.slice(0, segIdx - 1), ...segs.slice(segIdx)];
        setSegments(normalizeSegments(newSegs));
        setCursor(cur - 1);
        syncToStore(newSegs);
      }
    } else if (seg && seg.type !== 'text' && offset === 1) {
      // Cursor right after a chip - delete the chip
      const newSegs = [...segs.slice(0, segIdx), ...segs.slice(segIdx + 1)];
      setSegments(normalizeSegments(newSegs));
      setCursor(cur - 1);
      syncToStore(newSegs);
    }
  };

  const pushUndo = (force = false) => {
    const now = Date.now();
    if (!force && now - lastUndoPushTime.current < 500) return;
    lastUndoPushTime.current = now;
    undoStack.current.push({
      segments: segmentsRef.current,
      cursor: cursorRef.current,
    });
    if (undoStack.current.length > 100) undoStack.current.shift();
  };

  // Helper to apply an edit result from utility functions
  const applyEdit = (result: { segments: Segment[]; cursor: number }) => {
    pushUndo(true);
    inputMetrics.markStateUpdate();
    setSegments(result.segments);
    setCursor(result.cursor);
    syncToStore(result.segments);
  };

  /** Accept reverse search result into the input buffer. */
  const acceptReverseSearch = () => {
    const rs = reverseSearchRef.current;
    if (rs.match) {
      CommandHistory.getInstance().setIndex(rs.match.historyIndex);
    }
    const result = exitSearch(rs, 'matchPos');
    setReverseSearchActive(false);
    setStoreReverseSearchActive(false);
    const newSegs: Segment[] = [{ type: 'text', value: result.text }];
    setSegments(newSegs);
    setCursor(result.cursor);
    syncToStore(newSegs);
  };

  useKeypress(
    (userInput: string, key: Key) => {
      // Read latest state from refs to avoid stale closures when keypresses
      // arrive faster than React can re-render.
      let segments = segmentsRef.current;
      let cursor = cursorRef.current;

      // Shared ↑/↓ queue-restore application. `up` passes loadSegments=false
      // once it walks past the oldest entry so the same keypress falls through
      // to CommandHistory below.
      const applyQueueNav = (
        result: Extract<ReturnType<typeof navigateQueueUp>, { kind: 'queue' }>,
        loadSegments: boolean
      ) => {
        if (result.replace) {
          applyQueueReplace(result.replace);
        }
        queueRestoreRef.current = result.state;
        setEditingEntry(result.state);
        if (!loadSegments) return;
        suppressNextTriggerRef.current = true;
        setPromptHint(null);
        const newSegs: Segment[] = [{ type: 'text', value: result.loadText }];
        setSegments(newSegs);
        setCursor(result.loadText.length);
        syncToStore(newSegs);
        inputMetrics.markStateUpdate();
      };

      // Don't process input during shell escape — AppContainer forwards it to the PTY
      if (shellEscapeActive) return;

      // Don't process input when selection menu is open (Menu handles its own input)
      if (activeCommand) return;

      // Don't process input while a useInput-owning panel is open (repo/session
      // picker, source-provider gate) — it handles its own keystrokes.
      if (inputPanelOpen) return;

      // Toggle interrupt behavior (Ctrl+S by default) — works in all states
      if (keybindings.matches('toggleInterruptMode', userInput, key)) {
        toggleInterruptMode();
        return;
      }

      // Clear expand hint on any keypress
      if (expandHintActive.current) {
        setPromptHint(null);
        expandHintActive.current = false;
      }

      // --- Reverse incremental search (Ctrl+R) key handling ---
      if (reverseSearchRef.current.active) {
        const history = CommandHistory.getInstance().getAll();
        const rs = reverseSearchRef.current;

        if (key.escape) {
          acceptReverseSearch();
          return;
        }
        if (key.ctrl && userInput === 'c') {
          // Ctrl+C: abort search and clear input (readline convention)
          abortSearch(rs);
          setReverseSearchActive(false);
          setStoreReverseSearchActive(false);
          const newSegs: Segment[] = [{ type: 'text', value: '' }];
          setSegments(newSegs);
          setCursor(0);
          syncToStore(newSegs);
          return;
        }
        if (key.ctrl && userInput === 'r') {
          cycleOlder(rs, history);
          setReverseSearchActive((v) => !v); // force re-render
          return;
        }
        if (key.backspace || (key.ctrl && userInput === 'h')) {
          backspaceQuery(rs, history);
          setReverseSearchActive((v) => !v);
          return;
        }
        if (userInput && isPrintable(userInput) && !key.ctrl && !key.meta) {
          for (const ch of userInput) {
            appendQuery(rs, ch, history);
          }
          setReverseSearchActive((v) => !v);
          return;
        }
        // Any other key: accept matched result and fall through to normal handling
        acceptReverseSearch();
        // Re-read from refs since acceptReverseSearch updated them
        segments = segmentsRef.current;
        cursor = cursorRef.current;
      }

      if (key.paste) {
        handlePaste(userInput);
        return;
      }

      // Exit queue-restore mode and reset to an empty compose buffer.
      const clearQueueRestoreInput = () => {
        queueRestoreRef.current = null;
        setEditingEntry(null);
        const newSegs: Segment[] = [{ type: 'text', value: '' }];
        setSegments(newSegs);
        setCursor(0);
        syncToStore(newSegs);
      };

      // Esc in queue restore mode: abandon the edit, exit restore. The slot
      // is untouched so the original message stays in line. Not in restore?
      // fall through to the LiteLayout handler (cancel/etc).
      if (key.escape && queueRestoreRef.current) {
        clearQueueRestoreInput();
        return;
      }

      // Ctrl+X while editing a queued slot: drop the queued message entirely.
      // Gated to restore mode so it doesn't shadow Ctrl+X in the compose
      // buffer (reserved, no behavior today).
      if (key.ctrl && userInput === 'x' && queueRestoreRef.current) {
        const restore = queueRestoreRef.current;
        if (restore.kind === 'queue') {
          removeQueuedMessage(restore.queueIndex!);
        } else {
          clearSteerMessage(restore.originalText);
        }
        clearQueueRestoreInput();
        return;
      }

      // Clear path completion candidates on any key except Tab
      if (!key.tab) {
        setPathCandidates([]);
      }

      // Save and clear last yank tracking; Ctrl+Y and Alt+Y will re-set it.
      const prevYank = lastYankRef.current;
      lastYankRef.current = null;

      // Mirror CommandMenu's visibility check so PromptInput defers nav keys
      // (Tab/Up/Down/Enter) to the menu while it's mounted — including during
      // streaming (NOT gated on !isProcessing), where the menu stays up so the
      // user can autocomplete a slash command into the queue. The shared
      // isCommandVisibleInUiMode keeps the liteOnly filter in sync: without it,
      // typing a liteOnly name in TUI swallows Enter into an empty menu.
      const hasMatchingSlashCommands =
        activeTrigger?.key === '/' && !commandInputValue.includes(' ')
          ? slashCommands.some(
              (cmd) =>
                !cmd.meta?.hidden &&
                isCommandVisibleInUiMode(cmd, isLiteMode ? 'lite' : 'tui') &&
                cmd.name
                  .slice(1)
                  .toLowerCase()
                  .startsWith(commandInputValue.slice(1).toLowerCase())
            )
          : false;
      const slashMenuVisible = hasMatchingSlashCommands;
      // Check if file picker menu is visible
      const filePickerVisible =
        activeTrigger?.key === '@' && filePickerHasResults;
      // Prompt-item visibility is derived synchronously from the same inputs
      // the menu renders from, so this gate and the menu cannot disagree the
      // way the async file-search flag can.
      const atMenuPromptsVisible = atMenuShowsPrompts(
        slashCommands,
        commandInputValue,
        activeTrigger
      );
      // A visible trigger menu (slash, file, prompts) owns the next keypress
      // the same way it owns Enter/Tab below.
      const triggerMenuVisible =
        slashMenuVisible || filePickerVisible || atMenuPromptsVisible;

      // Esc during `/spec new` description collection: abandon the step. Lives
      // here because the placeholder advertising it belongs to this input —
      // when an overlay hides the input (or a menu is up), Esc is theirs; only
      // a bare Esc cancels.
      if (
        key.escape &&
        specDescriptionPending &&
        !isProcessing &&
        !triggerMenuVisible
      ) {
        cancelSpecDescription();
        return;
      }

      if (key.return) {
        if (key.meta || key.shift) {
          // Alt+Enter or Shift+Enter - insert newline
          insertText('\n');
        } else if (voiceStop) {
          // Stop active /voice recording
          voiceStop();
        } else {
          // Queue restore mode: Enter commits the edit back into its original
          // slot (preserving order), falling back to a fresh submit if the
          // slot drained. Empty edit deletes the slot ("discard this queued
          // message").
          const restore = queueRestoreRef.current;
          if (restore) {
            const content = buildContent(segments);
            queueRestoreRef.current = null;
            setEditingEntry(null);
            const result = commitQueueRestore(
              restore,
              content,
              unifiedEntriesRef.current
            );
            clearAll();
            switch (result.kind) {
              case 'replace-queue':
                replaceQueuedMessage(result.queueIndex, result.text);
                break;
              case 'delete-queue':
                removeQueuedMessage(result.queueIndex);
                break;
              case 'replace-steer':
                replaceSteerMessage(result.text, result.targetLine);
                break;
              case 'delete-steer':
                clearSteerMessage(result.targetLine);
                break;
              case 'fallback':
                if (result.text.trim()) {
                  // Fallback: slot drained or shifted — send as fresh message.
                  onSubmit(result.text);
                }
                break;
            }
            return;
          }
          // When processing, submit directly for queuing / slash-command
          // rejection. Footgun: the dropdown stays mounted during streaming and
          // twinki's broadcasting useInput fires the Menu's onSelect for this
          // SAME Enter, which already queues the completed command — so bail
          // when a menu is up or PromptInput double-queues the raw typed prefix.
          if (isProcessing) {
            if (slashMenuVisible || filePickerVisible || atMenuPromptsVisible)
              return;
            const content = buildContent(segments);
            // Don't submit whitespace-only prompts, but preserve indentation
            // (e.g. pasted code) in the submitted content.
            if (content.trim()) {
              clearAll();
              onSubmit(content);
            }
            return;
          }
          // Block Enter if file picker menu is visible with results
          if (filePickerVisible) return;
          // Block Enter if slash command menu is visible
          if (slashMenuVisible) return;
          // Block Enter while the @ menu shows prompt items — the menu's
          // own Enter handler selects the highlighted prompt.
          if (atMenuPromptsVisible) return;
          const content = buildContent(segments);
          const hasImages = segments.some((s) => s.type === 'image');
          if (content.trim() || hasImages) {
            clearAll();
            onSubmit(content);
          }
        }
      } else if (key.tab && !key.shift) {
        // Skip if a menu is handling tab
        if (slashMenuVisible || filePickerVisible || atMenuPromptsVisible)
          return;
        // Accept shadow text completion (e.g. /agent ro → /agent roberto)
        if (commandShadowText) {
          const text = getVisibleText(segments);
          const newText = text + commandShadowText;
          const newSegs: Segment[] = [{ type: 'text', value: newText }];
          setSegments(newSegs);
          setCursor(newText.length);
          syncToStore(newSegs);
          return;
        }
        // Tab on "/command " shows sub-command dropdown (e.g. /agent → create, edit, swap)
        // Skip if user already typed past a subcommand (e.g. /agent swap ro)
        {
          const text = getVisibleText(segments);
          if (text.startsWith('/') && text.includes(' ')) {
            const spaceIdx = text.indexOf(' ');
            const cmdName = text.slice(0, spaceIdx);
            const afterCmd = text.slice(spaceIdx + 1);
            const cmd = slashCommands.find((c) => c.name === cmdName);
            const subs = cmd?.meta?.subcommands;
            if (cmd && subs && subs.length > 0) {
              // Don't show subcommand menu if user already typed a subcommand + space
              const alreadyInSub = subs.some((s) =>
                afterCmd.startsWith(`${s} `)
              );
              // Don't show subcommand menu if the typed text doesn't match any
              // subcommand — user is typing free-form args (e.g. /goal fix the bug)
              const matchesSub =
                !afterCmd ||
                subs.some(
                  (s) => s.startsWith(afterCmd) || afterCmd.startsWith(s)
                );
              if (!alreadyInSub && matchesSub) {
                const subHints = cmd.meta?.subcommandHints ?? {};
                const subOptions = subs.map((sub) => ({
                  value: sub,
                  label: sub,
                  description: `${cmdName} ${sub}`,
                  hint: subHints[sub] ?? undefined,
                }));
                setActiveCommand({ command: cmd, options: subOptions });
                return;
              }
            }
          }
        }
        // Block file completion for selection commands awaiting argument shadow text
        // (shadow text may not have arrived yet due to debounce)
        {
          const text = getVisibleText(segments);
          if (text.startsWith('/') && text.includes(' ')) {
            const cmdName = text.slice(0, text.indexOf(' '));
            const cmd = slashCommands.find((c) => c.name === cmdName);
            if (cmd?.meta?.inputType === 'selection') return;
          }
        }
        // Expand paste chip when cursor is on one
        {
          const { segIdx } = locateCursor(segments, cursor);
          if (segments[segIdx]?.type === 'paste') {
            applyEdit(expandPasteSegment(segments, cursor));
            return;
          }
        }
        // Tab completion for filesystem paths
        const text = getVisibleText(segments);
        const result = completePathAtCursor(text, cursor);
        if (result) {
          if (result.replacement !== text.slice(result.start, cursor)) {
            // Replace the token in segments with the completed path
            const before = text.slice(0, result.start);
            const after = text.slice(cursor);
            const newText = before + result.replacement + after;
            const newCursor = result.start + result.replacement.length;
            const newSegs: Segment[] = [{ type: 'text', value: newText }];
            setSegments(newSegs);
            setCursor(newCursor);
            syncToStore(newSegs);
          } else if (result.candidates.length > 1) {
            // No progress possible — show candidates list
            setPathCandidates(result.candidates);
          }
        }
      } else if (key.backspace) {
        if (key.meta) {
          // Alt+Backspace - delete word backward
          applyEdit(deleteWordBackward(segments, cursor));
        } else {
          handleBackspace();
        }
      } else if (key.delete) {
        if (key.meta) {
          // Alt+Delete (fn+alt+delete on macOS) - delete word forward
          applyEdit(deleteWordForward(segments, cursor));
        } else {
          // fn+Delete on macOS - forward delete single char
          applyEdit(deleteForward(segments, cursor));
        }
      } else if (key.leftArrow) {
        // suppressArrows only blocks unmodified ↑/↓ — horizontal arrows
        // remain available so the user can still move the input cursor while
        // a parent handler claims arrows for navigation. Shift+← may still
        // be claimed by the parent (e.g. lite's subagent panel cycle binding)
        // — bail in that case so we don't fight over the keystroke.
        if (suppressArrows && key.shift) return;
        inputMetrics.markStateUpdate();
        if (key.ctrl || key.meta) {
          // Ctrl+Left or Cmd+Left - move word backward
          setCursor(moveWordBackward(segments, cursor));
        } else {
          setCursor(Math.max(0, cursor - 1));
        }
      } else if (key.rightArrow) {
        if (suppressArrows && key.shift) return;
        inputMetrics.markStateUpdate();
        // Accept shadow text when cursor is at end of input
        if (commandShadowText && cursor === totalWidth(segments)) {
          const text = getVisibleText(segments);
          const newText = text + commandShadowText;
          const newSegs: Segment[] = [{ type: 'text', value: newText }];
          setSegments(newSegs);
          setCursor(newText.length);
          syncToStore(newSegs);
          return;
        }
        if (key.ctrl || key.meta) {
          // Ctrl+Right or Cmd+Right - move word forward
          setCursor(moveWordForward(segments, cursor));
        } else {
          setCursor(Math.min(totalWidth(segments), cursor + 1));
        }
      } else if (key.upArrow) {
        if (suppressArrows) return;
        // shift+arrow is used by ActivityTray for queue navigation — don't handle here
        if (key.shift) return;
        // Skip if any menu is visible - let menu handle it
        if (
          slashMenuVisible ||
          filePickerVisible ||
          atMenuPromptsVisible ||
          activeCommand
        )
          return;
        // Multi-line or visually wrapped: move cursor up a visual line
        if (isVisuallyMultiLine(segments, termWidth)) {
          const newPos = moveCursorUpVisual(segments, cursor, termWidth);
          if (newPos !== null) {
            inputMetrics.markStateUpdate();
            setCursor(newPos);
            return;
          }
        }
        // Lite mode: ↑ walks back through queued messages first. Stepping past
        // the oldest entry (state === null) skips the load and falls through to
        // CommandHistory on the SAME keypress, so ↑↑↑ pages from queue tail into
        // history without a phantom no-op press in between.
        if (isLiteMode) {
          const result = navigateQueueUp(
            queueRestoreRef.current,
            getVisibleText(segments),
            unifiedEntriesRef.current
          );
          if (result.kind === 'queue') {
            applyQueueNav(result, result.state != null);
            if (result.state != null) return;
          }
        }
        // Single-line or already on first line: navigate history
        const currentText = buildContent(segments);
        const command = CommandHistory.getInstance().navigate(
          'up',
          currentText
        );
        if (command) {
          // Suppress trigger so slash commands from history don't open the menu
          suppressNextTriggerRef.current = true;
          setPromptHint(null);
          const newSegs: Segment[] = [{ type: 'text', value: command }];
          setSegments(newSegs);
          setCursor(command.length);
          syncToStore(newSegs);
        }
      } else if (key.downArrow) {
        if (suppressArrows) return;
        // shift+arrow is used by ActivityTray for queue navigation — don't handle here
        if (key.shift) return;
        // Skip if any menu is visible - let menu handle it
        if (
          slashMenuVisible ||
          filePickerVisible ||
          atMenuPromptsVisible ||
          activeCommand
        )
          return;
        // Multi-line or visually wrapped: move cursor down a visual line
        if (isVisuallyMultiLine(segments, termWidth)) {
          const newPos = moveCursorDownVisual(segments, cursor, termWidth);
          if (newPos !== null) {
            inputMetrics.markStateUpdate();
            setCursor(newPos);
            return;
          }
        }
        // Lite mode: ↓ walks forward through queue restore (mirror of ↑);
        // outside restore it falls through to history.
        if (isLiteMode && queueRestoreRef.current != null) {
          const result = navigateQueueDown(
            queueRestoreRef.current,
            getVisibleText(segments),
            unifiedEntriesRef.current
          );
          if (result.kind === 'queue') {
            applyQueueNav(result, true);
            return;
          }
        }
        // Single-line or already on last line: navigate history
        // Skip if user is just editing (not browsing history) to avoid clearing input
        if (!CommandHistory.getInstance().isNavigating()) return;
        const command = CommandHistory.getInstance().navigate('down');
        if (command !== null) {
          suppressNextTriggerRef.current = true;
          setPromptHint(null);
          const newSegs: Segment[] = [{ type: 'text', value: command }];
          setSegments(newSegs);
          setCursor(command.length);
          syncToStore(newSegs);
        }
      } else if (key.home) {
        inputMetrics.markStateUpdate();
        setCursor(0);
      } else if (key.end) {
        inputMetrics.markStateUpdate();
        setCursor(totalWidth(segments));
      } else if (key.ctrl) {
        // Emacs/readline shortcuts
        switch (userInput) {
          case 'a': // Ctrl+A - beginning of line
            inputMetrics.markStateUpdate();
            setCursor(moveToLogicalLineStart(segments, cursor));
            break;
          case 'e': // Ctrl+E - end of line
            inputMetrics.markStateUpdate();
            setCursor(moveToLogicalLineEnd(segments, cursor));
            break;
          case 'b': // Ctrl+B - back one char
            inputMetrics.markStateUpdate();
            setCursor(Math.max(0, cursor - 1));
            break;
          case 'f': // Ctrl+F - forward one char
            inputMetrics.markStateUpdate();
            setCursor(Math.min(totalWidth(segments), cursor + 1));
            break;
          case 'd': // Ctrl+D - delete char under cursor (forward delete)
            applyEdit(deleteForward(segments, cursor));
            break;
          case 'w': {
            // Ctrl+W - delete word backward
            const curBefore = cursor;
            applyEdit(deleteWordBackward(segments, cursor));
            const curAfter = cursorRef.current;
            const killed = getVisibleText(segments).slice(curAfter, curBefore);
            if (killed) killRingRef.current.push(killed, { prepend: true });
            break;
          }
          case 'k': {
            // Ctrl+K - kill to end of line
            const textBefore = getVisibleText(segments);
            applyEdit(killToLogicalLineEnd(segments, cursor));
            const textAfter = getVisibleText(segmentsRef.current);
            const killed = textBefore.slice(
              cursor,
              cursor + (textBefore.length - textAfter.length)
            );
            if (killed) killRingRef.current.push(killed, { prepend: false });
            break;
          }
          case 'u': {
            // Ctrl+U - kill to beginning of line
            const curBefore = cursor;
            applyEdit(killToLogicalLineBeginning(segments, cursor));
            const curAfter = cursorRef.current;
            const killed = getVisibleText(segments).slice(curAfter, curBefore);
            if (killed) killRingRef.current.push(killed, { prepend: true });
            break;
          }
          case 't': // Ctrl+T - transpose characters
            applyEdit(transposeChars(segments, cursor));
            break;
          case '_': // Ctrl+_ / Ctrl+/ — undo
            {
              const prev = undoStack.current.pop();
              if (prev) {
                inputMetrics.markStateUpdate();
                setSegments(prev.segments);
                setCursor(prev.cursor);
                syncToStore(prev.segments);
                // Re-show expand hint if undo restored a paste chip under cursor
                const { segIdx: undoSegIdx } = locateCursor(
                  prev.segments,
                  prev.cursor
                );
                if (prev.segments[undoSegIdx]?.type === 'paste') {
                  setPromptHint(EXPAND_HINT);
                  expandHintActive.current = true;
                }
              }
            }
            break;
          case 'j': // Ctrl+J - newline (existing)
            insertText('\n');
            break;
          case 'p': // Ctrl+P - move cursor up / previous history
            {
              if (slashMenuVisible || filePickerVisible) break;
              if (isVisuallyMultiLine(segments, termWidth)) {
                const newPos = moveCursorUpVisual(segments, cursor, termWidth);
                if (newPos !== null) {
                  inputMetrics.markStateUpdate();
                  setCursor(newPos);
                  break;
                }
              }
              const command = CommandHistory.getInstance().navigate(
                'up',
                buildContent(segments)
              );
              if (command) {
                setPromptHint(null);
                setSegments([{ type: 'text', value: command }]);
                setCursor(command.length);
              }
            }
            break;
          case 'n': // Ctrl+N - move cursor down / next history
            {
              if (slashMenuVisible || filePickerVisible) break;
              if (isVisuallyMultiLine(segments, termWidth)) {
                const newPos = moveCursorDownVisual(
                  segments,
                  cursor,
                  termWidth
                );
                if (newPos !== null) {
                  inputMetrics.markStateUpdate();
                  setCursor(newPos);
                  break;
                }
              }
              // Skip if user is just editing (not browsing history) to avoid clearing input
              if (!CommandHistory.getInstance().isNavigating()) break;
              const command = CommandHistory.getInstance().navigate('down');
              if (command) {
                setPromptHint(null);
                setSegments([{ type: 'text', value: command }]);
                setCursor(command.length);
              } else {
                setPromptHint(null);
                setSegments([{ type: 'text', value: '' }]);
                setCursor(0);
              }
            }
            break;
          case 'l': // Ctrl+L - clear screen
            process.stdout.write('\x1b[2J\x1b[H');
            break;
          case 'v': // Ctrl+V - paste image from clipboard
            handlePasteImage();
            break;
          case 'y': {
            // Ctrl+Y - yank (paste from kill ring)
            const yanked = killRingRef.current.peek();
            if (yanked) {
              const yankStart = cursor;
              insertText(yanked);
              lastYankRef.current = { start: yankStart, length: yanked.length };
            }
            break;
          }
          case 'h': // Ctrl+H - backspace alias
            handleBackspace();
            break;
          case 'r': // Ctrl+R - reverse incremental search
            {
              const currentText = getVisibleText(segments);
              enterSearch(reverseSearchRef.current, currentText, cursor);
              setReverseSearchActive(true);
              setStoreReverseSearchActive(true);
            }
            break;
          case 'o': // Ctrl+O - voice input (disabled)
            break;
          default:
            break;
        }
      } else if (key.meta) {
        // Alt/Meta shortcuts (word movement and deletion)
        switch (userInput) {
          case 'b': // Alt+B - back one word
            inputMetrics.markStateUpdate();
            setCursor(moveWordBackward(segments, cursor));
            break;
          case 'f': // Alt+F - forward one word
            inputMetrics.markStateUpdate();
            setCursor(moveWordForward(segments, cursor));
            break;
          case 'd': {
            // Alt+D - delete word forward (kill command)
            const textBefore = getVisibleText(segments);
            applyEdit(deleteWordForward(segments, cursor));
            const textAfter = getVisibleText(segmentsRef.current);
            const killed = textBefore.slice(
              cursor,
              cursor + (textBefore.length - textAfter.length)
            );
            if (killed) killRingRef.current.push(killed, { prepend: false });
            break;
          }
          case 't': // Alt+T - transpose words
            applyEdit(transposeWords(segments, cursor));
            break;
          case 'u': // Alt+U - uppercase word
            applyEdit(uppercaseWord(segments, cursor));
            break;
          case 'l': // Alt+L - lowercase word
            applyEdit(lowercaseWord(segments, cursor));
            break;
          case 'c': // Alt+C - capitalize word
            applyEdit(capitalizeWord(segments, cursor));
            break;
          case 'y': {
            // Alt+Y - yank-pop (cycle kill ring, replace last yank)
            if (prevYank && killRingRef.current.length > 1) {
              const { start, length } = prevYank;
              // Delete the previously yanked text
              const text = getVisibleText(segments);
              const newText = text.slice(0, start) + text.slice(start + length);
              // Rotate and insert next entry
              killRingRef.current.rotate();
              const next = killRingRef.current.peek();
              if (next) {
                const final =
                  newText.slice(0, start) + next + newText.slice(start);
                setSegments([{ type: 'text', value: final }]);
                setCursor(start + next.length);
                lastYankRef.current = { start, length: next.length };
              }
            }
            break;
          }
          default:
            break;
        }
      } else if (
        userInput === ' ' &&
        !key.ctrl &&
        !key.meta &&
        totalWidth(segments) === 0 &&
        slashCommands.some((c) => c.name === '/voice' && !c.meta?.type)
      ) {
        // Space hold-to-record: only intercept when voice mode is available
        const now = Date.now();

        // Clear existing release timer -- key is still being held
        if (spaceReleaseTimerRef.current) {
          clearTimeout(spaceReleaseTimerRef.current);
          spaceReleaseTimerRef.current = null;
        }

        if (spaceHoldStartRef.current === null) {
          // First space event -- start tracking
          spaceHoldStartRef.current = now;

          // Activate recording after SPACE_HOLD_MS
          spaceActivateTimerRef.current = setTimeout(() => {
            if (!pttActiveRef.current) {
              pttActiveRef.current = true;
              const remoteUrl = process.env.KIRO_VOICE_SERVER_URL ?? undefined;
              const spaceSession = startPTTRecording(remoteUrl, {
                onLevel: (level) => {
                  setVoiceLevel(level);
                },
                onPartial: (text) => {
                  setVoicePartialText(text);
                },
                onStatus: (status) => {
                  if (status === 'recording') {
                    setVoiceLevel(0);
                  } else if (status === 'downloading') {
                    showTransientAlert({
                      message: 'Downloading voice model...',
                      status: 'info',
                      autoHideMs: 120000,
                    });
                  } else if (status === 'download_complete') {
                    pttActiveRef.current = false;
                    pttSessionRef.current = null;
                    setVoiceCancel(null);
                    setVoiceLevel(null);
                    showTransientAlert({
                      message:
                        'Voice model ready! Hold Space or type /voice to start recording.',
                      status: 'success',
                      autoHideMs: 8000,
                    });
                  }
                },
              });
              pttSessionRef.current = spaceSession;
              setVoiceCancel(() => {
                voiceCancelledRef.current = true;
                spaceSession.cancel();
                pttActiveRef.current = false;
                pttSessionRef.current = null;
                setVoiceStop(null);
                setVoiceCancel(null);
                setVoiceLevel(null);
                setVoicePartialText(null);
              });
            }
          }, SPACE_HOLD_MS);
        }

        // Set release detection timer
        spaceReleaseTimerRef.current = setTimeout(() => {
          // Space key released
          if (spaceActivateTimerRef.current) {
            clearTimeout(spaceActivateTimerRef.current);
            spaceActivateTimerRef.current = null;
          }
          spaceHoldStartRef.current = null;
          spaceReleaseTimerRef.current = null;

          if (pttActiveRef.current && pttSessionRef.current) {
            // Stop recording
            pttActiveRef.current = false;
            const session = pttSessionRef.current;
            pttSessionRef.current = null;
            setVoiceCancel(null);
            session.stop();
            session.text
              .then((text) => {
                if (voiceCancelledRef.current) {
                  voiceCancelledRef.current = false;
                  setVoiceStop(null);
                  setVoiceLevel(null);
                  setVoicePartialText(null);
                  return;
                }
                setVoiceStop(null);
                setVoiceLevel(null);
                setVoicePartialText(null);
                incrementVoiceHint();
                if (text) {
                  if (voiceAutoSubmit) {
                    sendMessage(text);
                  } else {
                    insertVoiceText(text);
                  }
                } else {
                  showTransientAlert({
                    message: 'No speech detected',
                    status: 'error',
                    autoHideMs: 2000,
                  });
                }
              })
              .catch(() => {
                setVoiceStop(null);
                setVoiceLevel(null);
              });
          } else {
            // Hold was released before activation -- insert the space
            insertText(' ');
          }
        }, SPACE_RELEASE_TIMEOUT_MS);
      } else if (userInput && isPrintable(userInput)) {
        insertText(normalizeLineEndings(userInput));
      }
    },
    // Detach the prompt's key listener entirely while a useInput-owning panel
    // is open, so its type-to-search owns the keyboard (no double echo).
    { onEmptyPaste: handlePasteImage, isActive: !inputPanelOpen }
  );

  const renderContent = () => {
    // When a menu (slash commands, file picker) is open, it emits its own
    // CURSOR_MARKER for screen-reader accessibility. Suppress the marker here
    // to avoid two hardware cursors in multiplexers (tmux/zellij). The visual
    // inverse block still renders so the user sees where their input cursor is.
    const menuHasCursor =
      (activeTrigger?.key === '/' &&
        !commandInputValue.includes(' ') &&
        slashCommands.some(
          (cmd) =>
            !cmd.meta?.hidden &&
            cmd.name
              .slice(1)
              .toLowerCase()
              .startsWith(commandInputValue.slice(1).toLowerCase())
        )) ||
      (activeTrigger?.key === '@' &&
        (filePickerHasResults ||
          atMenuShowsPrompts(slashCommands, commandInputValue, activeTrigger)));

    // Reverse search mode: show the search prompt
    if (reverseSearchRef.current.active) {
      const rs = reverseSearchRef.current;
      const matchLine = rs.match?.line ?? '';
      const matchPos = rs.match?.matchPosition ?? 0;
      const prefix = `(reverse-i-search)\`${rs.query}': `;

      if (matchLine) {
        // Show cursor at the match position within the matched line
        const beforeMatch = matchLine.slice(0, matchPos);
        const charAtCursor = matchLine[matchPos] ?? ' ';
        const afterCursor = matchLine.slice(matchPos + 1);
        return (
          <Text wrap="wrap">
            <Text>{placeholderColor(prefix)}</Text>
            <Text>{primaryColor(beforeMatch)}</Text>
            <CursorBlock char={charAtCursor} suppressMarker={menuHasCursor} />
            {afterCursor && <Text>{primaryColor(afterCursor)}</Text>}
          </Text>
        );
      }
      return (
        <Text wrap="wrap">
          <Text>{placeholderColor(prefix)}</Text>
          <CursorBlock suppressMarker={menuHasCursor} />
        </Text>
      );
    }

    // When selection menu is open, show the command name statically (no cursor)
    if (activeCommand) {
      const cmdName = activeCommand.command.name;
      return <Text>{styleInputText(cmdName, true)}</Text>;
    }

    const total = totalWidth(segments);
    if (total === 0) {
      const recordingPlaceholder = voiceHint
        ? `Recording... ENTER to stop ${glyphs.smallDot} ${voiceHint}`
        : 'Recording... ENTER to stop';
      const activePlaceholder =
        voiceCursorChar != null ? recordingPlaceholder : resolvedPlaceholder;
      return (
        <>
          {voiceCursorChar != null && voicePartialText ? (
            <Text wrap="wrap">
              {voicePartialText}
              {chalk.green(voiceCursorChar)}
            </Text>
          ) : voiceCursorChar != null ? (
            <>
              <Text>{chalk.green(voiceCursorChar)}</Text>
              <Text>{placeholderColor(activePlaceholder)}</Text>
            </>
          ) : (
            <>
              <CursorBlock suppressMarker={menuHasCursor} />
              <Text>{placeholderColor(activePlaceholder)}</Text>
            </>
          )}
        </>
      );
    }

    // Build flat array of <Text> children. Twinki's text rendering flattens
    // nested <Text> nodes into one ANSI string, then wrap-ansi
    // wraps it at the container width using string-width (Unicode-correct).
    const parts: React.ReactNode[] = [];
    let pos = 0;

    for (let i = 0; i < segments.length; i++) {
      const seg = segments[i]!;
      const w = segmentWidth(seg);
      const cursorInSeg = cursor >= pos && cursor <= pos + w;

      if (seg.type === 'text') {
        if (cursorInSeg) {
          const localCursor = cursor - pos;
          // Handle the case that cursor is on the newline at the end of a line:
          // render a visible space for the cursor block, and keep the \n in `after`
          // so the line break still renders.
          const onNewline = seg.value[localCursor] === '\n';
          // When shadow text exists and cursor is at end of input, show
          // the first shadow char inside the cursor block so it looks like
          // one continuous word (matching v1 rustyline hinter behavior).
          const hasShadowAtEnd =
            commandShadowText && cursor === total && !onNewline;
          const charAtCursor = onNewline
            ? ' '
            : (seg.value[localCursor] ??
              (hasShadowAtEnd ? commandShadowText[0] : ' '));
          const afterStart = onNewline ? localCursor : localCursor + 1;
          const after =
            afterStart < seg.value.length ? seg.value.slice(afterStart) : '';
          const shadowRemainder = hasShadowAtEnd
            ? commandShadowText.slice(1)
            : null;
          parts.push(
            <React.Fragment key={i}>
              <Text>
                {styleInputText(seg.value.slice(0, localCursor), i === 0)}
              </Text>
              {voiceCursorChar != null ? (
                <Text>{chalk.green(voiceCursorChar)}</Text>
              ) : (
                <CursorBlock
                  char={charAtCursor}
                  suppressMarker={menuHasCursor}
                />
              )}
              {shadowRemainder && (
                <Text>{placeholderColor(shadowRemainder)}</Text>
              )}
              {after && <Text>{primaryColor(after)}</Text>}
            </React.Fragment>
          );
        } else {
          parts.push(<Text key={i}>{styleInputText(seg.value, i === 0)}</Text>);
        }
      } else if (seg.type === 'file') {
        if (cursorInSeg && cursor === pos) {
          parts.push(
            // Text color handled by FileChip component (uses theme colors internally)
            <React.Fragment key={i}>
              {voiceCursorChar != null ? (
                <Text>{chalk.green(voiceCursorChar)}</Text>
              ) : (
                <CursorBlock suppressMarker={menuHasCursor} />
              )}
              <FileChip filePath={seg.filePath} lineCount={seg.lineCount} />
            </React.Fragment>
          );
        } else {
          parts.push(
            <FileChip
              key={i}
              filePath={seg.filePath}
              lineCount={seg.lineCount}
            />
          );
        }
      } else if (seg.type === 'paste') {
        if (cursorInSeg && cursor === pos) {
          parts.push(
            // Text color handled by PastedChip component (uses theme colors internally)
            <React.Fragment key={i}>
              {voiceCursorChar != null ? (
                <Text>{chalk.green(voiceCursorChar)}</Text>
              ) : (
                <CursorBlock suppressMarker={menuHasCursor} />
              )}
              <PastedChip lineCount={seg.lineCount} charCount={seg.charCount} />
            </React.Fragment>
          );
        } else {
          parts.push(
            <PastedChip
              key={i}
              lineCount={seg.lineCount}
              charCount={seg.charCount}
            />
          );
        }
      } else if (seg.type === 'image') {
        if (cursorInSeg && cursor === pos) {
          parts.push(
            <React.Fragment key={i}>
              {voiceCursorChar != null ? (
                <Text>{chalk.green(voiceCursorChar)}</Text>
              ) : (
                <CursorBlock suppressMarker={menuHasCursor} />
              )}
              <PastedChip
                type="image"
                imageWidth={seg.width}
                imageHeight={seg.height}
                imageSizeBytes={seg.sizeBytes}
              />
            </React.Fragment>
          );
        } else {
          parts.push(
            <PastedChip
              key={i}
              type="image"
              imageWidth={seg.width}
              imageHeight={seg.height}
              imageSizeBytes={seg.sizeBytes}
            />
          );
        }
      }
      pos += w;
    }

    // Trailing cursor after a chip at the end
    if (cursor === total) {
      const lastSeg = segments[segments.length - 1];
      if (lastSeg && lastSeg.type !== 'text') {
        parts.push(
          voiceCursorChar != null ? (
            <Text key="cursor-end">{chalk.green(voiceCursorChar)}</Text>
          ) : (
            <React.Fragment key="cursor-end">
              <CursorBlock suppressMarker={menuHasCursor} />
            </React.Fragment>
          )
        );
      }
    }

    return <Text wrap="wrap">{parts}</Text>;
  };

  const candidateRows = useMemo(() => {
    if (pathCandidates.length === 0) return null;
    const sorted = [...pathCandidates].sort();
    const maxLen = Math.max(...sorted.map((c) => c.length));
    const colWidth = maxLen + 2;
    const cols = Math.max(1, Math.floor(termWidth / colWidth));
    const rows: string[][] = [];
    for (let i = 0; i < sorted.length; i += cols) {
      rows.push(sorted.slice(i, i + cols));
    }
    return { rows, colWidth };
  }, [pathCandidates, termWidth]);

  return (
    <Box flexDirection="column">
      <Box>{renderContent()}</Box>
      {candidateRows && (
        <Box flexDirection="column">
          {candidateRows.rows.map((row, ri) => (
            <Text key={ri} wrap="truncate">
              {row.map((c) => c.padEnd(candidateRows.colWidth)).join('')}
            </Text>
          ))}
        </Box>
      )}
    </Box>
  );
});
