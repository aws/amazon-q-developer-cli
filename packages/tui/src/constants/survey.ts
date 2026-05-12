/**
 * Survey definitions for all Aperture-backed research surveys.
 *
 * Each survey is a self-contained definition that includes:
 *   - Aperture form coordinates (category, name, version)
 *   - Questions with their Aperture response types
 *   - Sampling rate and cooldown configuration
 *   - UI metadata (title, notification text)
 *
 * Keep this file dependency-free so it can be imported by both the store and
 * UI components without pulling React into non-UI code paths.
 */

// ─── Shared types ────────────────────────────────────────────────────────────

export type SurveyQuestionType = 'rating' | 'textArea' | 'text';

export interface SurveyQuestion {
  /** Internal ID used as the key in the answers map. */
  id: string;
  /**
   * The `question` field sent to Aperture (must match template's
   * `questionText` exactly).
   */
  apertureQuestionText: string;
  /** Aperture response type. */
  responseType: SurveyQuestionType;
  /** Whether this field contains PII. */
  pii: boolean;
  /** User-facing prompt shown in the TUI panel. */
  prompt: string;
  /** Options for rating questions (displayed in the TUI as radio choices). */
  options?: string[];
  /** For rating type: rowLabels sent to Aperture (user-facing question). */
  rowLabels?: string[];
  /** For rating type: columnLabels sent to Aperture (the option labels). */
  columnLabels?: string[];
  /** Placeholder for free-text inputs. */
  placeholder?: string;
  /** If true, Enter on an empty value is accepted. */
  optional?: boolean;
  /** Optional validator — return an error message to block submit, or null. */
  validate?: (value: string) => string | null;
}

export interface SurveyDefinition {
  /** Unique survey identifier used for state persistence keys. */
  id: string;
  /** Aperture form coordinates. */
  aperture: {
    category: string;
    name: string;
    version: string;
  };
  /** Title shown in the survey panel header. */
  title: string;
  /** Notification bar message when prompting the user. */
  notificationMessage: string;
  /** Sampling rate (0–1). Override via env var per survey. */
  sampleRate: number;
  /** Cooldown in days between prompts. */
  cooldownDays: number;
  /** Questions in display order. */
  questions: SurveyQuestion[];
}

// ─── Validation helpers ──────────────────────────────────────────────────────

/**
 * Basic email-shape validation. Deliberately lenient — we want to catch
 * obvious typos, not enforce RFC 5322.
 */
export function validateEmail(value: string): string | null {
  const trimmed = value.trim();
  if (!/^[^\s@]+@[^\s@]+\.[^\s@]+$/.test(trimmed)) {
    return 'Enter a valid email address or leave blank to skip';
  }
  return null;
}

// ─── Survey definitions ──────────────────────────────────────────────────────

export const SESSION_FEEDBACK_SURVEY: SurveyDefinition = {
  id: 'session-feedback',
  aperture: { category: 'KiroCLI', name: 'SessionFeedback', version: '1.0.0' },
  title: 'Research question',
  notificationMessage: 'How did Kiro do?',
  sampleRate: 0.05,
  cooldownDays: 90,
  questions: [
    {
      id: 'experience',
      apertureQuestionText: 'Experience with Kiro CLI',
      responseType: 'rating',
      pii: false,
      prompt: 'How would you rate your experience with Kiro CLI today?',
      options: ['Very poor', 'Poor', 'Fair', 'Good', 'Excellent'],
      rowLabels: ['How would your rate your experience with Kiro CLI today?'],
      columnLabels: ['Very poor', 'Poor', 'Fair', 'Good', 'Excellent'],
    },
    {
      id: 'feedback',
      apertureQuestionText:
        'Do you have additional feedback about this experience?',
      responseType: 'textArea',
      pii: false,
      prompt: 'Do you have additional feedback about this experience?',
      placeholder: 'provide your experience',
      optional: true,
    },
    {
      id: 'email',
      apertureQuestionText:
        'We may want to contact you about your feedback. Share your email to join our research panel. ',
      responseType: 'text',
      pii: true,
      prompt:
        'We may want to contact you about your feedback. Share your email to join our research panel.',
      placeholder: 'you@example.com — press enter to skip',
      optional: true,
      validate: (value) => {
        if (value.trim().length === 0) return null;
        return validateEmail(value);
      },
    },
  ],
};

export const PLAN_QUALITY_SURVEY: SurveyDefinition = {
  id: 'plan-quality',
  aperture: { category: 'KiroCLI', name: 'Plan', version: '1.0.0' },
  title: 'Plan feedback',
  notificationMessage: 'How did the planning agent do?',
  sampleRate: 0.1,
  cooldownDays: 90,
  questions: [
    {
      id: 'plan_quality',
      apertureQuestionText: '/plan',
      responseType: 'rating',
      pii: false,
      prompt:
        'How well did Kiro\u2019s plan capture what you are trying to accomplish?',
      options: [
        'Not at all well',
        'Not very well',
        'Somewhat well',
        'Very well',
        'Extremely well',
      ],
      rowLabels: [
        'How well did Kiro\u2019s plan capture what you are trying to accomplish?',
      ],
      // NOTE: trailing space on "Extremely well " matches the template.
      columnLabels: [
        'Not at all well',
        'Not very well',
        'Somewhat well',
        'Very well',
        'Extremely well ',
      ],
    },
    {
      id: 'feedback',
      apertureQuestionText:
        'Do you have additional feedback about this experience?',
      responseType: 'textArea',
      pii: false,
      prompt: 'Do you have additional feedback about this experience?',
      placeholder: 'provide your feedback',
      optional: true,
    },
  ],
};

export const IMPLEMENT_PLAN_SURVEY: SurveyDefinition = {
  id: 'implement-plan',
  aperture: { category: 'KiroCLI', name: 'ImplementPlan', version: '1.0.0' },
  title: 'Implementation feedback',
  notificationMessage: 'How was the implementation?',
  sampleRate: 1.0, // Only shown if plan-quality was shown — no independent sampling.
  cooldownDays: 90, // Shared cooldown with all surveys.
  questions: [
    {
      id: 'implementation_quality',
      apertureQuestionText: 'Implementation of plan',
      responseType: 'rating',
      pii: false,
      prompt:
        'How would you rate the quality of Kiro\u2019s implementation of the plan?',
      options: ['Very poor', 'Poor', 'Fair', 'Good', 'Excellent'],
      rowLabels: [
        "How would you rate the quality of Kiro's implementation of the plan?",
      ],
      columnLabels: ['Very poor', 'Poor', 'Fair', 'Good', 'Excellent'],
    },
    {
      id: 'feedback',
      apertureQuestionText:
        'Do you have additional feedback about this experience?',
      responseType: 'textArea',
      pii: false,
      prompt: 'Do you have any additional feedback about this experience?',
      placeholder: 'provide your feedback',
      optional: true,
    },
    {
      id: 'email',
      apertureQuestionText:
        'Can we contact you about your feedback? If so, please input your email',
      responseType: 'text',
      pii: true,
      prompt:
        'Can we contact you about your feedback? If so, please input your email',
      placeholder: 'you@example.com — press enter to skip',
      optional: true,
      validate: (value) => {
        if (value.trim().length === 0) return null;
        return validateEmail(value);
      },
    },
  ],
};

/** All surveys indexed by ID for easy lookup. */
export const ALL_SURVEYS: Record<string, SurveyDefinition> = {
  [SESSION_FEEDBACK_SURVEY.id]: SESSION_FEEDBACK_SURVEY,
  [PLAN_QUALITY_SURVEY.id]: PLAN_QUALITY_SURVEY,
  [IMPLEMENT_PLAN_SURVEY.id]: IMPLEMENT_PLAN_SURVEY,
};

/** Legacy alias — the "current" session survey for backward compat. */
export const CURRENT_SURVEY = SESSION_FEEDBACK_SURVEY;
