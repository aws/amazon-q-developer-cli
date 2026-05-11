/**
 * Research-survey eligibility + completion persistence.
 *
 * Stored at `~/.kiro/settings/survey_state.json` (or
 * `$KIRO_HOME/settings/survey_state.json`).
 *
 * Supports multiple surveys — each keyed by its `surveyId`. The file stores
 * a map of `{ [surveyId]: SurveyState }`.
 *
 * All reads/writes are best-effort and failures are swallowed — the survey
 * is opt-in UX and must never block the session.
 */

import { readFileSync, writeFileSync, mkdirSync, existsSync } from 'node:fs';
import { dirname } from 'node:path';
import { kiroHomePath } from './kiro-home.js';
import { logger } from './logger.js';
import type { SurveyDefinition } from '../constants/survey.js';

export interface SurveyState {
  /** Result of the one-time sampling roll. `null` until first computed. */
  eligible: boolean | null;
  /** The sample rate that was used for the eligibility roll. When the survey
   *  definition's sampleRate changes, a mismatch triggers a re-roll. */
  sampledAtRate?: number;
  /** ms-epoch — last time we popped the notification for this user. */
  lastShownAt: number | null;
  /** ms-epoch — last time the user completed the survey. */
  lastCompletedAt: number | null;
  /** How many times the user dismissed the prompt without answering. */
  dismissCount: number;
}

export type AllSurveyStates = Record<string, SurveyState>;

const DEFAULT_STATE: SurveyState = {
  eligible: null,
  lastShownAt: null,
  lastCompletedAt: null,
  dismissCount: 0,
};

/** After this many completed turns in the session, session feedback becomes eligible. */
export const DEFAULT_TURN_THRESHOLD = 3;

function statePath(): string {
  return kiroHomePath('settings', 'survey_state.json');
}

function getSampleRate(survey: SurveyDefinition): number {
  // Per-survey env override: KIRO_SURVEY_SAMPLE_RATE_<ID> (uppercased, dashes→underscores)
  const envKey = `KIRO_SURVEY_SAMPLE_RATE_${survey.id.toUpperCase().replace(/-/g, '_')}`;
  const perSurvey = process.env[envKey];
  if (perSurvey) {
    const n = Number(perSurvey);
    if (Number.isFinite(n) && n >= 0 && n <= 1) return n;
  }
  // Global override
  const global = process.env.KIRO_SURVEY_SAMPLE_RATE;
  if (global) {
    const n = Number(global);
    if (Number.isFinite(n) && n >= 0 && n <= 1) return n;
  }
  return survey.sampleRate;
}

function getCooldownDays(survey: SurveyDefinition): number {
  // Per-survey env override
  const envKey = `KIRO_SURVEY_COOLDOWN_DAYS_${survey.id.toUpperCase().replace(/-/g, '_')}`;
  const perSurvey = process.env[envKey];
  if (perSurvey) {
    const n = Number(perSurvey);
    if (Number.isFinite(n) && n >= 0) return n;
  }
  // Global override
  const global = process.env.KIRO_SURVEY_COOLDOWN_DAYS;
  if (global) {
    const n = Number(global);
    if (Number.isFinite(n) && n >= 0) return n;
  }
  return survey.cooldownDays;
}

// ─── Persistence ─────────────────────────────────────────────────────────────

export function loadAllSurveyStates(): AllSurveyStates {
  try {
    const p = statePath();
    if (!existsSync(p)) return {};
    const raw = JSON.parse(readFileSync(p, 'utf-8'));
    if (raw && typeof raw === 'object' && !Array.isArray(raw)) {
      return raw as AllSurveyStates;
    }
  } catch (err) {
    logger.warn('[survey-state] Failed to read survey state', err);
  }
  return {};
}

export function loadSurveyState(surveyId: string): SurveyState {
  const all = loadAllSurveyStates();
  return parseSurveyState(all[surveyId]);
}

function parseSurveyState(raw: unknown): SurveyState {
  if (!raw || typeof raw !== 'object') return { ...DEFAULT_STATE };
  const obj = raw as Record<string, unknown>;
  return {
    eligible: typeof obj.eligible === 'boolean' ? obj.eligible : null,
    sampledAtRate:
      typeof obj.sampledAtRate === 'number' ? obj.sampledAtRate : undefined,
    lastShownAt: typeof obj.lastShownAt === 'number' ? obj.lastShownAt : null,
    lastCompletedAt:
      typeof obj.lastCompletedAt === 'number' ? obj.lastCompletedAt : null,
    dismissCount: typeof obj.dismissCount === 'number' ? obj.dismissCount : 0,
  };
}

function saveAllStates(states: AllSurveyStates): void {
  try {
    const p = statePath();
    mkdirSync(dirname(p), { recursive: true });
    writeFileSync(p, JSON.stringify(states, null, 2), 'utf-8');
  } catch (err) {
    logger.warn('[survey-state] Failed to write survey state', err);
  }
}

export function saveSurveyState(surveyId: string, state: SurveyState): void {
  const all = loadAllSurveyStates();
  all[surveyId] = state;
  saveAllStates(all);
}

// ─── Eligibility ─────────────────────────────────────────────────────────────

/**
 * Lazily computes and persists the sampling roll for a given survey.
 */
export function resolveEligibility(
  survey: SurveyDefinition,
  state: SurveyState,
  rng: () => number = Math.random
): { eligible: boolean; state: SurveyState } {
  const currentRate = getSampleRate(survey);

  // Return cached result if the rate hasn't changed since the last roll.
  if (state.eligible !== null && state.sampledAtRate === currentRate) {
    return { eligible: state.eligible, state };
  }

  // First roll or rate changed — (re-)roll the dice.
  const eligible = rng() < currentRate;
  const next: SurveyState = { ...state, eligible, sampledAtRate: currentRate };
  saveSurveyState(survey.id, next);
  return { eligible, state: next };
}

/**
 * Returns true if enough time has elapsed since the last prompt.
 */
export function cooldownElapsed(
  survey: SurveyDefinition,
  state: SurveyState,
  now: number = Date.now()
): boolean {
  const mostRecent = Math.max(
    state.lastShownAt ?? 0,
    state.lastCompletedAt ?? 0
  );
  if (mostRecent === 0) return true;
  const days = getCooldownDays(survey);
  const ms = days * 24 * 60 * 60 * 1000;
  return now - mostRecent >= ms;
}

/**
 * Central policy check: should we show this survey?
 */
export function shouldShowSurvey(
  survey: SurveyDefinition,
  state: SurveyState,
  now: number = Date.now()
): boolean {
  if (state.eligible !== true) return false;
  return cooldownElapsed(survey, state, now);
}

// ─── Mutation helpers ────────────────────────────────────────────────────────

export function markSurveyShown(
  surveyId: string,
  now: number = Date.now()
): void {
  const state = loadSurveyState(surveyId);
  saveSurveyState(surveyId, { ...state, lastShownAt: now });
}

export function markSurveyCompleted(
  surveyId: string,
  now: number = Date.now()
): void {
  const state = loadSurveyState(surveyId);
  saveSurveyState(surveyId, {
    ...state,
    lastShownAt: now,
    lastCompletedAt: now,
  });
}

export function markSurveyDismissed(
  surveyId: string,
  now: number = Date.now()
): void {
  const state = loadSurveyState(surveyId);
  saveSurveyState(surveyId, {
    ...state,
    lastShownAt: now,
    dismissCount: state.dismissCount + 1,
  });
}

// ─── Test hooks ──────────────────────────────────────────────────────────────
export {
  statePath as _statePath,
  getSampleRate as _getSampleRate,
  getCooldownDays as _getCooldownDays,
};
