import { describe, test, expect, beforeEach, afterEach, mock } from 'bun:test';

import {
  buildSurveyPayload,
  submitFormToAperture,
  type SurveyMetadata,
} from './survey-submit.js';
import { SESSION_FEEDBACK_SURVEY, validateEmail } from '../constants/survey.js';

describe('validateEmail', () => {
  test('accepts typical addresses', () => {
    expect(validateEmail('user@example.com')).toBeNull();
    expect(validateEmail('first.last+tag@sub.example.co.uk')).toBeNull();
    expect(validateEmail('  user@example.com  ')).toBeNull();
  });

  test('rejects obvious typos', () => {
    expect(validateEmail('no-at-sign')).not.toBeNull();
    expect(validateEmail('missing-domain@')).not.toBeNull();
    expect(validateEmail('@example.com')).not.toBeNull();
    expect(validateEmail('no-dot@example')).not.toBeNull();
    expect(validateEmail('white space@example.com')).not.toBeNull();
  });
});

describe('buildSurveyPayload: SessionFeedback', () => {
  test('produces the correct Aperture form coordinates', () => {
    const payload = buildSurveyPayload(SESSION_FEEDBACK_SURVEY, {
      experience: 'Good',
    });
    expect(payload.category).toBe('KiroCLI');
    expect(payload.name).toBe('SessionFeedback');
    expect(payload.version).toBe('1.0.0');
    expect(payload.locale).toBe('en_US');
  });

  test('always includes all 3 customerResponses', () => {
    const payload = buildSurveyPayload(SESSION_FEEDBACK_SURVEY, {
      experience: 'Good',
    });
    expect(payload.customerResponses).toHaveLength(3);
  });

  test('Q1: rating type with array responseValue, rowLabels, columnLabels', () => {
    const payload = buildSurveyPayload(SESSION_FEEDBACK_SURVEY, {
      experience: 'Excellent',
    });
    const r = payload.customerResponses[0]!;
    expect(r.question).toBe('Experience with Kiro CLI');
    expect(r.pii).toBe(false);
    expect(r.response.responseType).toBe('rating');
    expect(r.response.responseValue).toEqual(['5']);
    expect(r.response.rowLabels).toBeDefined();
    expect(r.response.columnLabels).toBeDefined();
  });

  test('Q2: textArea type', () => {
    const payload = buildSurveyPayload(SESSION_FEEDBACK_SURVEY, {
      experience: 'Good',
      feedback: 'Great CLI',
    });
    const r = payload.customerResponses[1]!;
    expect(r.response.responseType).toBe('textArea');
    expect(r.response.responseValue).toBe('Great CLI');
  });

  test('Q3: text type with pii and trailing space in question', () => {
    const payload = buildSurveyPayload(SESSION_FEEDBACK_SURVEY, {
      experience: 'Good',
      email: 'user@example.com',
    });
    const r = payload.customerResponses[2]!;
    expect(r.question).toMatch(/research panel\. $/);
    expect(r.pii).toBe(true);
    expect(r.response.responseType).toBe('text');
    expect(r.response.responseValue).toBe('user@example.com');
  });

  test('empty optional answers are sent as empty strings', () => {
    const payload = buildSurveyPayload(SESSION_FEEDBACK_SURVEY, {
      experience: 'Fair',
    });
    expect(payload.customerResponses[1]!.response.responseValue).toBe('');
    expect(payload.customerResponses[2]!.response.responseValue).toBe('');
  });

  test('includes metadata', () => {
    const payload = buildSurveyPayload(
      SESSION_FEEDBACK_SURVEY,
      { experience: 'Good' },
      { sessionId: 'sess-abc', isInternal: true }
    );
    const findMeta = (key: string) =>
      payload.metadataList!.find((m) => m.key === key)?.value;
    expect(findMeta('sessionId')).toBe('sess-abc');
    expect(findMeta('isInternal')).toBe('true');
    expect(findMeta('userId')).toMatch(/^[a-f0-9]{16}$/);
  });
});

describe('submitFormToAperture', () => {
  let originalFetch: typeof fetch;

  beforeEach(() => {
    originalFetch = globalThis.fetch;
  });

  afterEach(() => {
    globalThis.fetch = originalFetch;
  });

  test('returns ok: true on 2xx', async () => {
    globalThis.fetch = mock(
      async () => new Response('{"id":"test"}', { status: 200 })
    ) as unknown as typeof fetch;
    const outcome = await submitFormToAperture(SESSION_FEEDBACK_SURVEY, {
      experience: 'Good',
    });
    expect(outcome.ok).toBe(true);
    expect(outcome.rateLimited).toBe(false);
  });

  test('sends correct payload for any survey', async () => {
    let capturedBody: string | undefined;
    globalThis.fetch = mock(async (_url: unknown, init: unknown) => {
      capturedBody = (init as RequestInit).body as string;
      return new Response('{}', { status: 200 });
    }) as unknown as typeof fetch;

    await submitFormToAperture(SESSION_FEEDBACK_SURVEY, {
      experience: 'Excellent',
      feedback: 'Nice session',
    });

    const parsed = JSON.parse(capturedBody!);
    expect(parsed.name).toBe('SessionFeedback');
    expect(parsed.customerResponses[0].question).toBe(
      'Experience with Kiro CLI'
    );
    expect(parsed.customerResponses[0].response.responseValue).toEqual(['5']);
  });

  test('surfaces 429 as rateLimited', async () => {
    globalThis.fetch = mock(
      async () => new Response('', { status: 429 })
    ) as unknown as typeof fetch;
    const outcome = await submitFormToAperture(SESSION_FEEDBACK_SURVEY, {
      experience: 'Good',
    });
    expect(outcome.ok).toBe(false);
    expect(outcome.rateLimited).toBe(true);
  });

  test('catches unexpected throws', async () => {
    globalThis.fetch = mock(async () => {
      throw new TypeError('Failed to fetch');
    }) as unknown as typeof fetch;
    const outcome = await submitFormToAperture(SESSION_FEEDBACK_SURVEY, {
      experience: 'Good',
    });
    expect(outcome.ok).toBe(false);
  });

  // Capture the User-Agent header from the outgoing request.
  const captureUserAgent = async (
    metadata?: SurveyMetadata
  ): Promise<string | undefined> => {
    let captured: string | undefined;
    globalThis.fetch = mock(async (_url: unknown, init: unknown) => {
      const headers = (init as RequestInit).headers as
        | Record<string, string>
        | undefined;
      captured = headers?.['User-Agent'];
      return new Response('{}', { status: 200 });
    }) as unknown as typeof fetch;

    await submitFormToAperture(
      SESSION_FEEDBACK_SURVEY,
      { experience: 'Good' },
      metadata ? { metadata } : {}
    );
    return captured;
  };

  test('appends engine/v3 to User-Agent for KAS sessions', async () => {
    const ua = await captureUserAgent({ agentEngine: 'kas' });
    expect(ua).toMatch(/^kiro-cli\/\S+ engine\/v3$/);
  });

  test('appends engine/v2 to User-Agent for v2 sessions', async () => {
    const ua = await captureUserAgent({ agentEngine: 'v2' });
    expect(ua).toMatch(/^kiro-cli\/\S+ engine\/v2$/);
  });

  test('omits engine token when agentEngine is undefined', async () => {
    const ua = await captureUserAgent(undefined);
    expect(ua).toBeDefined();
    expect(ua).not.toContain('engine/');
    expect(ua).toMatch(/^kiro-cli\/\S+$/);
  });
});
