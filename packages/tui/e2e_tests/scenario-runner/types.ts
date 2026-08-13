import type { SerializedAppState } from '../../src/test-utils/shared/ipc-types';

export type Engine = 'v2' | 'kas';
export type ScenarioBackendId = 'live' | 'acp-mock' | 'krs-mock';

export interface Scenario {
  id: string;
  name: string;
  category: string;
  description: string;
  docRef?: string;
  steps: string[];
  verify: string[];
  observe?: string;
  acpMock?: Record<string, unknown>;
  engine?: Engine[];
  backend?: ScenarioBackendId[];
  timeout?: number;
  tags?: string[];
  priority?: 'p0' | 'p1' | 'p2';
  /** Extra env for the spawned TUI (e.g. rollout gates for dark-shipped commands). */
  env?: Record<string, string>;
}

export type ExitReason =
  | 'passed'
  | 'assertion-failed'
  | 'timeout'
  | 'crash'
  | 'fixture-missing';

export interface StepTiming {
  step: string;
  index: number;
  startedAt: number;
  duration: number;
  status: 'done' | 'error';
  error?: string;
}

export interface VerifyResult {
  predicate: string;
  passed: boolean;
  actual?: string;
}

export interface Frame {
  label: string;
  timestamp: number;
  text: string[];
  html: string;
}

export interface TestHarness {
  sendKeys(input: string | number[]): Promise<void>;
  pressEnter(): Promise<void>;
  pressEscape(): Promise<void>;
  pressCtrlC(): Promise<void>;
  pressCtrlCTwice(): Promise<void>;
  sleepMs(ms: number): Promise<void>;
  getStore(): Promise<SerializedAppState>;
  getSnapshot(): string[];
  getSnapshotHtml(): string;
  waitForText(text: string, timeout?: number): Promise<void>;
  waitForIdle(timeout?: number): Promise<void>;
  expectExit(timeout?: number): Promise<number>;
  cleanup(): Promise<void>;
}

export interface ScenarioBackend {
  id: ScenarioBackendId;
  engine: Engine;
  launch(scenario: Scenario, opts: RunOptions): Promise<TestHarness>;
}

export interface RunOptions {
  backend: ScenarioBackend;
  scenariosPath?: string;
  scenarios?: string[];
  categories?: string[];
  tags?: string[];
  priority?: string[];
  timeout?: number;
  terminal?: { width: number; height: number };
  captureFrames?: boolean;
  parallel?: number;
  outputDir?: string;
  fixturesDir?: string;
}

export interface ScenarioResult {
  scenario: Scenario;
  backendId: string;
  engine: Engine;
  passed: boolean;
  duration: number;
  exitReason: ExitReason;
  verifyResults: VerifyResult[];
  stepTimings: StepTiming[];
  error?: string;
  frames: Frame[];
}

export interface RunReport {
  backendId: string;
  engine: Engine;
  startedAt: number;
  completedAt: number;
  total: number;
  passed: number;
  failed: number;
  skipped: number;
  skippedScenarioIds?: string[];
  results: ScenarioResult[];
}
