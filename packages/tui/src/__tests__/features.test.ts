import { afterEach, beforeEach, describe, expect, it } from 'bun:test';
import { Feature, features } from '../features';

const ORIGINAL_FEATURES = process.env.KIRO_ENABLED_FEATURES;
const ORIGINAL_INTERNAL = process.env.KIRO_INTERNAL;

function setEnv(enabledFeatures: string | undefined, internal?: string) {
  if (enabledFeatures === undefined) delete process.env.KIRO_ENABLED_FEATURES;
  else process.env.KIRO_ENABLED_FEATURES = enabledFeatures;
  if (internal === undefined) delete process.env.KIRO_INTERNAL;
  else process.env.KIRO_INTERNAL = internal;
  features._resetForTests();
}

describe('FeatureManager', () => {
  beforeEach(() => setEnv(undefined));

  afterEach(() => {
    setEnv(ORIGINAL_FEATURES, ORIGINAL_INTERNAL);
  });

  it('enables features listed in KIRO_ENABLED_FEATURES', () => {
    setEnv('["voice","memory"]');
    expect(features.isEnabled(Feature.Voice)).toBe(true);
    expect(features.isEnabled(Feature.Memory)).toBe(true);
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

  it('reports internal user from KIRO_INTERNAL', () => {
    setEnv('[]', '1');
    expect(features.isInternalUser).toBe(true);
    setEnv('[]');
    expect(features.isInternalUser).toBe(false);
  });
});
