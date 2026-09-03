// The assertion that matters most: web output vs CLI output, same argv.
//
// Every other test in this directory is a proxy for this one. datum.day's
// claim is not that it resembles datum — it is that it IS datum, the same
// program with a different front end. The only way to check that claim is
// to run a command through both and compare what each one stored.
//
// The CLI's half is captured, not written by hand: tests/fixtures/
// cli-parity.json holds what the real binary did with each argv, refreshed
// by `npm run test:fixtures`. The browser's half runs here, through the
// `run(argv)` seam. What is compared is the entry's semantics — field,
// verb, state, duration, data, time — and nothing per-write; see
// support/semantics.mjs for why.
//
// A case with a `divergence` note is one the browser gets wrong today. It
// still runs, reported as a todo rather than a failure, because the
// interim TypeScript runner is being replaced wholesale by the real CLI
// compiled to wasm (da-1vat) and fixing it there would be an investment in
// code with no future. These cases are the acceptance criteria for that
// swap: when it lands they should pass, and the note comes out.

import test, { before, after, describe } from 'node:test';
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { fileURLToPath } from 'node:url';
import { startApp, stopApp, withApp } from './support/app.mjs';
import { comparable, fromEntry } from './support/semantics.mjs';

const fixture = JSON.parse(
	readFileSync(fileURLToPath(new URL('./fixtures/cli-parity.json', import.meta.url)), 'utf8')
);

before(startApp);
after(stopApp);

describe('the same argv stores the same entry as the CLI', () => {
	test('the fixture has been captured from a real binary', () => {
		assert.ok(fixture.cases.length > 0);
		assert.ok(fixture.capturedWith, 'run `npm run test:fixtures` to capture');
		for (const testCase of fixture.cases) assert.ok(testCase.cli, `no capture for ${testCase.argv}`);
	});

	for (const testCase of fixture.cases) {
		const name = 'datum ' + testCase.argv.join(' ');
		test(name, { todo: testCase.divergence ?? undefined }, async () => {
			await withApp(async ({ run, entries }) => {
				const result = await run(testCase.argv);
				assert.equal(
					result.code === 0,
					testCase.cli.code === 0,
					`the CLI exited ${testCase.cli.code}, the browser ${result.code}: ${result.stderr}`
				);
				if (testCase.cli.code !== 0) return;

				const stored = await entries();
				assert.equal(stored.length, 1, 'expected exactly one entry');
				assert.deepEqual(comparable(fromEntry(stored[0]), testCase.time), testCase.cli.entry);
			});
		});
	}
});
