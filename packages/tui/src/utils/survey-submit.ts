/**
 * Generic Aperture Form submission for any survey definition.
 *
 * Builds the payload from a SurveyDefinition + answers map, then POSTs to
 * the Aperture ingestion endpoint. Fire-and-forget — never throws.
 */

import type { SurveyDefinition } from '../constants/survey.js';
import type { AgentEngine } from '../agent-engine.js';
import {
  submitForm,
  buildUserAgent,
  type ApertureSubmitResult,
} from './aperture-client.js';
import { logger } from './logger.js';
import { hostname } from 'node:os';
import { createHash } from 'node:crypto';

// ─── Aperture payload types ──────────────────────────────────────────────────

export interface ApertureCustomerResponse {
  question: string;
  pii: boolean;
  response: {
    responseType: string;
    responseValue: string | string[];
    rowLabels?: string[];
    columnLabels?: string[];
  };
}

export interface ApertureFormPayload {
  customerResponses: ApertureCustomerResponse[];
  category: string;
  version: string;
  name: string;
  locale: string;
  reference: string;
  location: string;
  metadataList?: Array<{ key: string; value: string }>;
}

// ─── Metadata ────────────────────────────────────────────────────────────────

export interface SurveyMetadata {
  userId?: string;
  sessionId?: string;
  taskId?: string;
  isInternal?: boolean;
  /**
   * Active agent engine (`v2` = Rust backend, `kas` = KAS backend). When set,
   * it is appended to the request `User-Agent` so survey responses can be
   * attributed to v2 vs KAS sessions.
   */
  agentEngine?: AgentEngine;
}

let cachedUserId: string | null = null;
export function getAnonymousUserId(): string {
  if (cachedUserId) return cachedUserId;
  try {
    const raw = hostname() + (process.env.USER ?? process.env.USERNAME ?? '');
    cachedUserId = createHash('sha256').update(raw).digest('hex').slice(0, 16);
  } catch {
    cachedUserId = 'unknown';
  }
  return cachedUserId;
}

// ─── Payload builder ─────────────────────────────────────────────────────────

/**
 * Build the Aperture Form payload from a survey definition + answers map.
 *
 * All questions are always included (Aperture requires the full set).
 * Empty optional answers are sent as empty strings.
 */
export function buildSurveyPayload(
  survey: SurveyDefinition,
  answers: Record<string, string>,
  meta?: SurveyMetadata
): ApertureFormPayload {
  const customerResponses: ApertureCustomerResponse[] = survey.questions.map(
    (q) => {
      const value = answers[q.id] ?? '';
      const response: ApertureCustomerResponse['response'] = {
        responseType: q.responseType,
        responseValue:
          q.responseType === 'rating'
            ? [String((q.options?.indexOf(value) ?? 0) + 1)]
            : value,
      };
      // Include rowLabels/columnLabels for rating types (required for Slack notifications)
      if (q.responseType === 'rating') {
        if (q.rowLabels) response.rowLabels = q.rowLabels;
        if (q.columnLabels) response.columnLabels = q.columnLabels;
      }
      return {
        question: q.apertureQuestionText,
        pii: q.pii,
        response,
      };
    }
  );

  const metadataList: Array<{ key: string; value: string }> = [
    { key: 'userId', value: meta?.userId ?? getAnonymousUserId() },
    { key: 'sessionId', value: meta?.sessionId ?? '' },
    { key: 'taskId', value: meta?.taskId ?? '' },
    { key: 'isInternal', value: meta?.isInternal ? 'true' : 'false' },
  ];

  return {
    customerResponses,
    category: survey.aperture.category,
    version: survey.aperture.version,
    name: survey.aperture.name,
    locale: 'en_US',
    reference: 'kiro-cli',
    location: 'kiro-cli',
    metadataList,
  };
}

// ─── Submission ──────────────────────────────────────────────────────────────

export interface SurveySubmitOutcome {
  ok: boolean;
  rateLimited: boolean;
  message?: string;
}

/**
 * Submit survey answers to Aperture. Swallows errors and returns a
 * normalized outcome so the caller can stay declarative.
 */
export async function submitFormToAperture(
  survey: SurveyDefinition,
  answers: Record<string, string>,
  options: { signal?: AbortSignal; metadata?: SurveyMetadata } = {}
): Promise<SurveySubmitOutcome> {
  const payload = buildSurveyPayload(survey, answers, options.metadata);

  // Decorate the base User-Agent with the active engine for this survey
  // submission only (do not mutate the shared aperture-client default).
  const engine = options.metadata?.agentEngine;
  // Map the store engine value to its user-facing label (v3 == KAS, v2 == Rust backend).
  const engineLabel: 'v3' | 'v2' | undefined = engine
    ? engine === 'kas'
      ? 'v3'
      : 'v2'
    : undefined;
  const userAgent = engineLabel
    ? `${buildUserAgent()} engine/${engineLabel}`
    : buildUserAgent();

  let result: ApertureSubmitResult;
  try {
    result = await submitForm(payload, {
      signal: options.signal,
      headers: { 'User-Agent': userAgent },
    });
  } catch (err) {
    logger.warn('[survey-submit] unexpected throw from submitForm', err);
    return { ok: false, rateLimited: false, message: 'Unexpected error' };
  }

  if (result.ok) {
    logger.info('[survey-submit] form submitted', {
      surveyId: survey.id,
      apertureName: survey.aperture.name,
      responseCount: payload.customerResponses.length,
    });
    return { ok: true, rateLimited: false };
  }

  const kind = result.error?.kind;
  logger.warn('[survey-submit] failed to submit form', {
    surveyId: survey.id,
    kind,
    status: result.error?.status,
    message: result.error?.message,
  });

  if (kind === 'rate_limited') {
    return {
      ok: false,
      rateLimited: true,
      message: 'Too many requests — please try again later.',
    };
  }
  return {
    ok: false,
    rateLimited: false,
    message: result.error?.message,
  };
}
