import { describe, it, expect } from 'vitest';
import React, { useState, useEffect } from 'react';
import pkg from '@xterm/headless';
const { Terminal: XtermTerminal } = pkg;
import { render } from '../src/reconciler/render.js';
import { Text } from '../src/components/Text.js';
import { Box } from '../src/components/Box.js';
import type { Terminal } from '../src/terminal/terminal.js';

/**
 * Tests for overflow-wrap rendering (KIRO_DISABLE_WRAP=1 scenario).
 *
 * Uses <Text wrap="overflow">. A logical line wider than the terminal
 * stays as a single array entry; the terminal soft-wraps it visually
 * into multiple physical rows. Twinki's internal state machine must
 * track PHYSICAL rows so that:
 *   - cursor positioning is accurate
 *   - `\x1b[J` clears the correct region
 *   - streaming chunks don't leave duplicates in scrollback
 */

class TestTerminal implements Terminal {
  private xterm: InstanceType<typeof XtermTerminal>;
  private inputHandler?: (data: string) => void;
  private _cols: number;
  private _rows: number;

  constructor(cols = 40, rows = 10) {
    this._cols = cols;
    this._rows = rows;
    this.xterm = new XtermTerminal({
      cols,
      rows,
      scrollback: 1000,
      allowProposedApi: true,
    });
  }
  get kittyProtocolActive() { return true; }
  get columns() { return this._cols; }
  get rows() { return this._rows; }
  start(onInput: (data: string) => void) { this.inputHandler = onInput; }
  stop() {}
  async drainInput() {}
  write(data: string) { this.xterm.write(data); }
  sendInput(data: string) { this.inputHandler?.(data); }
  moveBy(n: number) {
    if (n > 0) this.write(`\x1b[${n}B`);
    else if (n < 0) this.write(`\x1b[${-n}A`);
  }
  hideCursor() { this.write('\x1b[?25l'); }
  showCursor() { this.write('\x1b[?25h'); }
  clearLine() { this.write('\x1b[K'); }
  clearFromCursor() { this.write('\x1b[J'); }
  clearScreen() { this.write('\x1b[2J\x1b[H'); }
  setTitle() {}
  enableMouse() {}
  disableMouse() {}

  async flush() {
    await new Promise<void>(r => this.xterm.write('', r));
  }

  getViewport(): string[] {
    const buf = this.xterm.buffer.active;
    const lines: string[] = [];
    for (let i = 0; i < this._rows; i++) {
      const line = buf.getLine(buf.viewportY + i);
      lines.push(line ? line.translateToString(true) : '');
    }
    return lines;
  }

  getFullBuffer(): string[] {
    const buf = this.xterm.buffer.active;
    const lines: string[] = [];
    const total = buf.baseY + this._rows;
    for (let i = 0; i < total; i++) {
      const line = buf.getLine(i);
      lines.push(line ? line.translateToString(true) : '');
    }
    return lines;
  }
}

async function wait(ms = 15) {
  await new Promise(r => setTimeout(r, ms));
}

/**
 * React component driven by an external updater. Each `update(text)` call
 * sets the component's content, triggering a React re-render + twinki frame.
 */
function makeDrivenComponent() {
  let setter: ((value: string) => void) | null = null;
  const Comp: React.FC = () => {
    const [text, setText] = useState('');
    useEffect(() => {
      setter = setText;
      return () => { setter = null; };
    }, []);
    return (
      <Box flexDirection="column">
        <Text wrap="overflow">{text}</Text>
      </Box>
    );
  };
  return {
    Comp,
    update: (value: string) => {
      if (setter) setter(value);
    },
  };
}

describe('Overflow wrap — React pipeline', () => {
  it('renders short then wide text without duplication', async () => {
    const term = new TestTerminal(20, 8);
    const { Comp, update } = makeDrivenComponent();
    const instance = render(<Comp />, { terminal: term, wideLines: true });
    await wait();
    await term.flush();

    // Stage 1: short line
    update('short line');
    await wait();
    await term.flush();

    let vp = term.getViewport();
    expect(vp[0]).toContain('short line');

    // Stage 2: grow past 20 cols (soft-wraps to 2 rows)
    update('short line continues long text now');
    await wait();
    await term.flush();

    vp = term.getViewport();
    // Row 0 has first 20 cols; row 1 has the overflow
    expect(vp[0]).toContain('short line continues');
    expect(vp[1]).toMatch(/long text now/);

    // No leftover ghost "short line" anywhere in buffer
    const buffer = term.getFullBuffer();
    const ghost = buffer.filter(l => l.trim() === 'short line').length;
    expect(ghost).toBe(0);

    instance.unmount();
  });

  it('streams chunks into a multi-paragraph list that overflows viewport', async () => {
    // Reproduces the user's real bug: streaming N paragraphs, each soft-wrapping
    // into multiple physical rows, where the total content height exceeds the
    // viewport. Intermediate streaming states of an in-place-growing line leave
    // ghost copies of the first soft-wrapped row in terminal scrollback.
    const term = new TestTerminal(70, 27);

    // Driven component that renders a stable prefix + a streaming paragraph.
    // Mimics the TUI's incremental markdown stream: previous paragraphs already
    // rendered, current paragraph grows in place.
    let setStreaming: ((s: string) => void) | null = null;
    let setFinished: ((p: string[]) => void) | null = null;
    const Comp: React.FC = () => {
      const [finished, setFin] = useState<string[]>([]);
      const [streaming, setStr] = useState('');
      useEffect(() => {
        setStreaming = setStr;
        setFinished = setFin;
        return () => {
          setStreaming = null;
          setFinished = null;
        };
      }, []);
      return (
        <Box flexDirection="column">
          {finished.map((line, i) => (
            <Text key={i} wrap="overflow">{line}</Text>
          ))}
          {streaming ? (
            <Text wrap="overflow">{streaming}</Text>
          ) : null}
        </Box>
      );
    };
    const instance = render(<Comp />, { terminal: term, wideLines: true });
    await wait();
    await term.flush();

    // Build enough paragraphs to exceed the 27-row viewport. Each is ~130-170 chars
    // → 2-3 physical rows at 70 cols.
    const paragraphs = [
      'Compute — EC2 virtual servers with configurable CPU, memory, storage, networking, and the ability to scale horizontally to meet variable demand.',
      'Storage — S3 object storage offering 99.999999999% (11 nines) durability for files, static websites, backups, and arbitrary data payloads across regions.',
      'Databases — RDS for managed relational databases and DynamoDB for serverless key-value and document stores with single-digit-ms latency at any scale.',
      'Networking — VPC isolates virtual networks, CloudFront provides a global CDN, and Route 53 offers DNS with health checking and traffic routing policies.',
      'Security — IAM provides users, roles, and policies; KMS offers encryption keys; GuardDuty detects threats using ML anomaly detection and finding management.',
      'DevOps — CodePipeline for continuous delivery, CloudFormation for infrastructure-as-code, and CloudWatch for monitoring, alerting, and dashboards.',
      'AI/ML — SageMaker for model training and deployment, Bedrock for foundation model access, Rekognition for image and video analysis across media workflows.',
      'Messaging — SQS provides managed queues, SNS provides pub/sub notifications, and EventBridge powers event-driven service decoupling with rule-based routing.',
      'Analytics — Athena runs serverless SQL queries on S3 data; Kinesis streams real-time data; Glue is a managed ETL service with a data catalog and crawlers.',
      'Regions — Each region (e.g., us-east-1) contains multiple AZs. Deploying across AZs provides high availability within a region and recovery across regions.',
      'Pricing — Pay-as-you-go, Reserved Instances for 1-3 year commitments, Spot Instances for interruptible workloads, and Savings Plans across compute families.',
      'Developer Tools — AWS CLI, SDKs for every major language, CDK for defining infrastructure in TypeScript and Python, and SAM for serverless application deployment.',
    ];
    setFinished?.(paragraphs);
    await wait();
    await term.flush();

    // Now stream a new paragraph in place, growing it chunk by chunk.
    const target =
      'Shared Responsibility Model — AWS secures the infrastructure (physical, network, hypervisor). You secure what you put on it (data, IAM config, OS patches, application code, encryption).';
    for (let end = 20; end <= target.length; end += 15) {
      setStreaming?.(target.slice(0, end));
      await wait(8);
      await term.flush();
    }
    setStreaming?.(target);
    await wait();
    await term.flush();

    const buffer = term.getFullBuffer();
    console.log('FULL BUFFER:');
    buffer.forEach((line, i) => console.log(`  ${i}: ${JSON.stringify(line)}`));

    // The first soft-wrapped row of the target at 70 cols. The prefix
    // "Shared Responsibility Model — AWS " is a stable prefix of every
    // intermediate streaming state, so counting how many lines START with it
    // tells us how many ghost copies exist.
    const stablePrefix = 'Shared Responsibility Model — AWS';
    const occurrences = buffer.filter((l) => l.startsWith(stablePrefix)).length;
    expect(
      occurrences,
      `first row of the target paragraph should appear exactly once, got ${occurrences}`
    ).toBe(1);

    instance.unmount();
  });

  it('updates a line that grows from 1 to 3 physical rows correctly', async () => {
    const term = new TestTerminal(20, 8);
    const { Comp, update } = makeDrivenComponent();
    const instance = render(<Comp />, { terminal: term, wideLines: true });
    await wait();
    await term.flush();

    update('abc');
    await wait();
    await term.flush();

    update('abc defghij klmnopqr stuv wxyz0123456789 ABCDEFGHIJKL');
    await wait();
    await term.flush();

    const vp = term.getViewport();
    // Expected rows of a 53-char content in a 20-col terminal:
    //   row 0: "abc defghij klmnopqr" (20 cols)
    //   row 1: " stuv wxyz0123456789" (20 cols)
    //   row 2: " ABCDEFGHIJKL" (13 cols)
    expect(vp[0]).toContain('abc defghij klmnopqr');
    expect((vp[0] + vp[1] + vp[2])).toContain('wxyz0123456789');
    expect((vp[0] + vp[1] + vp[2])).toContain('ABCDEFGHIJKL');

    // No ghost of the initial "abc" anywhere
    const buffer = term.getFullBuffer();
    const abcOccurrences = buffer.filter(l => l.trim() === 'abc').length;
    expect(abcOccurrences).toBe(0);

    instance.unmount();
  });
});
