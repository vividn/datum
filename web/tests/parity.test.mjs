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
// were describing. The browser now runs datum itself (`datum-cli` built
// for wasm32-wasip1, da-g8o8), so a divergence is a bug in one shared
// implementation rather than drift between two, and there is nothing left
// for an annotation to excuse.

import test, { before, after, describe } from 'node:test';
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { fileURLToPath } from 'node:url';
import { sharedApp, startApp, stopApp } from './support/app.mjs';
import { comparable, fromBackupRow } from './support/semantics.mjs';

const fixture = JSON.parse(
	readFileSync(fileURLToPath(new URL('./fixtures/cli-parity.json', import.meta.url)), 'utf8')
);

before(startApp);
after(stopApp);

// On the repl layout, not the day page. A case here asks the seam one
// question and reads the row back; it never looks at the screen. Opening
// the day frame for it draws a fortnight of dayview bands first — 1.2 s a
// case, and there are two dozen (da-x6ea).
//
// ONE app for the file, and a DATABASE of its own for each case (da-8c2uc):
// a context each cost 2.5 s a case, and the isolation these need is one
// argv, one entry — which a database gives. A case's setup writes nothing
// outside it: an alias and a spec are entries in the database they were set
// in, and the config a command writes (the database registry, the client
// id) no case reads. The clock stays where the app opened: nothing here
// moves it.
const shared = sharedApp({ query: 'repl' });

describe('the same argv stores the same entry as the CLI', () => {
	test('the fixture has been captured from a real binary', () => {
		assert.ok(fixture.cases.length > 0);
		assert.ok(fixture.capturedWith, 'run `npm run test:fixtures` to capture');
		for (const testCase of fixture.cases) assert.ok(testCase.cli, `no capture for ${testCase.argv}`);
	});

	fixture.cases.forEach((testCase, n) => {
		const name = 'datum ' + testCase.argv.join(' ');
		test(name, { todo: testCase.divergence ?? undefined }, async () => {
			const { run, entries } = await shared();
			const database = `parity-${n}`;
			const db = ['--db', database];
			// A case's `setup` runs through the same seam the case does,
			// so the world the browser logs into is one the browser built
			// — `alias set` in the REPL, not a row written behind it.
			for (const argv of testCase.setup ?? []) {
				const prepared = await run([...db, ...argv]);
				assert.equal(prepared.code, 0, `setup \`datum ${argv.join(' ')}\`: ${prepared.stderr}`);
			}
			const result = await run([...db, ...testCase.argv]);
			assert.equal(
				result.code === 0,
				testCase.cli.code === 0,
				`the CLI exited ${testCase.cli.code}, the browser ${result.code}: ${result.stderr}`
			);
			if (testCase.cli.code !== 0) return;

			// `entries` drops the TRACK document a field's first log
			// declares it with (da-c8mh): the same row on both sides, and
			// this case is about what its own argv stores.
			const stored = await entries(50, database);
			assert.equal(stored.length, 1, 'expected exactly one entry');
			// The raw input is the argv as typed, and the native CLI keeps a
			// `--db` in it too: what the browser ran had one more flag.
			const expected = { ...testCase.cli.entry, rawInput: `${db.join(' ')} ${testCase.cli.entry.rawInput}` };
			assert.deepEqual(comparable(fromBackupRow(stored[0]), testCase.time), expected);
		});
	});
});
