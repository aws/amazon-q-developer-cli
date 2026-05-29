import { describe, it, expect } from 'bun:test';
import { mapSessionStatusToStageState } from '../types.js';
import type { Stage } from '../types.js';

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

const baseStage = (overrides: Partial<Stage> = {}): Stage => ({
  name: 'test-stage',
  agentName: 'test-agent',
  state: 'Executing',
  description: 'desc',
  events: 0,
  role: 'worker',
  sessionId: 'sess-1',
  dependsOn: [],
  hasLoop: false,
  loopIteration: 0,
  loopMaxIterations: 0,
  ...overrides,
});

/**
 * Replicates the loop label logic from StageRow.tsx:
 *   stage.hasLoop && stage.loopMaxIterations
 *     ? ` ↻ [${(stage.loopIteration ?? 0) + 1}/${stage.loopMaxIterations}]`
 *     : ''
 */
const stageRowLoopLabel = (stage: Stage): string =>
  stage.hasLoop && stage.loopMaxIterations
    ? ` ↻ [${(stage.loopIteration ?? 0) + 1}/${stage.loopMaxIterations}]`
    : '';

/**
 * Replicates the loop label logic from DagVisualization.tsx:
 *   stage?.hasLoop && stage.loopMaxIterations
 *     ? ` ↻[${(stage.loopIteration ?? 0) + 1}/${stage.loopMaxIterations}]`
 *     : ''
 */
const dagLoopLabel = (stage: Stage | undefined): string =>
  stage?.hasLoop && stage.loopMaxIterations
    ? ` ↻[${(stage.loopIteration ?? 0) + 1}/${stage.loopMaxIterations}]`
    : '';

// ---------------------------------------------------------------------------
// 1. mapSessionStatusToStageState
// ---------------------------------------------------------------------------

describe('mapSessionStatusToStageState', () => {
  it('when status is busy then returns Executing', () => {
    expect(mapSessionStatusToStageState('busy')).toBe('Executing');
  });

  it('when status is terminated then returns Completed', () => {
    expect(mapSessionStatusToStageState('terminated')).toBe('Completed');
  });

  it('when status is failed then returns Failed', () => {
    expect(mapSessionStatusToStageState('failed')).toBe('Failed');
  });

  it('when status is pending then returns Pending', () => {
    expect(mapSessionStatusToStageState('pending')).toBe('Pending');
  });

  it('when status is idle then returns Pending', () => {
    expect(mapSessionStatusToStageState('idle')).toBe('Pending');
  });
});

// ---------------------------------------------------------------------------
// 2. Stage type — loop fields
// ---------------------------------------------------------------------------

describe('Stage type loop fields', () => {
  it('when Stage has loop fields then they are accessible', () => {
    const stage = baseStage({
      hasLoop: true,
      loopIteration: 3,
      loopMaxIterations: 5,
    });
    expect(stage.hasLoop).toBe(true);
    expect(stage.loopIteration).toBe(3);
    expect(stage.loopMaxIterations).toBe(5);
  });

  it('when Stage omits loop fields then they default to undefined', () => {
    const stage: Stage = {
      name: 'x',
      agentName: 'a',
      state: 'Pending',
      description: '',
      events: 0,
      role: 'r',
      sessionId: 's',
    };
    expect(stage.hasLoop).toBeUndefined();
    expect(stage.loopIteration).toBeUndefined();
    expect(stage.loopMaxIterations).toBeUndefined();
  });
});

// ---------------------------------------------------------------------------
// 3. StageRow loop label logic
// ---------------------------------------------------------------------------

describe('StageRow loop label', () => {
  it('when hasLoop=true and loopMaxIterations=4 and loopIteration=2 then renders ↻ [3/4]', () => {
    const stage = baseStage({
      hasLoop: true,
      loopIteration: 2,
      loopMaxIterations: 4,
    });
    expect(stageRowLoopLabel(stage)).toBe(' ↻ [3/4]');
  });

  it('when hasLoop=false then no loop label', () => {
    const stage = baseStage({ hasLoop: false });
    expect(stageRowLoopLabel(stage)).toBe('');
  });

  it('when hasLoop=true but loopMaxIterations=0 then no loop label', () => {
    const stage = baseStage({
      hasLoop: true,
      loopIteration: 0,
      loopMaxIterations: 0,
    });
    expect(stageRowLoopLabel(stage)).toBe('');
  });

  it('when loopIteration=0 and loopMaxIterations=5 then renders ↻ [1/5]', () => {
    const stage = baseStage({
      hasLoop: true,
      loopIteration: 0,
      loopMaxIterations: 5,
    });
    expect(stageRowLoopLabel(stage)).toBe(' ↻ [1/5]');
  });
});

// ---------------------------------------------------------------------------
// 4. DagVisualization loop label logic
// ---------------------------------------------------------------------------

describe('DagVisualization loop label', () => {
  it('when hasLoop=true and loopMaxIterations=3 and loopIteration=1 then renders ↻[2/3]', () => {
    const stage = baseStage({
      hasLoop: true,
      loopIteration: 1,
      loopMaxIterations: 3,
    });
    expect(dagLoopLabel(stage)).toBe(' ↻[2/3]');
  });

  it('when hasLoop=false then no loop label', () => {
    const stage = baseStage({ hasLoop: false });
    expect(dagLoopLabel(stage)).toBe('');
  });

  it('when stage is undefined then no loop label', () => {
    expect(dagLoopLabel(undefined)).toBe('');
  });
});

// ---------------------------------------------------------------------------
// 5. AgentSession → Stage mapping (loop fields pass through)
// ---------------------------------------------------------------------------

describe('AgentSession to Stage loop field mapping', () => {
  it('when session has loop fields then they map to Stage', () => {
    const session = {
      hasLoop: true,
      loopIteration: 2,
      loopMaxIterations: 5,
    };

    // Replicates CrewMonitorScreen.tsx mapping
    const stage: Partial<Stage> = {
      hasLoop: (session as any).hasLoop ?? false,
      loopIteration: (session as any).loopIteration ?? 0,
      loopMaxIterations: (session as any).loopMaxIterations ?? 0,
    };

    expect(stage.hasLoop).toBe(true);
    expect(stage.loopIteration).toBe(2);
    expect(stage.loopMaxIterations).toBe(5);
  });

  it('when session has no loop fields then defaults to no-loop', () => {
    const session = {};

    const stage: Partial<Stage> = {
      hasLoop: (session as any).hasLoop ?? false,
      loopIteration: (session as any).loopIteration ?? 0,
      loopMaxIterations: (session as any).loopMaxIterations ?? 0,
    };

    expect(stage.hasLoop).toBe(false);
    expect(stage.loopIteration).toBe(0);
    expect(stage.loopMaxIterations).toBe(0);
  });
});

// ---------------------------------------------------------------------------
// 6. Loop iteration deduplication
// ---------------------------------------------------------------------------

/**
 * Replicates the deduplication logic from CrewMonitorScreen.tsx:
 * When a loop fires, the backend creates new sessions with the same name.
 * The monitor should show only the latest iteration per stage name+group.
 * At the same loopIteration, prefers the most recently created session.
 */
function deduplicateStages(
  allStages: (Stage & { created?: number })[]
): Stage[] {
  const deduped = new Map<string, (typeof allStages)[0]>();
  for (const stage of allStages) {
    const key = `${stage.group ?? ''}::${stage.name}`;
    const existing = deduped.get(key);
    if (!existing) {
      deduped.set(key, stage);
    } else if ((stage.loopIteration ?? 0) > (existing.loopIteration ?? 0)) {
      deduped.set(key, stage);
    } else if ((stage.loopIteration ?? 0) === (existing.loopIteration ?? 0)) {
      // Same iteration — prefer most recently created
      if ((stage.created ?? 0) > (existing.created ?? 0)) {
        deduped.set(key, stage);
      }
    }
  }
  return [...deduped.values()];
}

describe('Loop iteration deduplication', () => {
  it('keeps only the latest iteration per stage name', () => {
    const stages = [
      baseStage({
        name: 'writer',
        sessionId: 's1',
        loopIteration: 0,
        state: 'Completed',
        group: 'g1',
        created: 100,
      } as any),
      baseStage({
        name: 'reviewer',
        sessionId: 's2',
        loopIteration: 0,
        hasLoop: true,
        loopMaxIterations: 2,
        state: 'Completed',
        group: 'g1',
        created: 200,
      } as any),
      baseStage({
        name: 'writer',
        sessionId: 's3',
        loopIteration: 1,
        state: 'Executing',
        group: 'g1',
        created: 300,
      } as any),
      baseStage({
        name: 'reviewer',
        sessionId: 's4',
        loopIteration: 1,
        hasLoop: true,
        loopMaxIterations: 2,
        state: 'Pending',
        group: 'g1',
        created: 400,
      } as any),
    ];

    const result = deduplicateStages(stages);
    expect(result).toHaveLength(2);
    expect(result.find((s) => s.name === 'writer')?.sessionId).toBe('s3');
    expect(result.find((s) => s.name === 'reviewer')?.sessionId).toBe('s4');
  });

  it('does not deduplicate across different groups', () => {
    const stages = [
      baseStage({
        name: 'writer',
        sessionId: 's1',
        group: 'g1',
        created: 100,
      } as any),
      baseStage({
        name: 'writer',
        sessionId: 's2',
        group: 'g2',
        created: 200,
      } as any),
    ];

    const result = deduplicateStages(stages);
    expect(result).toHaveLength(2);
  });

  it('prefers most recently created at same iteration', () => {
    const stages = [
      baseStage({
        name: 'writer',
        sessionId: 's1',
        loopIteration: 0,
        state: 'Completed',
        group: 'g1',
        created: 100,
      } as any),
      baseStage({
        name: 'writer',
        sessionId: 's2',
        loopIteration: 0,
        state: 'Completed',
        group: 'g1',
        created: 200,
      } as any),
    ];

    const result = deduplicateStages(stages);
    expect(result).toHaveLength(1);
    expect(result[0]?.sessionId).toBe('s2');
  });

  it('with no duplicates returns all stages unchanged', () => {
    const stages = [
      baseStage({
        name: 'writer',
        sessionId: 's1',
        group: 'g1',
        created: 100,
      } as any),
      baseStage({
        name: 'reviewer',
        sessionId: 's2',
        group: 'g1',
        created: 200,
      } as any),
    ];

    const result = deduplicateStages(stages);
    expect(result).toHaveLength(2);
  });
});

// ---------------------------------------------------------------------------
// 7. Off-by-one regression: loopIteration is 0-indexed, display is 1-indexed
// ---------------------------------------------------------------------------

describe('Loop display is 1-indexed (human-friendly round count)', () => {
  it('after 3 rounds with max 4, StageRow shows ↻ [3/4] not [2/4]', () => {
    // Backend sets loopIteration=2 after 3 rounds (0-indexed)
    const stage = baseStage({
      hasLoop: true,
      loopIteration: 2,
      loopMaxIterations: 4,
    });
    expect(stageRowLoopLabel(stage)).toBe(' ↻ [3/4]');
  });

  it('after 3 rounds with max 4, DagVisualization shows ↻[3/4] not [2/4]', () => {
    const stage = baseStage({
      hasLoop: true,
      loopIteration: 2,
      loopMaxIterations: 4,
    });
    expect(dagLoopLabel(stage)).toBe(' ↻[3/4]');
  });

  it('first round (loopIteration=0) shows 1/N not 0/N', () => {
    const stage = baseStage({
      hasLoop: true,
      loopIteration: 0,
      loopMaxIterations: 4,
    });
    expect(stageRowLoopLabel(stage)).toBe(' ↻ [1/4]');
    expect(dagLoopLabel(stage)).toBe(' ↻[1/4]');
  });

  it('max iterations reached (loopIteration=3, max=4) shows 4/4', () => {
    const stage = baseStage({
      hasLoop: true,
      loopIteration: 3,
      loopMaxIterations: 4,
    });
    expect(stageRowLoopLabel(stage)).toBe(' ↻ [4/4]');
    expect(dagLoopLabel(stage)).toBe(' ↻[4/4]');
  });
});
