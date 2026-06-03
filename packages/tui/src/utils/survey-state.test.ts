import { describe, test, expect, beforeEach, afterEach } from 'bun:test';
import { mkdtempSync, rmSync, existsSync, readFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';

import {
  loadSurveyState,
  saveSurveyState,
  resolveEligibility,
  cooldownElapsed,
  shouldShowSurvey,
  markSurveyShown,
  markSurveyCompleted,
  markSurveyDismissed,
  _statePath,
} from './survey-state.js';
import {
  SESSION_FEEDBACK_SURVEY,
  PLAN_QUALITY_SURVEY,
} from '../constants/survey.js';

describe('survey-state', () => {
  let tmpDir: string;
  let originalKiroHome: string | undefined;
  let originalSampleRate: string | undefined;
  let originalCooldown: string | undefined;

  beforeEach(() => {
    originalKiroHome = process.env.KIRO_HOME;
    originalSampleRate = process.env.KIRO_SURVEY_SAMPLE_RATE;
    originalCooldown = process.env.KIRO_SURVEY_COOLDOWN_DAYS;
    tmpDir = mkdtempSync(join(tmpdir(), 'kiro-survey-test-'));
    process.env.KIRO_HOME = tmpDir;
  });

  afterEach(() => {
    if (originalKiroHome === undefined) delete process.env.KIRO_HOME;
    else process.env.KIRO_HOME = originalKiroHome;
    if (originalSampleRate === undefined)
      delete process.env.KIRO_SURVEY_SAMPLE_RATE;
    else process.env.KIRO_SURVEY_SAMPLE_RATE = originalSampleRate;
    if (originalCooldown === undefined)
      delete process.env.KIRO_SURVEY_COOLDOWN_DAYS;
    else process.env.KIRO_SURVEY_COOLDOWN_DAYS = originalCooldown;
    rmSync(tmpDir, { recursive: true, force: true });
  });

  test('loadSurveyState returns defaults when no file exists', () => {
    const state = loadSurveyState('session-feedback');
    expect(state.eligible).toBeNull();
    expect(state.lastShownAt).toBeNull();
    expect(state.lastCompletedAt).toBeNull();
    expect(state.dismissCount).toBe(0);
  });

  test('saveSurveyState persists per-survey state', () => {
    saveSurveyState('session-feedback', {
      eligible: true,
      lastShownAt: 123,
      lastCompletedAt: 456,
      dismissCount: 2,
    });
    const path = _statePath();
    expect(existsSync(path)).toBe(true);
    const parsed = JSON.parse(readFileSync(path, 'utf-8'));
    expect(parsed['session-feedback'].eligible).toBe(true);
    expect(parsed['session-feedback'].dismissCount).toBe(2);
  });

  test('multiple surveys are stored independently', () => {
    saveSurveyState('session-feedback', {
      eligible: true,
      lastShownAt: 100,
      lastCompletedAt: null,
      dismissCount: 0,
    });
    saveSurveyState('plan-quality', {
      eligible: false,
      lastShownAt: 200,
      lastCompletedAt: null,
      dismissCount: 1,
    });
    expect(loadSurveyState('session-feedback').eligible).toBe(true);
    expect(loadSurveyState('plan-quality').eligible).toBe(false);
    expect(loadSurveyState('plan-quality').dismissCount).toBe(1);
  });

  test('resolveEligibility returns cached value once set', () => {
    const state = loadSurveyState('session-feedback');
    const first = resolveEligibility(
      SESSION_FEEDBACK_SURVEY,
      state,
      () => 0.01
    );
    expect(first.eligible).toBe(true);
    expect(first.state.sampledAtRate).toBe(1.0);
    // Second call with same rate returns cached value (no re-roll)
    const second = resolveEligibility(
      SESSION_FEEDBACK_SURVEY,
      loadSurveyState('session-feedback'),
      () => 0.99
    );
    expect(second.eligible).toBe(true);
  });

  test('resolveEligibility uses survey-specific sample rate', () => {
    // PLAN_QUALITY_SURVEY has 10% rate
    process.env.KIRO_SURVEY_SAMPLE_RATE = '0.1';
    const hit = resolveEligibility(
      PLAN_QUALITY_SURVEY,
      loadSurveyState('plan-quality'),
      () => 0.05
    );
    expect(hit.eligible).toBe(true);
  });

  test('resolveEligibility re-rolls when sample rate changes', () => {
    // First roll at 100% — user is eligible (rng returns 0.01)
    const first = resolveEligibility(
      SESSION_FEEDBACK_SURVEY,
      loadSurveyState('session-feedback'),
      () => 0.01
    );
    expect(first.eligible).toBe(true);
    expect(first.state.sampledAtRate).toBe(1.0);

    // Simulate rate change to 1% by using a custom survey with lower rate
    const lowRateSurvey = { ...SESSION_FEEDBACK_SURVEY, sampleRate: 0.01 };
    // rng returns 0.5 — above the new 1% threshold → should become ineligible
    const second = resolveEligibility(
      lowRateSurvey,
      loadSurveyState('session-feedback'),
      () => 0.5
    );
    expect(second.eligible).toBe(false);
    expect(second.state.sampledAtRate).toBe(0.01);
  });

  test('shouldShowSurvey is false when ineligible', () => {
    saveSurveyState('session-feedback', {
      eligible: false,
      lastShownAt: null,
      lastCompletedAt: null,
      dismissCount: 0,
    });
    expect(
      shouldShowSurvey(
        SESSION_FEEDBACK_SURVEY,
        loadSurveyState('session-feedback')
      )
    ).toBe(false);
  });

  test('shouldShowSurvey honors the cooldown', () => {
    const now = Date.now();
    const twoDaysAgo = now - 2 * 24 * 60 * 60 * 1000;
    saveSurveyState('session-feedback', {
      eligible: true,
      lastShownAt: twoDaysAgo,
      lastCompletedAt: null,
      dismissCount: 1,
    });
    // 30-day cooldown — 2 days ago is too recent
    expect(
      shouldShowSurvey(
        SESSION_FEEDBACK_SURVEY,
        loadSurveyState('session-feedback'),
        now
      )
    ).toBe(false);

    // 31 days ago — just past the 30-day boundary
    const oldShown = now - 31 * 24 * 60 * 60 * 1000;
    saveSurveyState('session-feedback', {
      eligible: true,
      lastShownAt: oldShown,
      lastCompletedAt: null,
      dismissCount: 1,
    });
    expect(
      shouldShowSurvey(
        SESSION_FEEDBACK_SURVEY,
        loadSurveyState('session-feedback'),
        now
      )
    ).toBe(true);
  });

  test('cooldownElapsed returns true on a fresh state', () => {
    const state = loadSurveyState('session-feedback');
    expect(cooldownElapsed(SESSION_FEEDBACK_SURVEY, state)).toBe(true);
  });

  test('cooldownElapsed respects global env override', () => {
    process.env.KIRO_SURVEY_COOLDOWN_DAYS = '1';
    const now = Date.now();
    saveSurveyState('session-feedback', {
      eligible: true,
      lastShownAt: now - 2 * 60 * 60 * 1000, // 2h ago
      lastCompletedAt: null,
      dismissCount: 0,
    });
    expect(
      cooldownElapsed(
        SESSION_FEEDBACK_SURVEY,
        loadSurveyState('session-feedback'),
        now
      )
    ).toBe(false);
  });

  test('markSurveyShown updates lastShownAt', () => {
    saveSurveyState('session-feedback', {
      eligible: true,
      lastShownAt: null,
      lastCompletedAt: 100,
      dismissCount: 0,
    });
    markSurveyShown('session-feedback', 999);
    const state = loadSurveyState('session-feedback');
    expect(state.lastShownAt).toBe(999);
    expect(state.lastCompletedAt).toBe(100);
  });

  test('markSurveyCompleted updates both timestamps', () => {
    saveSurveyState('plan-quality', {
      eligible: true,
      lastShownAt: null,
      lastCompletedAt: null,
      dismissCount: 0,
    });
    markSurveyCompleted('plan-quality', 42);
    const state = loadSurveyState('plan-quality');
    expect(state.lastShownAt).toBe(42);
    expect(state.lastCompletedAt).toBe(42);
  });

  test('markSurveyDismissed bumps counter', () => {
    saveSurveyState('session-feedback', {
      eligible: true,
      lastShownAt: null,
      lastCompletedAt: null,
      dismissCount: 2,
    });
    markSurveyDismissed('session-feedback', 77);
    const state = loadSurveyState('session-feedback');
    expect(state.dismissCount).toBe(3);
    expect(state.lastShownAt).toBe(77);
  });
});
