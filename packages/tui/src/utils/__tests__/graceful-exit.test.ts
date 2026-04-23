import { describe, it, expect, beforeEach, afterEach, mock } from 'bun:test';

let originalExit: typeof process.exit;
let mockExit: ReturnType<typeof mock>;

beforeEach(() => {
  originalExit = process.exit;
  mockExit = mock(() => {});
  process.exit = mockExit as any;
});

afterEach(() => {
  process.exit = originalExit;
});

// Helper to get a fresh module instance (avoids persistent `exiting` state).
// Bun treats each unique specifier as a separate module instance.
let importCounter = 0;
async function freshModule() {
  importCounter++;
  return await import(`../graceful-exit.ts?v=${importCounter}`);
}

describe('graceful-exit', () => {
  describe('isExiting', () => {
    it('returns false initially', async () => {
      const mod = await freshModule();
      expect(mod.isExiting()).toBe(false);
    });
  });

  describe('registerInstance', () => {
    it('accepts drain and unmount functions', async () => {
      const mod = await freshModule();
      const drain = mock(() => Promise.resolve());
      const unmount = mock(() => {});
      // Should not throw
      mod.registerInstance(drain, unmount);
    });

    it('accepts undefined drain', async () => {
      const mod = await freshModule();
      const unmount = mock(() => {});
      // Should not throw
      mod.registerInstance(undefined, unmount);
    });
  });

  describe('gracefulExit without drain', () => {
    it('calls unmount then process.exit', async () => {
      const mod = await freshModule();
      const unmount = mock(() => {});
      mod.registerInstance(undefined, unmount);

      mod.gracefulExit(0);
      expect(unmount).toHaveBeenCalledTimes(1);
      expect(mockExit).toHaveBeenCalledWith(0);
    });

    it('passes code parameter to process.exit', async () => {
      const mod = await freshModule();
      const unmount = mock(() => {});
      mod.registerInstance(undefined, unmount);

      mod.gracefulExit(42);
      expect(mockExit).toHaveBeenCalledWith(42);
    });

    it('uses default code 0 when not specified', async () => {
      const mod = await freshModule();
      const unmount = mock(() => {});
      mod.registerInstance(undefined, unmount);

      mod.gracefulExit();
      expect(mockExit).toHaveBeenCalledWith(0);
    });

    it('sets isExiting to true', async () => {
      const mod = await freshModule();
      const unmount = mock(() => {});
      mod.registerInstance(undefined, unmount);

      mod.gracefulExit(0);
      expect(mod.isExiting()).toBe(true);
    });
  });

  describe('gracefulExit with drain', () => {
    it('calls drain then unmount then process.exit', async () => {
      const mod = await freshModule();
      const callOrder: string[] = [];
      const drain = mock(() => {
        callOrder.push('drain');
        return Promise.resolve();
      });
      const unmount = mock(() => {
        callOrder.push('unmount');
      });
      // Override process.exit to record call order
      mockExit = mock(() => {
        callOrder.push('exit');
      });
      process.exit = mockExit as any;

      mod.registerInstance(drain, unmount);
      mod.gracefulExit(0);

      // drain is async, so we need to wait for the promise chain
      await new Promise((r) => setTimeout(r, 50));

      expect(drain).toHaveBeenCalledTimes(1);
      expect(unmount).toHaveBeenCalledTimes(1);
      expect(mockExit).toHaveBeenCalledWith(0);
      expect(callOrder).toEqual(['drain', 'unmount', 'exit']);
    });

    it('calls unmount and process.exit even if drain rejects', async () => {
      const mod = await freshModule();
      const drain = mock(() => Promise.reject(new Error('drain failed')));
      const unmount = mock(() => {});

      mod.registerInstance(drain, unmount);
      mod.gracefulExit(1);

      await new Promise((r) => setTimeout(r, 50));

      expect(unmount).toHaveBeenCalledTimes(1);
      expect(mockExit).toHaveBeenCalledWith(1);
    });
  });

  describe('idempotency', () => {
    it('second gracefulExit call is a no-op', async () => {
      const mod = await freshModule();
      const unmount = mock(() => {});
      mod.registerInstance(undefined, unmount);

      mod.gracefulExit(0);
      expect(unmount).toHaveBeenCalledTimes(1);
      expect(mockExit).toHaveBeenCalledTimes(1);

      // Reset mock counts to verify second call does nothing
      unmount.mockClear();
      mockExit.mockClear();

      mod.gracefulExit(0);
      expect(unmount).toHaveBeenCalledTimes(0);
      expect(mockExit).toHaveBeenCalledTimes(0);
    });
  });

  describe('gracefulExit with no registered instance', () => {
    it('calls process.exit even without registered unmount', async () => {
      const mod = await freshModule();
      // Do not register any instance
      mod.gracefulExit(0);
      expect(mockExit).toHaveBeenCalledWith(0);
    });
  });
});
