import { describe, it, expect } from 'bun:test';
import {
  isRemoteSession,
  isWorkflowSession,
  SessionLifecycleOwner,
} from '../multi-session';

describe('isRemoteSession', () => {
  it('is false for undefined / null', () => {
    expect(isRemoteSession(undefined)).toBe(false);
    expect(isRemoteSession(null)).toBe(false);
  });

  it('is false when no executionTarget is set (local default)', () => {
    expect(isRemoteSession({})).toBe(false);
    expect(isRemoteSession({ executionTarget: undefined })).toBe(false);
  });

  it('is false for an explicit local execution target', () => {
    expect(isRemoteSession({ executionTarget: { kind: 'local' } })).toBe(false);
  });

  it('is true for cloud-sandbox', () => {
    expect(
      isRemoteSession({ executionTarget: { kind: 'cloud-sandbox' } })
    ).toBe(true);
  });

  it('is true for remote-control', () => {
    expect(
      isRemoteSession({ executionTarget: { kind: 'remote-control' } })
    ).toBe(true);
  });
});

describe('isWorkflowSession', () => {
  it('requires explicit workflow lifecycle ownership', () => {
    expect(isWorkflowSession(undefined)).toBe(false);
    expect(isWorkflowSession(null)).toBe(false);
    expect(isWorkflowSession({})).toBe(false);
  });

  it('recognizes sessions owned by the workflow extension', () => {
    expect(
      isWorkflowSession({
        lifecycleOwner: SessionLifecycleOwner.WorkflowExtension,
      })
    ).toBe(true);
  });
});
