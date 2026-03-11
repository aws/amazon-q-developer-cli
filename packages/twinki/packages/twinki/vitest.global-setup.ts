import { rmSync } from 'node:fs';
import { join } from 'node:path';

export function setup() {
	const dir = join(import.meta.dirname, 'test', '.artifacts');
	try { rmSync(dir, { recursive: true, force: true }); } catch {}
}
