/**
 * Type definitions for the spec artifact parser.
 *
 * These describe the *summary* shape of each artifact — what the
 * structured artifact view renders to the user. The summary is a
 * deliberately lossy projection of the underlying markdown that
 * lets the user grasp the artifact at a glance and drill in to
 * a `detailBody` (verbatim source slice) on demand.
 */

export type ArtifactKind = 'requirements' | 'design' | 'tasks' | 'bugfix';

export interface RequirementItem {
  /** Verbatim integer captured from `### Requirement N:`. */
  number: number;
  /** Heading text after `### Requirement N: `. */
  title: string;
  /** Verbatim `**User Story:** …` line, or null if missing. */
  userStory: string | null;
  /** Full source slice for DetailView (heading + user story + acceptance criteria). */
  detailBody: string;
}

export interface DesignSection {
  /** H2 heading text (without the leading `## `). */
  title: string;
  /** Verbatim slice from this H2 to the next H2 or EOF. */
  detailBody: string;
}

export interface SubTask {
  title: string;
  checked: boolean;
  /** Indentation in spaces (tabs expanded to 4). */
  depth: number;
}

export interface HighLevelTask {
  /** Verbatim numbering string ("1", "1.0", "2"). */
  number: string;
  title: string;
  checked: boolean;
  /** Direct child checkbox items. */
  subTasks: SubTask[];
  /** Verbatim source slice from this task to the next high-level task or EOF. */
  detailBody: string;
}

export interface BugfixClause {
  /** Verbatim `X.Y` numbering: the section, then the clause within it. */
  number: string;
  /** The clause text after its number. */
  text: string;
}

export interface BugfixSection {
  /** H3 heading text, e.g. "Current Behavior (Defect)". */
  title: string;
  clauses: BugfixClause[];
  /** Verbatim slice from this heading to the next one or EOF. */
  detailBody: string;
}

export type ArtifactSummary =
  | { kind: 'requirements'; items: RequirementItem[] }
  | {
      kind: 'design';
      overview: string;
      overviewTruncated: boolean;
      sections: DesignSection[];
    }
  | { kind: 'tasks'; items: HighLevelTask[] }
  | { kind: 'bugfix'; overview: string; sections: BugfixSection[] };

/** Maximum overview length before truncation with ellipsis. */
export const DESIGN_OVERVIEW_MAX_CHARS = 600;
