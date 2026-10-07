// The web asks datum for its action echo as data, and places it (da-xfa4).
//
// Two claims, and the second is the one that rots if nobody watches it.
//
// 1. `echoEnv` asks for `--show json` in the env (`DATUM_SHOW=json`) for
//    the commands that have an echo, and only when EVERY side of a
//    multi-side run has one — and the entry records what was typed, not
//    the page's flag (da-vx31e).
// 2. `NO_ECHO` — the commands it leaves alone — says what the CLI does.
//    That list is a DENYLIST because an argv whose first word is not a
//    command (`mood switch happy` field-first, an alias) is a write under
//    another name and must be asked; the cost of a denylist is that a
//    command added to the CLI and not added here would be asked, and a
//    reader that draws JSON would then answer with it instead of its
//    table. So every word in the list is run through
//    the real CLI here and made to prove it belongs, and every verb that
//    does echo is made to prove it still does.
//
// 3. A reader NO_ECHO cannot name — a kit command, which the database
//    installs — is asked again without the flag (`runEchoed`, da-5x6x0).
//
// The commands run through the app's own seam, which is the real CLI
// compiled to wasm: this is datum answering, not a table written down.

import { readFileSync } from 'node:fs';
import test, { before, after, describe } from 'node:test';
import assert from 'node:assert/strict';
import { sharedApp, startApp, stopApp } from './support/app.mjs';
import { ONE_PANE } from './support/layout.mjs';
import { echoEnv, parseEcho } from '../src/lib/echo.ts';

before(startApp);
after(stopApp);

// ONE APP for the file (da-6b3dj, sharedApp in support/app.mjs): `solo`
// empties the database between tests, and what is read off the screen is
// the NEWEST document, so the scrollback of the tests before is not in it.
const shared = sharedApp();
// `main` as a draft of ONE REPL pane: nothing here reads the day frame,
// and drawing it costs what a boot did (da-m1m9a).
const onSeam = (fn) => shared.solo(fn, ONE_PANE);

const ASKED = ['DATUM_SHOW=json'];

describe('echoEnv', () => {
	test('a write verb is asked for its echo', () => {
		assert.deepEqual(echoEnv(['occur', 'coffee']), ASKED);
	});

	test('a command not in the CLI — field-first, an alias — is asked too', () => {
		// `mood switch happy` and `vbp correr` are both write commands the
		// CLI resolves in its own pre-pass; the web cannot tell them from a
		// verb and must not treat an unknown word as a reader.
		assert.deepEqual(echoEnv(['mood', 'switch', 'happy']), ASKED);
		assert.deepEqual(echoEnv(['vbp', 'correr']), ASKED);
	});

	test('a reader is left alone', () => {
		for (const argv of [['tail'], ['dayview'], ['fields', '--json'], ['db', 'backup', '-']]) {
			assert.deepEqual(echoEnv(argv), [], argv.join(' '));
		}
	});

	test('a global flag in front does not hide the command', () => {
		assert.deepEqual(echoEnv(['--db', '_internal', 'tail']), []);
		assert.deepEqual(echoEnv(['--db', '_internal', 'occur', 'x']), ASKED);
	});

	test('a multi-side run is asked only when every side echoes', () => {
		// The env reaches every side, and a reader would answer with JSON.
		assert.deepEqual(echoEnv(['occur', 'a', ',', 'occur', 'b']), ASKED);
		assert.deepEqual(echoEnv(['occur', 'a', ',', 'tail']), []);
	});
});

describe('the record is what was typed (da-vx31e)', () => {
	test('`why` quotes the line, not the page’s --show', async () => {
		await onSeam(async (app) => {
			await app.submit('occur coffee');
			const why = await app.run(['why', '_LAST']);
			assert.match(why.stdout, /raw_input:\s+occur coffee$/m, why.stdout || why.stderr);
			// …and the REPL drew its echo as a document, so it WAS asked for.
			const hid = why.stdout.match(/hid: (\w+)/)?.[1] ?? '';
			const drawn = await app.page.locator('.repl .doc[data-hid]').evaluateAll((all) => all.map((d) => d.dataset.hid));
			assert.ok(drawn.some((h) => h && hid.startsWith(h)), `${hid} not among ${drawn}`);
		});
	});

	test('a --show the reader typed wins', async () => {
		await onSeam(async (app) => {
			await app.submit('occur coffee --show minimal');
			assert.match((await app.stdouts()).at(-1) ?? '', /^CREATE /);
		});
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
 * The echo arrives painted (da-yu9s).
 *
 * `--show json` strips the ANSI, so for a while the REPL's own scrollback
 * showed `occur coffee` in grey beside a table still in datum's colours.
 * The document now carries the colours as data, and both surfaces place
 * them. Measured on the screen, and against the CLI's own numbers: the
 * colour asserted is the one the SAME document called `field_color`, not a
 * hex written down here.
 */
describe('the echo is painted in datum’s colours', () => {
	/** Every run of one surface's info line, as the browser drew it. */
	const runs = (page, root) =>
		page.locator(`${root} .doc .info`).last().evaluate((el) =>
			[...el.querySelectorAll('span')].map((s) => ({
				text: s.textContent,
				color: getComputedStyle(s).color
			}))
		);

	/** `#rrggbb` as `getComputedStyle` gives it back. */
	const asRgb = (hex) => {
		const n = parseInt(hex.slice(1), 16);
		return `rgb(${(n >> 16) & 255}, ${(n >> 8) & 255}, ${n & 255})`;
	};

	test('the field is in its own colour, in the bar and in the REPL alike', async () => {
		await onSeam(async (app) => {
			await app.submit('occur coffee');
			const [doc] = parseEcho((await app.run(['get', 'coffee', '--show', 'json'])).stdout);
			assert.ok(doc.field_color, 'the CLI answered no field colour');

			for (const root of ['.prompt-bar', '.repl']) {
				const drawn = await runs(app.page, root);
				assert.equal(
					drawn.map((r) => r.text).join(''),
					doc.info,
					`${root}: the painted line is not the line the CLI printed`
				);
				const field = drawn.filter((r) => r.text === doc.field);
				assert.equal(field.length, 1, `${root}: ${JSON.stringify(drawn)}`);
				assert.equal(
					field[0].color,
					asRgb(doc.field_color),
					`${root}: the field is not in its own colour`
				);
			}
			assert.deepEqual(app.errors, []);
		});
	});

	test('a state gets its own colour, which is not the field’s', async () => {
		await onSeam(async (app) => {
			// The field exists first, so the echo is its own entry alone.
			await app.declare('mood', 'switch');
			await app.submit('switch mood happy');
			// The entry's document: `get` answers with the field's declaration
			// (da-c8mh) first.
			const doc = parseEcho((await app.run(['get', 'mood', '--show', 'json'])).stdout).at(-1);
			assert.notEqual(doc.state_color, doc.field_color);

			const drawn = await runs(app.page, '.repl');
			assert.equal(drawn.map((r) => r.text).join(''), doc.info);
			assert.ok(
				drawn.some(
					(r) => r.text.startsWith(doc.state) && r.color === asRgb(doc.state_color)
				),
				`the state is not in its own colour: ${JSON.stringify(drawn)}`
			);
			assert.deepEqual(app.errors, []);
		});
	});
});

/**
 * The argvs `echoEnv` leaves alone, with arguments that make each one a
 * command the CLI will actually try to run.
 *
 * `db`, `spec`, `alias`, `config`, `sync` and `kit` are groups: the flag
 * they refuse is refused at the subcommand, so each is probed as one.
 *
 * `help`, `repl` and the flag-only argvs are here for da-1hdg: they are
 * not in `datum help`'s command table, which is how a sweep over that
 * table left them getting the flag and turned `help` — the first thing a
 * new user types into the bar — into a parse error.
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
	['sync'],
	['kit', 'list'],
	['track', 'coffee'],
	['ignore', 'coffee'],
	['reparse'],
	['help'],
	['help', 'occur'],
	['repl'],
	['--version'],
	['--help']
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
	// `_LAST`, not `mood`: a field names its entry AND the TRACK document
	// that declared it (da-c8mh), which is two rows, not one.
	['delete', '_LAST'],
	['undo']
];

describe('the list says what the CLI does', () => {
	test('every reader would gain nothing or break, which is why it is on the list', async () => {
		await onSeam(async (app) => {
			await app.run(['occur', 'coffee']);
			for (const argv of READERS) {
				assert.deepEqual(echoEnv(argv), [], `${argv.join(' ')} is not on the list`);
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
		await onSeam(async (app) => {
			for (const argv of WRITERS) {
				const result = await app.run([...argv, '--show', 'json']);
				assert.deepEqual(echoEnv(argv), ASKED, `${argv.join(' ')} is on the list by mistake`);
				assert.ok(
					parseEcho(result.stdout),
					`${argv.join(' ')} → ${result.code} ${result.stdout || result.stderr}`
				);
				// A second on: the clock is held, and `stop work` at the
				// instant `start work` began would close a block of nothing.
				await app.clock.forward(1000);
			}
		});
	});
});

/**
 * A kit command is a reader the list cannot hold (da-5x6x0): its name is
 * the database's. Given `--show json` it prints its rows as JSON (da-7ramf),
 * so the page asks it again without the flag, and the bar shows its table.
 */
describe('a kit command answers with its table', () => {
	const chores = readFileSync(new URL('../../kits/chores.toml', import.meta.url), 'utf8');

	test('chorelist in the bar prints the table, not its JSON', async () => {
		await onSeam(async (app) => {
			const installed = await app.run(['kit', 'install', '--yes', '-'], chores);
			assert.equal(installed.code, 0, installed.stderr);
			await app.run(['occur', 'laundry', '-d', '-3', 'next_date=+1']);
			// Control: the flag the page appends does get the rows as JSON.
			const asked = await app.run(['chorelist', '--show', 'json']);
			assert.match(asked.stdout, /^\[\{.*"field":"laundry"/, asked.stdout || asked.stderr);

			await app.submit('chorelist');
			const shown = (await app.stdouts()).at(-1);
			assert.match(shown, /Field\s+ITI\s+Due/, shown);
			assert.match(shown, /laundry/, shown);
			assert.doesNotMatch(shown, /^\[/, shown);

			// Again: the page has learned the word, and it is still the table.
			await app.submit('chorelist');
			assert.match((await app.stdouts()).at(-1), /Field\s+ITI\s+Due/);
		});
	});
});

/**
 * A kit's alias logs in the bar as it does in the CLI (da-tckse). Typed
 * into the bar, so the argv is the one the page sends: the alias's name
 * and its words, with `DATUM_SHOW=json` (da-vx31e; until then a `--show
 * json` appended to argv). Two ways that broke:
 * the alias walk took `json` for a positional, so `feel 3` stored
 * `feel=3` under `--show`; and an alias expanding to `track` (the chores
 * kit's `newchore`) refused the flag outright.
 */
describe('a kit alias logs in the bar', () => {
	const chores = readFileSync(new URL('../../kits/chores.toml', import.meta.url), 'utf8');

	test('feel, spent and newchore log, and the slots get their words', async () => {
		await onSeam(async (app) => {
			const installed = await app.run(['kit', 'install', '--yes', '-'], chores);
			assert.equal(installed.code, 0, installed.stderr);
			await app.run(['alias', 'set', 'feel', 'occur mood -k feel -k energy= -C']);
			await app.run(['alias', 'set', 'spent', 'occur spent -k amount -k category -C']);

			const last = async (field) =>
				parseEcho((await app.run(['get', field, '--show', 'json'])).stdout)?.at(-1);

			await app.submit('feel 3');
			assert.equal((await last('mood'))?.data_inline, 'feel: 3');

			await app.submit('spent 12.50 food');
			assert.equal((await last('spent'))?.data_inline, 'amount: 12.5, category: "food"');

			const tracked = async () =>
				(await app.run(['grep', '-f', 'vacuum', 'TRACKED'])).stdout.match(/TRACKED/g)?.length ?? 0;
			await app.submit('newchore vacuum 4');
			assert.match((await app.stdouts()).at(-1) ?? '', /Tracking 'vacuum'/);
			assert.equal(await tracked(), 1);
			const spec = await app.run(['spec', 'show', 'vacuum']);
			assert.match(spec.stdout, /next_date=\+4/, spec.stdout || spec.stderr);

			// Again, now the page has learned the word: one TRACK row more.
			await app.clock.forward(1000);
			await app.submit('newchore vacuum 4');
			assert.equal(await tracked(), 2);
		});
	});
});
