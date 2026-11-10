// A test process's browser profiles and artifacts go with it (da-bvhac).
//
// Playwright makes them under `os.tmpdir()`, and a run that did not close
// them left them in /tmp: 3,062 by 2026-11-10. support/tmpdir.mjs gives each
// process a directory of its own and removes it once the process is gone. So a
// child that imports it and launches a browser, and is then ended each way a
// run's file is, leaves no directory behind.

import test from 'node:test';
import assert from 'node:assert/strict';
import { spawn } from 'node:child_process';
import { existsSync, mkdtempSync, readdirSync, rmSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { until } from './support/until.mjs';

const TMPDIR = new URL('./support/tmpdir.mjs', import.meta.url).href;
const PLAYWRIGHT = import.meta.resolve('playwright');

// It prints its directory once its browser is up, or its launch has failed,
// and waits to be ended. A file's tests fail and it exits once its browser is
// gone, which Playwright's own SIGTERM handler closes without ending the
// process, so this one exits then too.
const RUN = `
await import(${JSON.stringify(TMPDIR)});
const { chromium } = await import(${JSON.stringify(PLAYWRIGHT)});
const browser = await chromium.launch().catch(() => null);
console.log(process.env.TMPDIR);
browser?.on('disconnected', () => process.exit(1));
setInterval(() => {}, 1 << 30);
`;

async function ended(how, env = {}) {
	const run = spawn(process.execPath, ['--input-type=module', '-e', RUN], {
		env: { ...process.env, ...env },
		stdio: ['ignore', 'pipe', 'inherit']
	});
	const gone = () => run.exitCode !== null || run.signalCode !== null;
	try {
		let out = '';
		run.stdout.on('data', (d) => (out += d));
		const dir = (await until(() => out, (o) => o.includes('\n') || gone())).trim();
		assert.ok(dir, 'the child printed its directory');
		const held = readdirSync(dir);
		assert.ok(held.some((f) => f.startsWith('playwright-artifacts-')), `the browser's own were in it: ${held}`);
		run.kill(how);
		await until(gone, Boolean);
		await until(() => existsSync(dir), (there) => !there);
	} finally {
		run.kill('SIGKILL');
	}
}

test('SIGTERM: the directory goes with the process', () => ended('SIGTERM'));
test('SIGINT: the directory goes with the process', () => ended('SIGINT'));
test('SIGKILL, which nothing in it can catch: the directory goes too', () => ended('SIGKILL'));
test('a launch that fails leaves its pair there, and they go with the process', async () => {
	const none = mkdtempSync(join(tmpdir(), 'no-browsers-'));
	try {
		await ended('SIGTERM', { PLAYWRIGHT_BROWSERS_PATH: none });
	} finally {
		rmSync(none, { recursive: true, force: true });
	}
});
