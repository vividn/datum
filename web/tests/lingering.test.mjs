// A file whose process outlives its tests says what holds it (da-0co61).
//
// CI cut layouts.test.mjs at the runner's 300 s with every subtest passed
// and no word on what kept the process running. `stopApp` now ends such a
// file after the suite's one tier, red, naming what is open. This is a file
// that leaves a timer running, with the tier cut to a second so the proof
// costs one app start: without the stop it would exit clean ten seconds on.

import test from 'node:test';
import assert from 'node:assert/strict';
import { spawn } from 'node:child_process';
import { fileURLToPath } from 'node:url';

const LEAKY = `
	import { startApp, stopApp } from './tests/support/app.mjs';
	await startApp();
	setTimeout(() => {}, 10_000);
	await stopApp();
`;

test('a process still running after stopApp is ended, red, and says what holds it', async () => {
	const child = spawn(process.execPath, ['--input-type=module', '-e', LEAKY], {
		cwd: fileURLToPath(new URL('..', import.meta.url)),
		env: { ...process.env, DATUM_TEST_LINGER_MS: '1000' }
	});
	let stderr = '';
	child.stderr.on('data', (d) => (stderr += d));
	const code = await new Promise((resolve) => child.on('close', resolve));
	assert.match(stderr, /still running 1 s after the browser and server closed, held by .*Timeout/);
	assert.equal(code, 1);
});
