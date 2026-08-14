import { afterEach, beforeEach, describe, expect, it } from 'bun:test';
import { Feature, features } from '../features';

const ORIGINAL_FEATURES = process.env.KIRO_ENABLED_FEATURES;
const ORIGINAL_INTERNAL = process.env.KIRO_INTERNAL;
const ORIGINAL_VOICE_SUPPORTED = process.env.KIRO_VOICE_SUPPORTED;
const ORIGINAL_VOICE_SERVER_URL = process.env.KIRO_VOICE_SERVER_URL;

function setEnv(enabledFeatures: string | undefined, internal?: string) {
  if (enabledFeatures === undefined) delete process.env.KIRO_ENABLED_FEATURES;
  else process.env.KIRO_ENABLED_FEATURES = enabledFeatures;
  if (internal === undefined) delete process.env.KIRO_INTERNAL;
  else process.env.KIRO_INTERNAL = internal;
  features._resetForTests();
}

describe('FeatureManager', () => {
  beforeEach(() => {
    delete process.env.KIRO_VOICE_SUPPORTED;
    delete process.env.KIRO_VOICE_SERVER_URL;
    setEnv(undefined);
  });

  afterEach(() => {
    setEnv(ORIGINAL_FEATURES, ORIGINAL_INTERNAL);
    if (ORIGINAL_VOICE_SUPPORTED === undefined) {
      delete process.env.KIRO_VOICE_SUPPORTED;
    } else {
      process.env.KIRO_VOICE_SUPPORTED = ORIGINAL_VOICE_SUPPORTED;
    }
    if (ORIGINAL_VOICE_SERVER_URL === undefined) {
      delete process.env.KIRO_VOICE_SERVER_URL;
    } else {
      process.env.KIRO_VOICE_SERVER_URL = ORIGINAL_VOICE_SERVER_URL;
    }
  });

  it('enables features listed in KIRO_ENABLED_FEATURES', () => {
    setEnv('["voice","memory","workflows"]');
    expect(features.isEnabled(Feature.Voice)).toBe(true);
    expect(features.isEnabled(Feature.Memory)).toBe(true);
    expect(features.isEnabled(Feature.Workflows)).toBe(true);
    expect(features.isEnabled(Feature.Lite)).toBe(false);
  });

  it('treats missing env as all features off', () => {
    expect(features.isEnabled(Feature.Voice)).toBe(false);
  });

  it.each(['not json', '{"voice":true}', '[1,2,3]', ''])(
    'treats malformed env %j as all features off',
    (raw) => {
      setEnv(raw);
      expect(features.isEnabled(Feature.Voice)).toBe(false);
    }
  );

  it('ignores unknown feature names without failing', () => {
    setEnv('["voice","some_future_feature"]');
    expect(features.isEnabled(Feature.Voice)).toBe(true);
  });

  it('keeps voice available when direct development launches omit the support marker', () => {
    setEnv('["voice"]');
    expect(features.isEnabled(Feature.Voice)).toBe(true);
  });

  it('requires local or remote voice support for the voice rollout', () => {
    setEnv('["voice"]');
    process.env.KIRO_VOICE_SUPPORTED = '0';
    expect(features.isEnabled(Feature.Voice)).toBe(false);

    process.env.KIRO_VOICE_SERVER_URL = 'http://127.0.0.1:19876';
    expect(features.isEnabled(Feature.Voice)).toBe(true);

    delete process.env.KIRO_VOICE_SERVER_URL;
    process.env.KIRO_VOICE_SUPPORTED = '1';
    expect(features.isEnabled(Feature.Voice)).toBe(true);
  });

  it('reports internal user from KIRO_INTERNAL', () => {
    setEnv('[]', '1');
    expect(features.isInternalUser).toBe(true);
    setEnv('[]');
    expect(features.isInternalUser).toBe(false);
  });
});
