import { describe, it, expect, beforeEach, mock, afterAll } from 'bun:test';

// Mock logger BEFORE importing inputMetrics to suppress file I/O
mock.module('../logger.js', () => ({
  logger: {
    info: () => {},
    debug: () => {},
    warn: () => {},
    error: () => {},
    trace: () => {},
  },
}));

afterAll(() => {
  mock.restore();
});

const { inputMetrics } = await import('../inputMetrics');

beforeEach(() => {
  inputMetrics.clear();
  inputMetrics.enable();
});

describe('InputMetrics', () => {
  describe('enable/disable', () => {
    it('isEnabled returns true after enable()', () => {
      inputMetrics.enable();
      expect(inputMetrics.isEnabled()).toBe(true);
    });

    it('isEnabled returns false after disable()', () => {
      inputMetrics.disable();
      expect(inputMetrics.isEnabled()).toBe(false);
    });

    it('toggle enable/disable', () => {
      inputMetrics.disable();
      expect(inputMetrics.isEnabled()).toBe(false);
      inputMetrics.enable();
      expect(inputMetrics.isEnabled()).toBe(true);
    });
  });

  describe('getStats', () => {
    it('returns null when no samples collected', () => {
      expect(inputMetrics.getStats()).toBeNull();
    });
  });

  describe('full lifecycle', () => {
    it('records a sample through the full mark sequence', () => {
      inputMetrics.markKeypress('a');
      inputMetrics.markHandlerStart();
      inputMetrics.markStateUpdate();
      inputMetrics.markRenderComplete();

      const stats = inputMetrics.getStats();
      expect(stats).not.toBeNull();
      expect(stats!.count).toBe(1);
      expect(stats!.avgKeypressToHandler).toBeGreaterThanOrEqual(0);
      expect(stats!.avgHandlerToStateUpdate).toBeGreaterThanOrEqual(0);
      expect(stats!.avgStateUpdateToRender).toBeGreaterThanOrEqual(0);
      expect(stats!.avgTotal).toBeGreaterThanOrEqual(0);
    });

    it('records correct sample data accessible via getSamples', () => {
      inputMetrics.markKeypress('x');
      inputMetrics.markHandlerStart();
      inputMetrics.markStateUpdate();
      inputMetrics.markRenderComplete();

      const samples = inputMetrics.getSamples();
      expect(samples).toHaveLength(1);
      expect(samples[0]!.inputChar).toBe('x');
      expect(samples[0]!.totalLatency).toBeGreaterThanOrEqual(0);
      expect(samples[0]!.timestamp).toBeGreaterThan(0);
    });
  });

  describe('disabled metrics', () => {
    it('mark functions are no-ops when disabled', () => {
      inputMetrics.disable();
      inputMetrics.markKeypress('a');
      inputMetrics.markHandlerStart();
      inputMetrics.markStateUpdate();
      inputMetrics.markRenderComplete();

      expect(inputMetrics.getStats()).toBeNull();
      expect(inputMetrics.getSamples()).toHaveLength(0);
    });
  });

  describe('markRenderComplete with missing markers', () => {
    it('resets without recording when handlerTime is missing', () => {
      inputMetrics.markKeypress('a');
      // skip markHandlerStart
      inputMetrics.markStateUpdate();
      inputMetrics.markRenderComplete();

      expect(inputMetrics.getSamples()).toHaveLength(0);
    });

    it('resets without recording when stateUpdateTime is missing', () => {
      inputMetrics.markKeypress('a');
      inputMetrics.markHandlerStart();
      // skip markStateUpdate
      inputMetrics.markRenderComplete();

      expect(inputMetrics.getSamples()).toHaveLength(0);
    });

    it('resets without recording when keypressTime is missing', () => {
      // skip markKeypress
      inputMetrics.markHandlerStart();
      inputMetrics.markStateUpdate();
      inputMetrics.markRenderComplete();

      expect(inputMetrics.getSamples()).toHaveLength(0);
    });
  });

  describe('render phase tracking', () => {
    it('tracks render phases during a full lifecycle', () => {
      inputMetrics.markKeypress('b');
      inputMetrics.markHandlerStart();
      inputMetrics.markStateUpdate();
      inputMetrics.startRenderPhase('ChatView');
      inputMetrics.endRenderPhase('ChatView');
      inputMetrics.markRenderComplete();

      // Phase tracking is internal; verify sample was still recorded
      const samples = inputMetrics.getSamples();
      expect(samples).toHaveLength(1);
    });

    it('endRenderPhase is no-op without startRenderPhase', () => {
      inputMetrics.markKeypress('c');
      inputMetrics.markHandlerStart();
      inputMetrics.markStateUpdate();
      // endRenderPhase without start should not throw
      inputMetrics.endRenderPhase('Orphan');
      inputMetrics.markRenderComplete();

      expect(inputMetrics.getSamples()).toHaveLength(1);
    });
  });

  describe('multiple samples', () => {
    it('produces correct count and stats', () => {
      for (let i = 0; i < 5; i++) {
        inputMetrics.markKeypress(String(i));
        inputMetrics.markHandlerStart();
        inputMetrics.markStateUpdate();
        inputMetrics.markRenderComplete();
      }

      const stats = inputMetrics.getStats();
      expect(stats).not.toBeNull();
      expect(stats!.count).toBe(5);
      expect(stats!.p50Total).toBeGreaterThanOrEqual(0);
      expect(stats!.p95Total).toBeGreaterThanOrEqual(0);
      expect(stats!.p99Total).toBeGreaterThanOrEqual(0);
      expect(stats!.maxTotal).toBeGreaterThanOrEqual(0);
      expect(stats!.maxTotal).toBeGreaterThanOrEqual(stats!.p50Total);
    });
  });

  describe('clear', () => {
    it('removes all collected samples', () => {
      inputMetrics.markKeypress('a');
      inputMetrics.markHandlerStart();
      inputMetrics.markStateUpdate();
      inputMetrics.markRenderComplete();

      expect(inputMetrics.getSamples()).toHaveLength(1);
      inputMetrics.clear();
      expect(inputMetrics.getSamples()).toHaveLength(0);
      expect(inputMetrics.getStats()).toBeNull();
    });
  });

  describe('getSamples', () => {
    it('returns a copy of the samples array', () => {
      inputMetrics.markKeypress('z');
      inputMetrics.markHandlerStart();
      inputMetrics.markStateUpdate();
      inputMetrics.markRenderComplete();

      const samples1 = inputMetrics.getSamples();
      const samples2 = inputMetrics.getSamples();
      expect(samples1).toEqual(samples2);
      expect(samples1).not.toBe(samples2); // different array references
    });
  });

  describe('logStats', () => {
    it('does not throw when no samples', () => {
      inputMetrics.clear();
      expect(() => inputMetrics.logStats()).not.toThrow();
    });

    it('does not throw when samples exist', () => {
      inputMetrics.markKeypress('a');
      inputMetrics.markHandlerStart();
      inputMetrics.markStateUpdate();
      inputMetrics.markRenderComplete();
      expect(() => inputMetrics.logStats()).not.toThrow();
    });
  });

  describe('multi-char input', () => {
    it('records multi-char input as bracketed length', () => {
      inputMetrics.markKeypress('abc');
      inputMetrics.markHandlerStart();
      inputMetrics.markStateUpdate();
      inputMetrics.markRenderComplete();

      const samples = inputMetrics.getSamples();
      expect(samples[0]!.inputChar).toBe('[3 chars]');
    });
  });
});
