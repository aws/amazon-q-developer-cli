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
 *     ? ` ↻ [${stage.loopIteration}/${stage.loopMaxIterations}]`
 *     : ''
 */
const stageRowLoopLabel = (stage: Stage): string =>
  stage.hasLoop && stage.loopMaxIterations
    ? ` ↻ [${stage.loopIteration}/${stage.loopMaxIterations}]`
    : '';

/**
 * Replicates the loop label logic from DagVisualization.tsx:
 *   stage?.hasLoop && stage.loopMaxIterations
 *     ? ` ↻[${stage.loopIteration}/${stage.loopMaxIterations}]`
 *     : ''
 */
const dagLoopLabel = (stage: Stage | undefined): string =>
  stage?.hasLoop && stage.loopMaxIterations
    ? ` ↻[${stage.loopIteration}/${stage.loopMaxIterations}]`
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
  it('when hasLoop=true and loopMaxIterations=4 and loopIteration=2 then renders ↻ [2/4]', () => {
    const stage = baseStage({
      hasLoop: true,
      loopIteration: 2,
      loopMaxIterations: 4,
    });
    expect(stageRowLoopLabel(stage)).toBe(' ↻ [2/4]');
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

  it('when loopIteration=0 and loopMaxIterations=5 then renders ↻ [0/5]', () => {
    const stage = baseStage({
      hasLoop: true,
      loopIteration: 0,
      loopMaxIterations: 5,
    });
    expect(stageRowLoopLabel(stage)).toBe(' ↻ [0/5]');
  });
});

// ---------------------------------------------------------------------------
// 4. DagVisualization loop label logic
// ---------------------------------------------------------------------------

describe('DagVisualization loop label', () => {
  it('when hasLoop=true and loopMaxIterations=3 and loopIteration=1 then renders ↻[1/3]', () => {
    const stage = baseStage({
      hasLoop: true,
      loopIteration: 1,
      loopMaxIterations: 3,
    });
    expect(dagLoopLabel(stage)).toBe(' ↻[1/3]');
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
