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
  timeout?: number;
  tags?: string[];
  priority?: 'p0' | 'p1' | 'p2';
  /** Extra env for the spawned TUI (e.g. rollout gates for dark-shipped commands). */
  env?: Record<string, string>;
  /**
   * Terminal size for this scenario, overriding the lane default. Layout-
   * dependent assertions need it: the same table renders as a grid or as
   * stacked rows purely as a function of width.
   */
  terminal?: { width: number; height: number };
  /**
   * Scripted KRS turns answering this scenario's prompts, validated by
   * krs-turns.schema.json. Their absence is what makes a scenario unrunnable
   * against the fake Kiro Runtime Service.
   */
  turns?: unknown[];
  /**
   * Set by the loader from the directory the scenario was read from. Absent for
   * a portable scenario, which runs under whichever backend the lane selects.
   */
  sourceBackend?: ScenarioBackendId;
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
  /**
   * Checks the backend can make about the run that the screen cannot show, run
   * after the scenario's own predicates.
   */
  selfChecks?(): Promise<VerifyResult[]>;
  cleanup(): Promise<void>;
}

export interface ScenarioBackend {
  id: ScenarioBackendId;
  engine: Engine;
  launch(scenario: Scenario, opts: RunOptions): Promise<TestHarness>;
}

export interface RunOptions {
  /** The lane's backend: what a portable scenario runs under. */
  backend: ScenarioBackend;
  /** Supplies a backend a scenario's directory asks for by name. */
  resolveBackend?: (id: ScenarioBackendId) => ScenarioBackend;
  /** Runs only the scenarios belonging to the lane's backend. */
  laneOnly?: boolean;
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
  /**
   * Every backend that ran, joined with `+`. A run is only single-valued when
   * one lane was asked for, so consumers must not read this as one backend.
   */
  backendId: string;
  /** Every engine that ran, joined the same way. */
  engine: string;
  startedAt: number;
  completedAt: number;
  total: number;
  passed: number;
  failed: number;
  skipped: number;
  skippedScenarioIds?: string[];
  results: ScenarioResult[];
}
