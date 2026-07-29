import { describe, it, expect } from 'bun:test';
import {
  cloudPanelNotice,
  cloudPanelEmptyMessage,
} from '../cloud-panel-notice.js';

describe('cloudPanelNotice', () => {
  it('returns undefined for every panel in a local session', () => {
    expect(cloudPanelNotice('mcp', false, 'awaiting-sandbox')).toBeUndefined();
    expect(
      cloudPanelNotice('tools', false, 'awaiting-sandbox')
    ).toBeUndefined();
    expect(cloudPanelNotice('hooks', false)).toBeUndefined();
  });

  it('says /hooks data comes from the sandbox (session-forwarded fetch)', () => {
    expect(cloudPanelNotice('hooks', true)).toBe(
      'Hooks fetched from the cloud sandbox'
    );
  });

  it('shows the awaiting notice while the sandbox snapshot is pending', () => {
    const awaiting =
      'Cloud sandbox configuration not yet received — it will appear when the sandbox reports it.';
    expect(cloudPanelNotice('mcp', true, 'awaiting-sandbox')).toBe(awaiting);
    expect(cloudPanelNotice('tools', true, 'awaiting-sandbox')).toBe(awaiting);
  });

  it('returns undefined once the sandbox snapshot is received', () => {
    expect(cloudPanelNotice('mcp', true, 'received')).toBeUndefined();
    expect(cloudPanelNotice('tools', true, 'received')).toBeUndefined();
  });
});

describe('cloudPanelEmptyMessage', () => {
  it('is authoritative sandbox-empty copy per surface', () => {
    expect(cloudPanelEmptyMessage('mcp')).toBe(
      'The cloud sandbox has no MCP servers configured'
    );
    expect(cloudPanelEmptyMessage('tools')).toBe(
      'The cloud sandbox has no tools available'
    );
  });
});
