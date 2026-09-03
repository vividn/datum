// datum.day's behaviour, expressed through `run(argv)`.
//
// Every assertion here is a command sequence and its observable result —
// never an internal of the runner behind the seam. That is deliberate. The
// TypeScript runner in src/lib/runners/local.ts is scheduled for deletion
// (da-1vat replaces it with the real CLI compiled to wasm), so a test that
// knew how it parsed would be deleted with it. A test that only knows
// `run(argv)` outlives the swap: the seam is precisely the thing that does
// not change. Run this suite against the wasm backend on the day it lands
// and any difference is a real behavioural divergence rather than a
// rewritten expectation.

import test, { before, after, describe } from 'node:test';
import assert from 'node:assert/strict';
import { startApp, stopApp, withApp } from './support/app.mjs';

before(startApp);
after(stopApp);

/** Exit codes the seam documents (src/lib/run.ts). */
const OK = 0;
const UNKNOWN_COMMAND = 127;

describe('logging verbs', () => {
	test('occur / start / stop / switch each land an entry the tail shows', async () => {
		await withApp(async ({ run, entries, reload, paneText }) => {
			// Distinct times, because a field's slug is its field and its
			// occur time: `start work` and `stop work` at the same instant
			// are one slug and the second is a duplicate.
			const commands = [
				['occur', 'coffee', '-t', '0700'],
				['start', 'work', '-t', '0800'],
				['stop', 'work', '-t', '1200'],
				['switch', 'mood', 'happy', '-t', '0930']
			];
			for (const argv of commands) {
				const result = await run(argv);
				assert.equal(result.code, OK, `${argv.join(' ')} → ${result.stderr}`);
				assert.equal(result.stderr, '');
				assert.notEqual(result.stdout, '', `${argv.join(' ')} printed nothing`);
			}

			const stored = await entries();
			assert.deepEqual(
				stored.map((e) => e.field).sort(),
				['coffee', 'mood', 'work', 'work']
			);

			// `tail` is a command like any other, so the seam answers it.
			const listed = await run(['tail']);
			assert.equal(listed.code, OK);
			const rows = listed.stdout.split('\n').filter(Boolean);
			assert.equal(rows.length, commands.length);
			assert.ok(
				rows.some((row) => row.includes('mood') && row.includes('happy')),
				`tail did not show the switch: ${listed.stdout}`
			);

			// And the tail pane is that command's output: the panes re-run
			// after a command the *page* ran, so reload to see what a fresh
			// visit shows.
			await reload();
			assert.equal(await paneText('tail'), listed.stdout);
		});
	});

	test('field-first argv means the same as verb-first', async () => {
		await withApp(async ({ run, entries }) => {
			assert.equal((await run(['mood', 'switch', 'happy'])).code, OK);
			const [stored] = await entries();
			assert.equal(stored.field, 'mood');
			assert.equal(stored.state, 'happy');
		});
	});

	test('a state logged twice at the same minute is a duplicate, not a second entry', async () => {
		await withApp(async ({ run, entries }) => {
			assert.equal((await run(['switch', 'mood', 'happy', '-t', '0915'])).code, OK);
			const again = await run(['switch', 'mood', 'sad', '-t', '0915']);
			assert.notEqual(again.code, OK);
			assert.match(again.stderr, /already have/);
			assert.equal((await entries()).length, 1);
		});
	});
});

describe('time and duration land on the entry', () => {
	test('-t HHMM sets the entry to that clock time', async () => {
		await withApp(async ({ run, entries }) => {
			assert.equal((await run(['occur', 'coffee', '-t', '0915'])).code, OK);
			const [stored] = await entries();
			// The browser context runs in UTC (see support/app.mjs), so the
			// stored instant's UTC clock is the clock the user typed.
			assert.match(stored.occurTime.utc, /T09:15:00/);
		});
	});

	test('-t -N backdates by N minutes', async () => {
		await withApp(async ({ run, entries }) => {
			const before = Date.now();
			assert.equal((await run(['occur', 'coffee', '-t', '-30'])).code, OK);
			const [stored] = await entries();
			const minutesAgo = (before - Date.parse(stored.occurTime.utc)) / 60000;
			assert.ok(
				minutesAgo > 29 && minutesAgo < 31,
				`expected ~30 minutes ago, got ${minutesAgo}`
			);
		});
	});

	test('a duration is stored as ISO-8601 and makes the entry a block', async () => {
		await withApp(async ({ run, entries }) => {
			assert.equal((await run(['occur', 'nap', 'dur=30m', '-t', '1000'])).code, OK);
			assert.equal((await run(['occur', 'walk', 'dur=1:30', '-t', '1100'])).code, OK);
			const stored = await entries();
			const byField = Object.fromEntries(stored.map((e) => [e.field, e]));
			assert.equal(byField.nap.dur, 'PT30M');
			assert.equal(byField.walk.dur, 'PT1H30M');
			assert.equal(byField.nap.verb, 'block');
			assert.equal(byField.walk.verb, 'block');
		});
	});

	test('no duration leaves a point entry', async () => {
		await withApp(async ({ run, entries }) => {
			await run(['occur', 'coffee', '-t', '0915']);
			const [stored] = await entries();
			assert.equal(stored.dur, undefined);
			assert.equal(stored.verb, 'point');
		});
	});
});

describe('failure', () => {
	test('an unknown command exits nonzero and says something on stderr', async () => {
		await withApp(async ({ run }) => {
			const result = await run(['definitelynotacommand']);
			assert.notEqual(result.code, OK);
			assert.equal(result.code, UNKNOWN_COMMAND);
			assert.notEqual(result.stderr, '');
			assert.equal(result.stdout, '');
		});
	});

	test('a datum command the browser cannot run yet fails loudly rather than silently', async () => {
		await withApp(async ({ run, entries }) => {
			const result = await run(['grep', 'coffee']);
			assert.equal(result.code, UNKNOWN_COMMAND);
			assert.match(result.stderr, /grep/);
			// And it is a missing command, not a field called "grep".
			assert.equal((await entries()).length, 0);
		});
	});

	test('an empty argv is a no-op success', async () => {
		await withApp(async ({ run, entries }) => {
			const result = await run([]);
			assert.equal(result.code, OK);
			assert.equal(result.stdout, '');
			assert.equal((await entries()).length, 0);
		});
	});
});

describe('the teach-echo', () => {
	test('a typed line is echoed back exactly as typed, and shell-split for the runner', async () => {
		await withApp(async ({ submit, echoes, spyOnRunner }) => {
			const line = 'note word correr "to run" srcLang=pt';
			const seen = await spyOnRunner();
			await submit(line);

			// The scrollback keeps the line the user wrote — quotes and all,
			// not a re-quoted rebuild of the argv.
			assert.equal((await echoes()).at(-1), line);

			// And what ran is that line shell-split the way the native REPL
			// splits it: the quoted words are one argument.
			// The panes re-run their own commands after every submission, so
			// the submitted one is simply the first the runner saw.
			const [executed] = await seen();
			assert.deepEqual(executed, ['note', 'word', 'correr', 'to run', 'srcLang=pt']);
		});
	});

	test('a widget press echoes the argv it ran, not a reconstruction of it', async () => {
		await withApp(async ({ run, reload, pressWidget, echoes, spyOnRunner }) => {
			// Give the bar a widget to press: a declared field gets one before
			// its first entry.
			assert.equal((await run(['spec', 'set', 'coffee', '--default-verb', 'occur'])).code, OK);
			await reload();

			const seen = await spyOnRunner();
			await pressWidget('coffee');
			const [executed] = await seen();
			assert.deepEqual(executed, ['occur', 'coffee']);
			assert.equal((await echoes()).at(-1), executed.join(' '));
		});
	});
});

describe('storage', () => {
	test('entries survive a reload', async () => {
		await withApp(async ({ run, reload, entries, paneText }) => {
			assert.equal((await run(['switch', 'mood', 'happy', '-t', '0915'])).code, OK);
			const before = await entries();
			assert.equal(before.length, 1);

			await reload();

			const after = await entries();
			assert.equal(after.length, 1, 'the entry did not survive the reload');
			// The hid is the durable identity (da-zlug) — same row, not a
			// re-derived lookalike.
			assert.equal(after[0].hid, before[0].hid);
			assert.equal(after[0].slug, before[0].slug);
			assert.match(await paneText('tail'), /happy/);
		});
	});

	test('the app does not claim durability it lacks', async () => {
		await withApp(async ({ bannerText }) => {
			// The banner reads the storage that actually opened. In-memory
			// wording here means the OPFS database failed to open — the
			// da-5iqw regression, and the one a missing COOP/COEP header
			// would reintroduce.
			assert.doesNotMatch(await bannerText(), /held in memory only/);
		});
	});

	test('the page throws nothing while logging', async () => {
		await withApp(async ({ run, errors }) => {
			await run(['occur', 'coffee']);
			assert.deepEqual(errors, []);
		});
	});
});
