import { logger } from './logger.js';

interface MetricSample {
  timestamp: number;
  keypressToHandler: number;
  handlerToStateUpdate: number;
  stateUpdateToRender: number;
  totalLatency: number;
  inputChar: string;
}

interface RenderPhaseTiming {
  componentName: string;
  duration: number;
}

interface MetricStats {
  count: number;
  avgKeypressToHandler: number;
  avgHandlerToStateUpdate: number;
  avgStateUpdateToRender: number;
  avgTotal: number;
  p50Total: number;
  p95Total: number;
  p99Total: number;
  maxTotal: number;
}

class InputMetrics {
  private samples: MetricSample[] = [];
  private enabled: boolean;
  private maxSamples = 1000;

  // Timing markers for current keypress
  private keypressTime: number | null = null;
  private handlerTime: number | null = null;
  private stateUpdateTime: number | null = null;
  private currentChar: string = '';

  // Render phase tracking
  private renderPhases: RenderPhaseTiming[] = [];
  private currentRenderStart: number | null = null;

  constructor() {
    this.enabled =
      process.env.KIRO_INPUT_METRICS === 'true' ||
      process.env.KIRO_TEST_MODE === '1';
    if (this.enabled) {
      logger.info('[InputMetrics] Input latency metrics enabled');
    }
  }

  isEnabled(): boolean {
    return this.enabled;
  }

  enable(): void {
    this.enabled = true;
    logger.info('[InputMetrics] Input latency metrics enabled');
  }

  disable(): void {
    this.enabled = false;
    logger.info('[InputMetrics] Input latency metrics disabled');
  }

  /**
   * Mark when a keypress event is received (earliest possible point)
   */
  markKeypress(char: string): void {
    if (!this.enabled) return;
    this.keypressTime = performance.now();
    this.currentChar = char.length === 1 ? char : `[${char.length} chars]`;
  }

  /**
   * Mark when the keypress handler starts processing
   */
  markHandlerStart(): void {
    if (!this.enabled || this.keypressTime === null) return;
    this.handlerTime = performance.now();
  }

  /**
   * Mark when state update (setState) is called
   */
  markStateUpdate(): void {
    if (!this.enabled || this.handlerTime === null) return;
    this.stateUpdateTime = performance.now();
  }

  /**
   * Start timing a render phase (for detailed breakdown)
   */
  startRenderPhase(componentName: string): void {
    if (!this.enabled) return;
    this.currentRenderStart = performance.now();
  }

  /**
   * End timing a render phase
   */
  endRenderPhase(componentName: string): void {
    if (!this.enabled || this.currentRenderStart === null) return;
    const duration = performance.now() - this.currentRenderStart;
    this.renderPhases.push({ componentName, duration });
    this.currentRenderStart = null;
  }

  /**
   * Mark when render completes and record the sample
   */
  markRenderComplete(): void {
    if (!this.enabled) return;
    if (
      this.keypressTime === null ||
      this.handlerTime === null ||
      this.stateUpdateTime === null
    ) {
      this.reset();
      return;
    }

    const renderTime = performance.now();

    const sample: MetricSample = {
      timestamp: Date.now(),
      keypressToHandler: this.handlerTime - this.keypressTime,
      handlerToStateUpdate: this.stateUpdateTime - this.handlerTime,
      stateUpdateToRender: renderTime - this.stateUpdateTime,
      totalLatency: renderTime - this.keypressTime,
      inputChar: this.currentChar,
    };

    this.samples.push(sample);

    // Keep only recent samples
    if (this.samples.length > this.maxSamples) {
      this.samples = this.samples.slice(-this.maxSamples);
    }

    // Log render phase breakdown if we have any
    if (this.renderPhases.length > 0) {
      const phaseStr = this.renderPhases
        .map((p) => `${p.componentName}=${p.duration.toFixed(2)}ms`)
        .join(' ');
      logger.debug(`[InputMetrics] Render phases: ${phaseStr}`);
    }

    // Log individual sample at debug level
    logger.debug(
      `[InputMetrics] char="${sample.inputChar}" ` +
        `keypress→handler=${sample.keypressToHandler.toFixed(2)}ms ` +
        `handler→state=${sample.handlerToStateUpdate.toFixed(2)}ms ` +
        `state→render=${sample.stateUpdateToRender.toFixed(2)}ms ` +
        `total=${sample.totalLatency.toFixed(2)}ms`
    );

    this.reset();
  }

  private reset(): void {
    this.keypressTime = null;
    this.handlerTime = null;
    this.stateUpdateTime = null;
    this.currentChar = '';
    this.renderPhases = [];
    this.currentRenderStart = null;
  }

  /**
   * Get statistics for collected samples
   */
  getStats(): MetricStats | null {
    if (this.samples.length === 0) return null;

    const totals = this.samples
      .map((s) => s.totalLatency)
      .sort((a, b) => a - b);
    const count = this.samples.length;

    const sum = (arr: number[]) => arr.reduce((a, b) => a + b, 0);
    const avg = (arr: number[]) => sum(arr) / arr.length;
    const percentile = (arr: number[], p: number): number => {
      const idx = Math.ceil((p / 100) * arr.length) - 1;
      return arr[Math.max(0, idx)] ?? 0;
    };

    return {
      count,
      avgKeypressToHandler: avg(this.samples.map((s) => s.keypressToHandler)),
      avgHandlerToStateUpdate: avg(
        this.samples.map((s) => s.handlerToStateUpdate)
      ),
      avgStateUpdateToRender: avg(
        this.samples.map((s) => s.stateUpdateToRender)
      ),
      avgTotal: avg(totals),
      p50Total: percentile(totals, 50),
      p95Total: percentile(totals, 95),
      p99Total: percentile(totals, 99),
      maxTotal: totals[totals.length - 1] ?? 0,
    };
  }

  /**
   * Log current statistics summary
   */
  logStats(): void {
    const stats = this.getStats();
    if (!stats) {
      logger.info('[InputMetrics] No samples collected yet');
      return;
    }

    logger.info(
      `[InputMetrics] Stats (n=${stats.count}):\n` +
        `  Avg keypress→handler: ${stats.avgKeypressToHandler.toFixed(2)}ms\n` +
        `  Avg handler→state:    ${stats.avgHandlerToStateUpdate.toFixed(2)}ms\n` +
        `  Avg state→render:     ${stats.avgStateUpdateToRender.toFixed(2)}ms\n` +
        `  Total latency:\n` +
        `    avg=${stats.avgTotal.toFixed(2)}ms\n` +
        `    p50=${stats.p50Total.toFixed(2)}ms\n` +
        `    p95=${stats.p95Total.toFixed(2)}ms\n` +
        `    p99=${stats.p99Total.toFixed(2)}ms\n` +
        `    max=${stats.maxTotal.toFixed(2)}ms`
    );
  }

  /**
   * Clear all collected samples
   */
  clear(): void {
    this.samples = [];
    this.reset();
  }

  /**
   * Get raw samples for analysis
   */
  getSamples(): MetricSample[] {
    return [...this.samples];
  }
}

export const inputMetrics = new InputMetrics();
