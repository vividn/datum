// The browser tokenizes a typed line exactly the way the terminal does.
//
// `shellSplit` claims to mirror `datum_parse::shell_split`, and the claim
// was false at two codepoints: U+0085 NEL split the line for the CLI and
// not for the browser, U+FEFF split it for the browser and not for the CLI
// (da-rx63). Neither raised an error. A pasted line — and a byte-order mark
// arrives by paste — simply became different argv on the two clients.
//
// The cases are not written here. They are read from
// `tests/fixtures/shell-split.json` at the repo root, which
// `crates/datum-parse/tests/shell_split_fixture.rs` also reads: two
// hand-written test files drift the same way two implementations do.
//
// The assertion goes through `runLine`, the seam the CLI widget itself uses, and
// reads the argv off the result — so what is checked is the tokenization
// the app actually runs on, not a copy of the module imported beside it.

import test, { before, after, describe } from 'node:test';
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { fileURLToPath } from 'node:url';
import { startApp, stopApp, withApp } from './support/app.mjs';

const fixture = JSON.parse(
	readFileSync(
		fileURLToPath(new URL('../../tests/fixtures/shell-split.json', import.meta.url)),
		'utf8'
	)
);

before(startApp);
after(stopApp);

describe('a typed line splits into the same argv the CLI gets', () => {
	test('the shared fixture still covers the two divergent codepoints', () => {
		assert.ok(fixture.cases.length > 0);
		// Written by code point: a literal invisible character in a source file
		// is one the next reader cannot see.
		for (const code of [0x85, 0xfeff]) {
			const c = String.fromCharCode(code);
			assert.ok(
				fixture.cases.some((testCase) => testCase.line.includes(c)),
				`no case exercises U+${code.toString(16).toUpperCase().padStart(4, '0')}`
			);
		}
	});

	test('every case', async () => {
		await withApp(async ({ runLine }) => {
			for (const testCase of fixture.cases) {
				const result = await runLine(testCase.line);
				assert.deepEqual(result.argv, testCase.argv, testCase.name);
			}
		});
	});
});
