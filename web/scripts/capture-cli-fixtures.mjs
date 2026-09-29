#!/usr/bin/env node
// Capture what the real `datum` CLI does, into tests/fixtures/.
//
// Two fixtures, one binary:
//
//   cli-parity.json  what the CLI STORES for a given argv, one entry per case
//   cli-stdout.json  what the CLI PRINTS for each command of a script, in
//                    colour, run in order against one database
//
// The parity suite's whole claim is that datum.day IS datum rather than a
// lookalike, and that claim can only be checked against the CLI's actual
// output. So the fixtures are captured, never hand-written: each case runs
// against a throwaway database, the entry is read back through `datum db
// backup` (the CLI's own JSONL dump), and the comparable semantics are
// recorded.
//
// The authored half of each case — `setup`, `argv` and `time` — is read
// from the existing file and written back untouched, so re-capturing
// refreshes the CLI's answers without rewriting the cases.
//
// `setup` is the commands a case needs run first — `alias set`, `spec set`
// — against its own database, so an aliased argv is compared as one thing
// the CLI did rather than as two separately-authored fixtures (da-5kbj).
//
// Usage:  npm run test:fixtures        [DATUM_BIN=/path/to/datum]

import { execFileSync } from 'node:child_process';
import { mkdtempSync, readFileSync, writeFileSync, rmSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { fileURLToPath } from 'node:url';
import { comparable, fromBackupRow } from '../tests/support/semantics.mjs';
import { masker } from '../tests/support/stdout.mjs';

const FIXTURE = fileURLToPath(new URL('../tests/fixtures/cli-parity.json', import.meta.url));
const STDOUT_FIXTURE = fileURLToPath(new URL('../tests/fixtures/cli-stdout.json', import.meta.url));
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
	return cliWith({ ...process.env, DATUM_DB: db, TZ: 'UTC', NO_COLOR: '1' }, argv);
}

/** The device id the stdout script's browser half runs under too. */
const STDOUT_DEVICE = 'web-parity';

/** Run the CLI with exactly `env`, plus PATH. */
function cliWith(env, argv) {
	try {
		const stdout = execFileSync(datum, argv, {
			env: { PATH: process.env.PATH, ...env },
			encoding: 'utf8',
			stdio: ['ignore', 'pipe', 'pipe']
		});
		return { code: 0, stdout };
	} catch (e) {
		return { code: e.status ?? 1, stdout: e.stdout ?? '' };
	}
}

/**
 * The single entry a fresh database holds, via the CLI's own JSONL dump.
 * The TRACK document a field's first log declares it with (da-c8mh) is not
 * one, nor is the spec entry it writes (da-lc26c) or an alias a case's
 * setup wrote (da-b3752) — the same rows on both sides, and this fixture is
 * about what the case's argv itself stores.
 */
function onlyEntry(db, dir) {
	const path = join(dir, 'backup.jsonl');
	cli(db, ['db', 'backup', path, '--overwrite']);
	const rows = readFileSync(path, 'utf8')
		.trim()
		.split('\n')
		.slice(1)
		.map((line) => JSON.parse(line))
		.filter((row) => row.extra_metadata?.track === undefined && !['spec', 'alias'].includes(row.field));
	if (rows.length !== 1) throw new Error(`expected exactly one entry, got ${rows.length}`);
	return rows[0];
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
		for (const argv of testCase.setup ?? []) {
			const { code } = cli(db, argv);
			if (code !== 0) throw new Error(`setup \`datum ${argv.join(' ')}\` exited ${code}`);
		}
		const { code } = cli(db, testCase.argv);
		testCase.cli =
			code === 0
				? { code, entry: comparable(fromBackupRow(onlyEntry(db, dir)), testCase.time) }
				: { code, entry: null };
	});
	// `datum 3.0.x-beta`: the height (the patch) and the commit change with
	// every commit, and a fixture that did would never compare equal (da-5172a).
	fixture.capturedWith =
		cli(join(dir, 'version.db'), ['--version'])
			.stdout.trim()
			.replace(/^(datum \d+\.\d+)\.\d+(\S*) \(.*\)$/, '$1.x$2') || null;
	writeFileSync(FIXTURE, format(fixture));
	console.log(`captured ${fixture.cases.length} cases → ${FIXTURE}`);

	// The stdout script: one database and one HOME for the whole run, the
	// environment the browser's runner gives every process — the zone, a
	// device id, colour on — and each answer masked where a write mints
	// something no two runs share (tests/support/stdout.mjs).
	const script = JSON.parse(readFileSync(STDOUT_FIXTURE, 'utf8'));
	if (!Array.isArray(script.script) || script.script.length === 0)
		throw new Error(`${STDOUT_FIXTURE} has no script`);
	const home = join(dir, 'home');
	const mask = masker();
	script.cases = script.script.map((argv) => {
		const { code, stdout } = cliWith(
			{ HOME: home, DATUM_DB: join(home, 'datum.db'), DATUM_DEVICE_ID: STDOUT_DEVICE, DATUM_COLOR: '1', TZ: 'UTC' },
			argv
		);
		return { argv, code, stdout: mask(stdout) };
	});
	script.capturedWith = fixture.capturedWith;
	writeFileSync(STDOUT_FIXTURE, format(script));
	console.log(`captured ${script.cases.length} commands' stdout → ${STDOUT_FIXTURE}`);
} finally {
	rmSync(dir, { recursive: true, force: true });
}
