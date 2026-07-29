export interface ToolBreakdownItem {
  name: string;
  tokens: number;
  percent: number;
}

export interface ToolGroupBreakdown {
  name: string;
  source: string;
  tokens: number;
  percent: number;
  items: ToolBreakdownItem[];
}

export interface ContextFileBreakdownItem {
  name: string;
  tokens: number;
  matched: boolean;
  percent: number;
  autoIncluded?: boolean;
}

export interface ContextBreakdownData {
  contextFiles: {
    percent: number;
    tokens: number;
    items?: ContextFileBreakdownItem[];
  };
  tools: {
    percent: number;
    tokens: number;
    groups?: ToolGroupBreakdown[];
  };
  kiroResponses: { percent: number; tokens: number };
  yourPrompts: { percent: number; tokens: number };
  sessionFiles?: {
    percent: number;
    tokens: number;
    items?: ContextFileBreakdownItem[];
  };
  /** UI-specific: initially show context breakdown in expanded mode. */
  initialExpanded?: boolean;
}
