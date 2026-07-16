import { describe, expect, it } from 'bun:test';

import { cloudConnectStage } from '../cloud-connect-stage';

describe('cloudConnectStage', () => {
  it('treats a failed agent_connect as terminal — no spinner even while unchecked', () => {
    // The regression: a rejected connect leaves cloudProviderChecked false, so
    // an unchecked==spinner rule alone would spin forever next to the error.
    expect(cloudConnectStage(false, 'failed')).toBe('failed');
    expect(cloudConnectStage(true, 'failed')).toBe('failed');
  });

  it('shows the spinner while unchecked and not failed', () => {
    expect(cloudConnectStage(false, undefined)).toBe('connecting');
    expect(cloudConnectStage(false, 'loading')).toBe('connecting');
    expect(cloudConnectStage(false, 'ready')).toBe('connecting');
  });

  it('yields to the welcome/checklist once the probe has resolved', () => {
    expect(cloudConnectStage(true, 'ready')).toBe('checked');
    expect(cloudConnectStage(true, undefined)).toBe('checked');
  });
});
