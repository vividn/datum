// The web's stdout against the native CLI's, command by command.
//
// datum.day runs the CLI itself (da-g8o8), so for every command a person
// can type the browser should print what the binary prints — colour
// escapes, table padding, SVG and all. tests/fixtures/cli-stdout.json holds
// what the native binary printed for a script run in order against one
// database, captured by `pnpm run test:fixtures`; this runs the same script
// through the seam and compares, after masking only what a write mints
// (tests/support/stdout.mjs). This is the assertion that keeps the web on
// the CLI's bytes: a runner that drifted from the binary — a different
// zone, a lost flag, a mangled argument — fails here by name.

import test, { before, after, describe } from 'node:test';
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { fileURLToPath } from 'node:url';
import { startApp, stopApp, withApp, todayAt } from './support/app.mjs';
import { masker } from './support/stdout.mjs';

const fixture = JSON.parse(
	readFileSync(fileURLToPath(new URL('./fixtures/cli-stdout.json', import.meta.url)), 'utf8')
);

before(startApp);
after(stopApp);

describe('the web prints what the CLI prints', () => {
	test('the fixture has been captured from a real binary, for the whole script', () => {
		assert.ok(fixture.capturedWith, 'run `npm run test:fixtures` to capture');
		assert.deepEqual(
			fixture.cases.map((c) => c.argv),
			fixture.script
		);
	});

	test('every command of the script, byte for byte', async () => {
		await withApp(
			async ({ run }) => {
				const mask = masker();
				for (const { argv, code, stdout } of fixture.cases) {
					const result = await run(argv);
					const label = `datum ${argv.join(' ')}`;
					assert.equal(result.code, code, `${label}: ${result.stderr}`);
					assert.equal(mask(result.stdout), stdout, label);
				}
			},
			{
				// Mid-afternoon. The script's entries are dated already
				// (support/stdout.mjs's PINNED_DAY), but nowview prints the
				// app's own clock into the drawing and sizes the text to it,
				// so an unpinned hour puts the runner's clock into a
				// byte-for-byte comparison (da-ldct).
				clock: todayAt(15),
				// The device id the native half ran under.
				prepare: (context) =>
					context.addInitScript(() => localStorage.setItem('datum-device-id', 'web-parity'))
			}
		);
	});
});
