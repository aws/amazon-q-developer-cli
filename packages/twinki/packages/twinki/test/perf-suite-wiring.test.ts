/**
 * Perf Suite Wiring — the timing suite still has something to measure
 */
import { describe, it, expect } from 'vitest';
import { readdirSync } from 'node:fs';
import perfConfig from '../../../vitest.perf.config';

// Escape every regex metacharacter first — including the backslash, which a
// partial escape would leave able to neutralise the escapes added after it —
// then reopen only the three glob constructs this config uses.
function globToRegExp(glob: string): RegExp {
	const body = glob
		.replace(/[.*+?^${}()|[\]\\]/g, '\\$&')
		.replace(/\\\{([^}]*)\\\}/g, (_, alts: string) => `(${alts.split(',').join('|')})`)
		.replace(/\\\*\\\*\//g, '(?:.*/)?')
		.replace(/\\\*/g, '[^/]*');
	return new RegExp(`^${body}$`);
}

describe('perf suite wiring', () => {
	// Timing budgets are selected purely by a filename glob, and they run where a
	// failing assertion cannot block a merge. A rename that left the glob matching
	// nothing would therefore end all measurement with every check still green.
	// Asserting the match from the merge-gating suite is what makes that loss loud.
	it('the perf include glob still selects test files', () => {
		// Read the glob rather than restate it, so editing the config cannot leave
		// this test agreeing with a pattern the perf run no longer uses.
		const include = (perfConfig as { test?: { include?: string[] } }).test
			?.include;
		expect(include).toBeDefined();

		const patterns = include!.map(globToRegExp);

		const perfFiles = readdirSync(import.meta.dirname).filter(name =>
			patterns.some(pattern => pattern.test(name))
		);

		expect(perfFiles.length).toBeGreaterThan(0);
	});
});
