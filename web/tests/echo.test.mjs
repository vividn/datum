// The web asks datum for its action echo as data, and places it (da-xfa4).
//
// Two claims, and the second is the one that rots if nobody watches it.
//
// 1. `echoArgv` appends `--show json` to the commands that have an echo,
//    to EACH SIDE of a multi-side run, and to nothing that already names a
//    `--show`.
// 2. `NO_ECHO` — the commands it leaves alone — says what the CLI does.
//    That list is a DENYLIST because an argv whose first word is not a
//    command (`mood switch happy` field-first, an alias) is a write under
//    another name and must get the flag; the cost of a denylist is that a
//    command added to the CLI and not added here would have the flag
//    appended to it, and most of the CLI's readers do not merely ignore
//    `--show`, they exit 1 on it. So every word in the list is run through
//    the real CLI here and made to prove it belongs, and every verb that
//    does echo is made to prove it still does.
//
// The commands run through the app's own seam, which is the real CLI
// compiled to wasm: this is datum answering, not a table written down.

import test, { before, after, describe } from 'node:test';
import assert from 'node:assert/strict';
import { startApp, stopApp, withApp } from './support/app.mjs';
import { echoArgv, parseEcho } from '../src/lib/echo.ts';

before(startApp);
after(stopApp);

describe('echoArgv', () => {
	test('a write verb is asked for its echo', () => {
		assert.deepEqual(echoArgv(['occur', 'coffee']), ['occur', 'coffee', '--show', 'json']);
	});

	test('a command not in the CLI — field-first, an alias — is asked too', () => {
		// `mood switch happy` and `vbp correr` are both write commands the
		// CLI resolves in its own pre-pass; the web cannot tell them from a
		// verb and must not treat an unknown word as a reader.
		assert.deepEqual(echoArgv(['mood', 'switch', 'happy']).slice(-2), ['--show', 'json']);
		assert.deepEqual(echoArgv(['vbp', 'correr']).slice(-2), ['--show', 'json']);
	});

	test('a reader is left alone', () => {
		for (const argv of [['tail'], ['dayview'], ['fields', '--json'], ['db', 'backup', '-']]) {
			assert.deepEqual(echoArgv(argv), argv, argv.join(' '));
		}
	});

	test('a `--show` the reader named wins, in either spelling', () => {
		assert.deepEqual(echoArgv(['occur', 'x', '--show', 'all']), ['occur', 'x', '--show', 'all']);
		assert.deepEqual(echoArgv(['occur', 'x', '--show=none']), ['occur', 'x', '--show=none']);
	});

	test('a global flag in front does not hide the command', () => {
		assert.deepEqual(echoArgv(['--db', '_internal', 'tail']), ['--db', '_internal', 'tail']);
		assert.deepEqual(echoArgv(['--db', '_internal', 'occur', 'x']).slice(-2), ['--show', 'json']);
	});

	test('EVERY side of a multi-side run is asked, not only the last', () => {
		// `--show` is a per-side flag: one at the end reaches only the last
		// side, which would leave the first printing its lines and the bar
		// with half a result.
		assert.deepEqual(echoArgv(['occur', 'a', ',', 'occur', 'b']), [
			'occur',
			'a',
			'--show',
			'json',
			',',
			'occur',
			'b',
			'--show',
			'json'
		]);
	});
});

describe('parseEcho', () => {
	const doc = {
		action: 'CREATE',
		hid: '35mtf',
		id: 'coffee:2026-03-16T09:15Z',
		header: 'CREATE 35mtf coffee:2026-03-16T09:15Z',
		info: '09:15:00+0  coffee',
		data: [],
		data_inline: ''
	};

	test('the document is read off the last line that parses', () => {
		const out = `a warning on stdout\n${JSON.stringify({ docs: [doc] })}\n`;
		assert.deepEqual(parseEcho(out), [doc]);
	});

	test('no document is null, which is the fallback contract', () => {
		for (const out of ['', 'time  field\n 09:15 coffee\n', '{', '{"docs":[]}', '{"docs":[{}]}']) {
			assert.equal(parseEcho(out), null, JSON.stringify(out));
		}
	});
});

/**
 * The words `echoArgv` leaves alone, with arguments that make each one a
 * command the CLI will actually try to run.
 *
 * `db`, `spec`, `alias`, `config`, `sync` and `kit` are groups: the flag
 * they refuse is refused at the subcommand, so each is probed as one.
 */
const READERS = [
	['tail'],
	['head'],
	['grep', 'coffee'],
	['now'],
	['nowview'],
	['dayview'],
	['count'],
	['stats'],
	['ids'],
	['fields'],
	['history'],
	['check'],
	['meta'],
	['db', 'backup', '-'],
	['spec', 'list'],
	['alias', 'list'],
	['sync', 'status'],
	['kit', 'list'],
	['track', 'coffee'],
	['ignore', 'coffee'],
	['reparse']
];

/** Verbs that DO echo a document (da-yc9w's own list). */
const WRITERS = [
	['occur', 'coffee'],
	['note', 'coffee', 'jot=a note'],
	['add', 'coffee', '--id', 'a-thing'],
	['start', 'work'],
	['stop', 'work'],
	['switch', 'mood', 'happy'],
	['get', 'coffee'],
	['delete', 'mood'],
	['undo']
];

describe('the list says what the CLI does', () => {
	test('every reader would gain nothing or break, which is why it is on the list', async () => {
		await withApp(async (app) => {
			await app.run(['occur', 'coffee']);
			for (const argv of READERS) {
				assert.deepEqual(echoArgv(argv), argv, `${argv.join(' ')} is not on the list`);
				const asked = await app.run([...argv, '--show', 'json']);
				const refused = asked.stderr.includes('unexpected flag: --show');
				assert.ok(
					refused || parseEcho(asked.stdout) === null,
					`${argv.join(' ')} echoes documents — it does not belong in NO_ECHO`
				);
			}
		});
	});

	test('every echoing verb still answers with a document', async () => {
		await withApp(async (app) => {
			for (const argv of WRITERS) {
				const result = await app.run(echoArgv(argv));
				assert.notDeepEqual(echoArgv(argv), argv, `${argv.join(' ')} is on the list by mistake`);
				assert.ok(
					parseEcho(result.stdout),
					`${argv.join(' ')} → ${result.code} ${result.stdout || result.stderr}`
				);
			}
		});
	});
});
