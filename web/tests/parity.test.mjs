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
// Every case is a hard assertion. There used to be a `divergence` escape
// hatch here — a note saying "the browser gets this one wrong, report it as
// a todo" — and it accumulated: `-q` off by a factor of three, `-y` read as
// minutes, `-y` stored as a state. da-7scm removed the thing those notes
// were describing. The browser now runs datum's own parser
// (`crates/datum-parse-wasm`), so a divergence is a bug in one shared
// implementation rather than drift between two, and there is nothing left
// for an annotation to excuse.

import test, { before, after, describe } from 'node:test';
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { fileURLToPath } from 'node:url';
import { startApp, stopApp, withApp } from './support/app.mjs';
import { comparable, fromBackupRow } from './support/semantics.mjs';

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
			// On the repl layout, not the day page. A case here asks the seam
			// one question and reads the row back; it never looks at the
			// screen. Opening the day frame for it draws a fortnight of
			// dayview bands first — 1.2 s a case, and there are two dozen
			// (da-x6ea). A context each is still a database each, which is
			// the isolation these need: one argv, one entry.
			await withApp(async ({ run, entries }) => {
				// A case's `setup` runs through the same seam the case does,
				// so the world the browser logs into is one the browser built
				// — `alias set` in the REPL, not a row written behind it.
				for (const argv of testCase.setup ?? []) {
					const prepared = await run(argv);
					assert.equal(prepared.code, 0, `setup \`datum ${argv.join(' ')}\`: ${prepared.stderr}`);
				}
				const result = await run(testCase.argv);
				assert.equal(
					result.code === 0,
					testCase.cli.code === 0,
					`the CLI exited ${testCase.cli.code}, the browser ${result.code}: ${result.stderr}`
				);
				if (testCase.cli.code !== 0) return;

				// `entries` drops the TRACK document a field's first log
				// declares it with (da-c8mh): the same row on both sides,
				// and this case is about what its own argv stores.
				const stored = await entries();
				assert.equal(stored.length, 1, 'expected exactly one entry');
				assert.deepEqual(comparable(fromBackupRow(stored[0]), testCase.time), testCase.cli.entry);
			}, { query: 'repl' });
		});
	}
});
