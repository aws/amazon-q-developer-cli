import { describe, expect, it } from 'bun:test';

import {
  SOURCE_PROVIDER_SETUP_URL,
  formatMissingSourceProviderGuidance,
} from '../cloud-urls';

describe('formatMissingSourceProviderGuidance', () => {
  it('uses the setup URL KAS supplied', () => {
    const msg = formatMissingSourceProviderGuidance(
      'https://app.kiro.dev/connect/gh'
    );
    expect(msg).toContain('https://app.kiro.dev/connect/gh');
    expect(msg).toContain('connected source provider');
  });

  it('falls back to the settings page when no setup URL is offered', () => {
    for (const empty of [undefined, null, '']) {
      expect(formatMissingSourceProviderGuidance(empty)).toContain(
        SOURCE_PROVIDER_SETUP_URL
      );
    }
  });
});
