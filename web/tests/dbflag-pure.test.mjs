// Which database an argv names with `--db`, in node (da-scrml).
//
// `dbFlag` is the one scanner run.ts and the worker's cli.ts read. The cases
// are not written here: `tests/fixtures/db-flag.json` at the repo root is
// read by `crates/datum-commands/tests/db_flag_fixture.rs` against the CLI's
// own pre-scan too, so the two cannot drift apart unseen.

import test from 'node:test';
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { fileURLToPath } from 'node:url';
import { resolveLibAsSource } from './support/libhook.mjs';

resolveLibAsSource(import.meta.url);
const { dbFlag } = await import('../src/lib/dbFlag.ts');
const { dbEnv, dbOf } = await import('../src/lib/run.ts');

const fixture = JSON.parse(
	readFileSync(fileURLToPath(new URL('../../tests/fixtures/db-flag.json', import.meta.url)), 'utf8')
);

test('dbFlag names the database the CLI does, case for case', () => {
	assert.ok(fixture.cases.length > 0);
	for (const { name, argv, db } of fixture.cases) assert.equal(dbFlag(argv)?.db ?? null, db, name);
});

test('the seam: -D is no database flag, so the page still names its own', () => {
	assert.deepEqual(dbEnv(['occur', 'coffee', '-D'], 'other'), ['DATUM_DB=other']);
	assert.equal(dbOf(['occur', 'coffee', '-D'], 'other'), 'other');
	assert.deepEqual(dbEnv(['--db', 'work', 'occur', 'coffee'], 'other'), []);
	assert.equal(dbOf(['--db=work', 'tail'], 'other'), 'work');
});

test('a --db with nothing after it names none, but is still the caller\'s flag', () => {
	assert.deepEqual(dbFlag(['tail', '--db']), {});
	assert.deepEqual(dbEnv(['tail', '--db'], 'other'), []);
	assert.equal(dbOf(['tail', '--db'], 'other'), 'other');
});
