import { describe, it, expect } from 'bun:test';
import { formatCloudFooter } from '../cloud-status';
import { ASCII_GLYPHS, UNICODE_GLYPHS } from '../glyphs';

describe('formatCloudFooter (location indicator)', () => {
  it('renders the bound repo as the sandbox workspace path (~/kiro/<name>)', () => {
    // A cloud session runs in the sandbox, so the footer names the sandbox
    // workspace dir, using the repo's basename (not the owner/name binding).
    expect(formatCloudFooter('acme/banana-service')).toBe(
      'Cloud (Preview) · ~/kiro/banana-service'
    );
  });

  it('uses the repo basename even when no owner prefix is present', () => {
    expect(formatCloudFooter('banana-service')).toBe(
      'Cloud (Preview) · ~/kiro/banana-service'
    );
  });

  it('shows just "Cloud (Preview)" for a New empty sandbox (no repo)', () => {
    expect(formatCloudFooter(null)).toBe('Cloud (Preview)');
    expect(formatCloudFooter(undefined)).toBe('Cloud (Preview)');
    expect(formatCloudFooter('')).toBe('Cloud (Preview)');
  });

  it('trims surrounding whitespace and treats blank as no repo', () => {
    expect(formatCloudFooter('  acme/repo  ')).toBe(
      'Cloud (Preview) · ~/kiro/repo'
    );
    expect(formatCloudFooter('   ')).toBe('Cloud (Preview)');
  });

  it('prefixes the icon when supplied', () => {
    expect(formatCloudFooter('acme/repo', null, '☁')).toBe(
      '☁ Cloud (Preview) · ~/kiro/repo'
    );
    expect(formatCloudFooter(null, null, '☁')).toBe('☁ Cloud (Preview)');
  });

  it('renders no badge when the icon is omitted (allowIcons preference off)', () => {
    // Callers pass `allowIcons ? glyphs.cloud : undefined`, so an undefined icon
    // is the icons-disabled path — the footer must carry no leading badge.
    expect(formatCloudFooter('acme/repo', 'main', undefined)).toBe(
      'Cloud (Preview) · ~/kiro/repo · main'
    );
    expect(formatCloudFooter(null, null, undefined)).toBe('Cloud (Preview)');
  });

  it('appends the branch segment after the repo when known', () => {
    expect(formatCloudFooter('acme/repo', 'main', '☁')).toBe(
      '☁ Cloud (Preview) · ~/kiro/repo · main'
    );
    expect(formatCloudFooter('acme/repo', '  main  ')).toBe(
      'Cloud (Preview) · ~/kiro/repo · main'
    );
  });

  it('drops the branch when there is no repo to anchor it', () => {
    expect(formatCloudFooter(null, 'main')).toBe('Cloud (Preview)');
    expect(formatCloudFooter('', 'main', '☁')).toBe('☁ Cloud (Preview)');
  });

  it('appends "(+N others)" when several repos are bound (mock 19.1)', () => {
    expect(formatCloudFooter('kiro/banana-service', 'main', '☁', 3)).toBe(
      '☁ Cloud (Preview) · ~/kiro/banana-service · main (+3 others)'
    );
  });

  it('uses singular "(+1 other)" when exactly one additional repo is bound', () => {
    expect(formatCloudFooter('kiro/KiroCLIReviewerCDK', 'main', '☁', 1)).toBe(
      '☁ Cloud (Preview) · ~/kiro/KiroCLIReviewerCDK · main (+1 other)'
    );
    // Singular form also without a branch segment.
    expect(formatCloudFooter('kiro/banana-service', null, undefined, 1)).toBe(
      'Cloud (Preview) · ~/kiro/banana-service (+1 other)'
    );
  });

  it('appends the suffix even without a known branch', () => {
    expect(formatCloudFooter('kiro/banana-service', null, '☁', 2)).toBe(
      '☁ Cloud (Preview) · ~/kiro/banana-service (+2 others)'
    );
  });

  it('omits the suffix for zero/negative/undefined others', () => {
    expect(formatCloudFooter('acme/repo', 'main', undefined, 0)).toBe(
      'Cloud (Preview) · ~/kiro/repo · main'
    );
    expect(formatCloudFooter('acme/repo', 'main', undefined, -1)).toBe(
      'Cloud (Preview) · ~/kiro/repo · main'
    );
    expect(formatCloudFooter('acme/repo', 'main', undefined, undefined)).toBe(
      'Cloud (Preview) · ~/kiro/repo · main'
    );
  });

  it('drops the suffix for a New empty sandbox (no repo to anchor it)', () => {
    expect(formatCloudFooter(null, null, '☁', 3)).toBe('☁ Cloud (Preview)');
  });

  it('degrades the segment separators in ASCII mode', () => {
    expect(formatCloudFooter('acme/repo', 'main', '*', 2, ASCII_GLYPHS)).toBe(
      '* Cloud (Preview) . ~/kiro/repo . main (+2 others)'
    );
  });

  it('keeps the middle-dot separator with the default (Unicode) glyphs', () => {
    expect(formatCloudFooter('acme/repo', 'main', '☁', 2, UNICODE_GLYPHS)).toBe(
      '☁ Cloud (Preview) · ~/kiro/repo · main (+2 others)'
    );
  });
});
