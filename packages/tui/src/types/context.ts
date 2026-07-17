export interface ContextBreakdownData {
  contextFiles: {
    percent: number;
    tokens: number;
    items?: Array<{
      name: string;
      tokens: number;
      matched: boolean;
      percent: number;
    }>;
  };
  tools: { percent: number; tokens: number };
  kiroResponses: { percent: number; tokens: number };
  yourPrompts: { percent: number; tokens: number };
  sessionFiles?: { percent: number; tokens: number };
  /** UI-specific: initially show context breakdown in expanded mode. */
  initialExpanded?: boolean;
}
