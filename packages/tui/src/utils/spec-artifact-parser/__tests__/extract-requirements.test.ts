import { describe, it, expect } from 'bun:test';
import { extractRequirements } from '../extract-requirements';

describe('extractRequirements', () => {
  it('returns empty for empty input', () => {
    expect(extractRequirements('')).toEqual([]);
  });

  it('returns empty for whitespace-only input', () => {
    expect(extractRequirements('   \n\t\n  ')).toEqual([]);
  });

  it('returns empty when no requirement headings exist', () => {
    const md = `# Requirements\n\nLorem ipsum body.\n## Some H2\nMore text.`;
    expect(extractRequirements(md)).toEqual([]);
  });

  it('parses a single requirement with title and user story', () => {
    const md = [
      '# Requirements',
      '',
      '### Requirement 1: User authentication',
      '**User Story:** As a user, I want to log in.',
      '',
      '#### Acceptance Criteria',
      '- The system MUST authenticate via OAuth.',
    ].join('\n');
    const items = extractRequirements(md);
    expect(items).toHaveLength(1);
    expect(items[0]!.number).toBe(1);
    expect(items[0]!.title).toBe('User authentication');
    expect(items[0]!.userStory).toBe(
      '**User Story:** As a user, I want to log in.'
    );
    expect(items[0]!.detailBody).toContain('Acceptance Criteria');
  });

  it('parses multiple requirements in source order', () => {
    const md = [
      '### Requirement 1: Alpha',
      '**User Story:** us-a',
      '',
      '### Requirement 2: Bravo',
      '**User Story:** us-b',
      '',
      '### Requirement 3: Charlie',
      '**User Story:** us-c',
    ].join('\n');
    const items = extractRequirements(md);
    expect(items.map((i) => i.number)).toEqual([1, 2, 3]);
    expect(items.map((i) => i.title)).toEqual(['Alpha', 'Bravo', 'Charlie']);
  });

  it('returns null user story when missing', () => {
    const md = [
      '### Requirement 1: No user story here',
      '',
      'Some body without the user story line.',
    ].join('\n');
    const items = extractRequirements(md);
    expect(items[0]!.userStory).toBeNull();
  });

  it('captures the user story verbatim with leading whitespace', () => {
    const md = [
      '### Requirement 1: Whitespace',
      '   **User Story:** indented user story',
    ].join('\n');
    const items = extractRequirements(md);
    expect(items[0]!.userStory).toBe('   **User Story:** indented user story');
  });

  it('uses the first user-story line when multiple appear', () => {
    const md = [
      '### Requirement 1: First wins',
      '**User Story:** first',
      '**User Story:** second',
    ].join('\n');
    const items = extractRequirements(md);
    expect(items[0]!.userStory).toBe('**User Story:** first');
  });

  it('does not match `### Requirements` plural heading', () => {
    const md = '### Requirements summary\n**User Story:** ignored';
    expect(extractRequirements(md)).toEqual([]);
  });

  it('detail body excludes content from the next requirement', () => {
    const md = [
      '### Requirement 1: One',
      '**User Story:** u1',
      'criterion-1',
      '',
      '### Requirement 2: Two',
      '**User Story:** u2',
      'criterion-2',
    ].join('\n');
    const items = extractRequirements(md);
    expect(items[0]!.detailBody).toContain('criterion-1');
    expect(items[0]!.detailBody).not.toContain('criterion-2');
    expect(items[0]!.detailBody).not.toContain('Requirement 2');
    expect(items[1]!.detailBody).toContain('criterion-2');
  });

  it('is deterministic over repeated calls', () => {
    const md = [
      '### Requirement 1: Alpha',
      '**User Story:** us-a',
      '### Requirement 2: Bravo',
      '**User Story:** us-b',
    ].join('\n');
    expect(JSON.stringify(extractRequirements(md))).toBe(
      JSON.stringify(extractRequirements(md))
    );
  });

  it('does not throw on null bytes or non-UTF8 substitution chars', () => {
    const md = `### Requirement 1: \uFFFD title with replacement\n**User Story:** \u0000 null body`;
    expect(() => extractRequirements(md)).not.toThrow();
    const items = extractRequirements(md);
    expect(items).toHaveLength(1);
  });

  it('handles trailing content after the last requirement', () => {
    const md = [
      '### Requirement 1: Only',
      '**User Story:** us',
      'tail line 1',
      'tail line 2',
    ].join('\n');
    const items = extractRequirements(md);
    expect(items[0]!.detailBody).toContain('tail line 1');
    expect(items[0]!.detailBody).toContain('tail line 2');
  });
});
