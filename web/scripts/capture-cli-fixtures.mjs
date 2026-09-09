#!/usr/bin/env node
// Capture what the real `datum` CLI does, into tests/fixtures/.
//
// Two fixtures, one binary:
//
//   cli-parity.json  what the CLI STORES for a given argv, one entry per case
//   cli-tail.json    what the CLI PRINTS for `datum tail` over a logged day
//
// The parity suite's whole claim is that datum.day IS datum rather than a
// lookalike, and that claim can only be checked against the CLI's actual
// output. So the fixtures are captured, never hand-written: each case runs
// against a throwaway database, the entry is read back through `datum db
// backup` (the CLI's own JSONL dump), and the comparable semantics are
// recorded.
//
// The authored half of each case — `argv` and `time` — is read from the
// existing file and written back untouched, so re-capturing refreshes the
// CLI's answers without rewriting the cases.
//
// Usage:  npm run test:fixtures        [DATUM_BIN=/path/to/datum]

import { execFileSync } from 'node:child_process';
import { mkdtempSync, readFileSync, writeFileSync, rmSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { fileURLToPath } from 'node:url';
import { comparable, fromBackupRow } from '../tests/support/semantics.mjs';
import { comparableTail } from '../tests/support/tail.mjs';

const FIXTURE = fileURLToPath(new URL('../tests/fixtures/cli-parity.json', import.meta.url));
const TAIL_FIXTURE = fileURLToPath(new URL('../tests/fixtures/cli-tail.json', import.meta.url));
const repo = fileURLToPath(new URL('../..', import.meta.url));

function datumBinary() {
	if (process.env.DATUM_BIN) return process.env.DATUM_BIN;
	for (const profile of ['release', 'debug']) {
		const candidate = join(repo, 'target', profile, 'datum');
		try {
			execFileSync(candidate, ['--version'], { stdio: 'ignore' });
			return candidate;
		} catch {
			// Try the next profile.
		}
	}
	throw new Error('no `datum` binary found — run `cargo build -p datum-cli` or set DATUM_BIN');
}

const datum = datumBinary();

/** Run the CLI against `db`, in UTC so a captured clock time is portable. */
function cli(db, argv) {
	try {
		const stdout = execFileSync(datum, argv, {
			env: { ...process.env, DATUM_DB: db, TZ: 'UTC', NO_COLOR: '1' },
			encoding: 'utf8',
			stdio: ['ignore', 'pipe', 'pipe']
		});
		return { code: 0, stdout };
	} catch (e) {
		return { code: e.status ?? 1, stdout: e.stdout ?? '' };
	}
}

/** The single entry a fresh database holds, via the CLI's own JSONL dump. */
function onlyEntry(db, dir) {
	const path = join(dir, 'backup.jsonl');
	cli(db, ['db', 'backup', path, '--overwrite']);
	const lines = readFileSync(path, 'utf8').trim().split('\n').slice(1);
	if (lines.length !== 1) throw new Error(`expected exactly one entry, got ${lines.length}`);
	return JSON.parse(lines[0]);
}

/** Tab-indented JSON, with argv and comment arrays kept on one line. */
function format(value) {
	return JSON.stringify(value, null, '\t').replace(/\[\n\t+("(?:[^"\\]|\\.)*"(?:,\n\t+"(?:[^"\\]|\\.)*")*)\n\t+\]/g, (_, body) =>
		'[' + body.replace(/\n\t+/g, ' ') + ']'
	) + '\n';
}

const fixture = JSON.parse(readFileSync(FIXTURE, 'utf8'));
// CI re-captures and fails on any diff, so an empty case list would be a
// green check that compared nothing. A gate that finds nothing must not pass.
if (!Array.isArray(fixture.cases) || fixture.cases.length === 0)
	throw new Error(`${FIXTURE} has no cases to capture`);
const dir = mkdtempSync(join(tmpdir(), 'datum-parity-'));
try {
	fixture.cases.forEach((testCase, i) => {
		// A database per case, so `onlyEntry` is unambiguous and no case can
		// collide with another on the UNIQUE slug.
		const db = join(dir, `${i}.db`);
		const { code } = cli(db, testCase.argv);
		testCase.cli =
			code === 0
				? { code, entry: comparable(fromBackupRow(onlyEntry(db, dir)), testCase.time) }
				: { code, entry: null };
	});
	fixture.capturedWith = cli(join(dir, 'version.db'), ['--version']).stdout.trim() || null;
	writeFileSync(FIXTURE, format(fixture));
	console.log(`captured ${fixture.cases.length} cases → ${FIXTURE}`);

	// The tail table: one database, the whole `log` sequence, then the
	// command. `datum tail` piped (which is what `execFileSync` gives it)
	// takes the same defaults the browser does — ten rows, no width budget
	// — so the two tables are comparable character for character.
	const tail = JSON.parse(readFileSync(TAIL_FIXTURE, 'utf8'));
	if (!Array.isArray(tail.log) || tail.log.length === 0)
		throw new Error(`${TAIL_FIXTURE} has nothing to log`);
	const tailDb = join(dir, 'tail.db');
	for (const argv of tail.log) {
		const { code } = cli(tailDb, argv);
		if (code !== 0) throw new Error(`\`datum ${argv.join(' ')}\` exited ${code}`);
	}
	const printed = cli(tailDb, ['tail']);
	if (printed.code !== 0) throw new Error(`\`datum tail\` exited ${printed.code}`);
	tail.table = comparableTail(printed.stdout);
	tail.capturedWith = fixture.capturedWith;
	writeFileSync(TAIL_FIXTURE, format(tail));
	console.log(`captured the tail of ${tail.log.length} entries → ${TAIL_FIXTURE}`);
} finally {
	rmSync(dir, { recursive: true, force: true });
}
