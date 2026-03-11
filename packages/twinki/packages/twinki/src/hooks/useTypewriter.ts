/**
 * useTypewriter — reveals text progressively at word boundaries.
 *
 * Advances word-by-word internally with natural timing jitter,
 * but batches React state updates to ~50ms intervals. This means
 * downstream components (e.g. <Markdown>) re-render at ~20fps
 * regardless of WPS, keeping the pipeline efficient.
 */
import { useState, useEffect, useRef, useCallback } from 'react';

export type TypewriterSpeed = 'instant' | 'fast' | 'natural' | 'typing';

export interface UseTypewriterOptions {
	/** Speed preset or words-per-second number. Default: 'natural' */
	speed?: TypewriterSpeed | number;
	/** Called when all text has been revealed */
	onComplete?: () => void;
}

const SPEED_WPS: Record<TypewriterSpeed, number> = {
	instant: Infinity,
	fast: 50,
	natural: 20,
	typing: 3,
};

const PAUSE_AFTER = /[.!?]\s*$/;
const SOFT_PAUSE = /[,;:]\s*$/;
const BATCH_MS = 50; // flush to React at most every 50ms

export function useTypewriter(
	text: string,
	options: UseTypewriterOptions = {},
): { visibleText: string; isComplete: boolean } {
	const { speed = 'natural', onComplete } = options;
	const wps = typeof speed === 'number' ? speed : SPEED_WPS[speed];

	const [wordIndex, setWordIndex] = useState(0);
	const wordsRef = useRef<string[]>([]);
	const prevTextRef = useRef('');
	const onCompleteRef = useRef(onComplete);
	onCompleteRef.current = onComplete;

	// Re-split when text changes (streaming appends)
	if (text !== prevTextRef.current) {
		wordsRef.current = text.split(/(?<=\s)/);
		prevTextRef.current = text;
	}

	const words = wordsRef.current;
	const isComplete = wordIndex >= words.length;

	useEffect(() => {
		if (wps === Infinity) {
			setWordIndex(words.length);
			return;
		}
		if (wordIndex >= words.length) {
			onCompleteRef.current?.();
			return;
		}

		let cursor = wordIndex;
		let cancelled = false;
		let flushTimer: ReturnType<typeof setTimeout> | null = null;
		let wordTimer: ReturnType<typeof setTimeout> | null = null;

		// Schedule periodic React flushes
		const scheduleFlush = () => {
			if (flushTimer !== null) return;
			flushTimer = setTimeout(() => {
				flushTimer = null;
				if (!cancelled) {
					setWordIndex(cursor);
				}
			}, BATCH_MS);
		};

		const advance = () => {
			if (cancelled || cursor >= words.length) {
				// Final flush
				if (!cancelled) setWordIndex(cursor);
				return;
			}

			const word = words[cursor] ?? '';
			const baseMs = 1000 / wps;
			let delay = baseMs;
			if (PAUSE_AFTER.test(word)) delay *= 2.5;
			else if (SOFT_PAUSE.test(word)) delay *= 1.5;
			delay *= 0.7 + Math.random() * 0.6;

			cursor++;
			scheduleFlush();

			wordTimer = setTimeout(advance, delay);
		};

		advance();

		return () => {
			cancelled = true;
			if (wordTimer !== null) clearTimeout(wordTimer);
			if (flushTimer !== null) clearTimeout(flushTimer);
		};
	}, [wordIndex >= words.length ? 'done' : 'running', wps, words.length]);

	const visibleText = words.slice(0, wordIndex).join('');
	return { visibleText, isComplete };
}
