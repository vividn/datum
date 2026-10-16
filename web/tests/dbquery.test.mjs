// `?db=<name>`: the page runs every command against the database the URL
// names (da-vk1x).
//
// vividn, 2026-09-16 22:26: "I like the ?db query". A shared app across tests
// is a shared database, and a person will want their second database open in
// a tab — and both want this one thing. The query is the whole of the state:
// it survives a reload, a link carries it, and nothing is stored.
//
// What is asserted here is the SEAM and the page, never the injection. A
// test that read the argv the runner was handed would be asserting the
// mechanism; what matters is where a row lands, which rows a pane shows,
// what the scrollback echoes and which tab re-reads. The one exception is
// the echo, which IS about the argv — and about the one the reader is shown,
// not the one the CLI got.

import test, { before, after, describe } from 'node:test';
import assert from 'node:assert/strict';
import { sharedApp, startApp, stopApp, withApp } from './support/app.mjs';
import { ONE_PANE } from './support/layout.mjs';
import { until } from './support/until.mjs';

/** Whether `locator` is drawn in the colour the CSS token `token` resolves to (da-ad2lt). */
const inks = (locator, token) =>
	locator.evaluate((el, token) => {
		const probe = el.appendChild(document.createElement('span'));
		probe.style.color = `var(${token})`;
		const same = getComputedStyle(probe).color === getComputedStyle(el).color;
		probe.remove();
		return same;
	}, token);

before(startApp);
after(stopApp);

/**
 * The repl layout, for the tests the day frame is only scenery for.
 *
 * Opening the day page draws a fortnight of dayview bands before `withApp`
 * returns, and these tests keep two and three TABS (da-x6ea). The tests
 * that read a PANE take the day page, because the pane is what they are
 * about.
 */
const onSeam = (query) => ({ query: `repl${query}` });

// ONE APP per query for the tests that write the page's database alone
// (da-6b3dj, sharedApp in support/app.mjs), `main` opened as a draft of one
// REPL pane — the repl layout's scenery. `solo` empties the page's database
// between tests and no other, so a test that writes the second one as well
// still opens its own app, as does one that reloads, navigates, opens a
// second tab, reads the whole scrollback, or is alone on its query.
const atOther = sharedApp(onSeam('?db=other'));
const atDefault = sharedApp(onSeam(''));
const onOther = (fn) => atOther.solo(fn, ONE_PANE);
const onDefault = (fn) => atDefault.solo(fn, ONE_PANE);

/**
 * What was LOGGED in `db`, read from this tab by naming the database —
 * `app.entries()` for a database that is not the page's.
 *
 * Without the TRACK documents that declare a field on its first use
 * (da-c8mh), as `app.entries()` drops them. A database nothing has written
 * to yet is created empty by the read, which is `datum db backup -`'s own
 * behaviour and answers `[]`.
 */
async function entriesIn(app, db) {
	const dumped = await app.page.evaluate(
		(name) => window.datum.runQuiet(['--db', name, 'db', 'backup', '-']),
		db
	);
	assert.equal(dumped.code, 0, dumped.stderr);
	const [, ...lines] = dumped.stdout.split('\n').filter(Boolean);
	return lines
		.map((line) => JSON.parse(line))
		.filter((row) => row.extra_metadata?.track === undefined && !['spec', 'alias'].includes(row.field));
}

/** The fields stored in `db` right now, sorted. */
const fieldsIn = async (app, db) => (await entriesIn(app, db)).map((entry) => entry.field).sort();

describe('the database in the URL', () => {
	test('a command lands in the named database and not in the default one', async () => {
		await onOther(async (app) => {
			assert.equal((await app.run(['occur', 'coffee', '-t', '0700'])).code, 0);

			assert.deepEqual(await fieldsIn(app, 'other'), ['coffee']);
			assert.deepEqual(await fieldsIn(app, 'datum'), []);
		});
	});

	test('the record is what was typed, with no --db in it (da-7xne1)', async () => {
		await onOther(async (app) => {
			assert.equal((await app.run(['occur', 'coffee', '-t', '0700'])).code, 0);
			const [row] = await entriesIn(app, 'other');
			assert.equal(row?.raw_input, 'occur coffee -t 0700', JSON.stringify(row));
		});
	});

	test('no ?db, and an empty one, are the default database', async () => {
		for (const query of ['', '?db=']) {
			const body = async (app) => {
				assert.equal((await app.run(['occur', 'coffee', '-t', '0700'])).code, 0);
				assert.deepEqual(await fieldsIn(app, 'datum'), ['coffee']);
				assert.deepEqual(await fieldsIn(app, 'other'), []);
			};
			// `?db=` is alone on its query: an app of its own.
			await (query === '' ? onDefault(body) : withApp(body, onSeam(query)));
		}
	});

	test('the named database survives a reload — the URL is the state', async () => {
		await withApp(async (app) => { // reloads
			assert.equal((await app.run(['occur', 'coffee', '-t', '0700'])).code, 0);
			await app.reload();
			assert.deepEqual(await fieldsIn(app, 'other'), ['coffee']);
			assert.deepEqual(await fieldsIn(app, 'datum'), []);
			// And it is still `other` the page runs against after the reload.
			assert.equal((await app.run(['occur', 'tea', '-t', '0800'])).code, 0);
			assert.deepEqual(await fieldsIn(app, 'other'), ['coffee', 'tea']);
			assert.deepEqual(await fieldsIn(app, 'datum'), []);
		}, onSeam('?db=other'));
	});

	test('the tail pane shows the named database’s rows and not the default one’s', async () => {
		await withApp(async (app) => { // reloads
			// One row in each, so "it shows coffee" cannot pass by showing
			// a `datum` the page never left.
			assert.equal((await app.run(['occur', 'coffee', '-t', '0700'])).code, 0);
			assert.equal((await app.run(['--db', 'datum', 'occur', 'tea', '-t', '0800'])).code, 0);
			await app.reload();

			const shown = (await app.paneTable('tail')).rows.flat().join(' ');
			assert.match(shown, /coffee/, `the tail pane is not showing other's rows: ${shown}`);
			assert.doesNotMatch(shown, /tea/, `the tail pane is showing datum's rows: ${shown}`);
		}, { query: '?db=other' });
	});

	test('the scrollback echoes the command as typed, with no --db in it', async () => {
		await withApp(async (app) => { // reads the whole scrollback
			await app.submit('occur coffee -t 0700');
			assert.deepEqual(await app.echoes(), ['occur coffee -t 0700']);
			// The flag is the web's, like `--width` (da-xfa4) — and the row
			// it did not show still went to `other`.
			assert.deepEqual(await fieldsIn(app, 'other'), ['coffee']);
		}, onSeam('?db=other'));
	});

	test('the tab is named after the database', async () => {
		await onOther(async (app) => {
			assert.equal(await app.page.title(), 'datum · other');
		});
		await onDefault(async (app) => {
			assert.equal(await app.page.title(), 'datum');
		});
	});
});

describe('a database the page may not be pointed at', () => {
	test('?db=_internal is refused, and the page runs on the default', async () => {
		await withApp(async (app) => { // alone on its query
			const notice = await app.page.locator('main .notice').innerText();
			assert.match(notice, /_internal/);
			assert.match(notice, /datum/);
			// No frame under the notice, so nothing is drawing `_internal`.
			assert.equal(await app.page.locator('main .frame').count(), 0);
			// The prompt still works, on the default database.
			assert.equal((await app.run(['occur', 'coffee', '-t', '0700'])).code, 0);
			assert.deepEqual(await fieldsIn(app, 'datum'), ['coffee']);
		}, { query: '?db=_internal' });
	});

	test('a path is refused too — there is no filesystem out there', async () => {
		await withApp(async (app) => { // alone on its query
			assert.match(await app.page.locator('main .notice').innerText(), /not a database name/);
			assert.equal((await app.run(['occur', 'coffee', '-t', '0700'])).code, 0);
			assert.deepEqual(await fieldsIn(app, 'datum'), ['coffee']);
		}, { query: '?db=%2Ftmp%2Fsneaky.db' });
	});
});

describe('navigating inside a database', () => {
	test('a /<name> tab stays in it, and the logo says which one', async () => {
		await withApp(async (app) => { // navigates
			// `/tail` opens that tab and puts the page back at `/` — with the
			// query, or the reader has silently changed database.
			await app.goto('/tail?db=other');
			assert.equal(new URL(app.page.url()).searchParams.get('db'), 'other');

			// The brand was a link home until da-y992 and is the database
			// menu now: what it says about the page is the checked item.
			await app.page.locator('.prompt-bar .brand').click();
			const checked = app.page.locator('[role="menu"][aria-label="databases"] [aria-checked="true"]');
			assert.equal((await checked.innerText()).replace(/[\u2713\u00a0]/g, '').trim(), 'other');
			await app.page.keyboard.press('Escape');

			// And the page is still running `other` after the navigation.
			assert.equal((await app.run(['occur', 'coffee', '-t', '0700'])).code, 0);
			assert.deepEqual(await fieldsIn(app, 'other'), ['coffee']);
			assert.deepEqual(await fieldsIn(app, 'datum'), []);
		}, { query: '?db=other' });
	});
});

describe('the layouts are the device’s, not the database’s', () => {
	test('a layout saved under one database is listed under the other', async () => {
		await withApp(async (first) => { // a second tab
			const other = await first.newTab('repl?db=other');
			const named = (app) =>
				app.page.evaluate(async () =>
					(await window.datum.layouts.listLayouts()).map((l) => l.name)
				);

			const before = await named(first);
			assert.deepEqual(await named(other), before, 'the two pages start on different layouts');

			await other.page.evaluate(() =>
				window.datum.layouts.saveLayout({
					version: 1,
					name: 'from-other',
					root: {
						id: 'root',
						active: 0,
						tabs: [{ id: 'tab-1', content: { id: 'w-1', kind: 'repl', settings: {} } }]
					}
				})
			);

			// `_internal` is untouched by `?db`: the save went there from a
			// page on `other`, and the page on `datum` sees it.
			await until(() => named(first), (names) => names.includes('from-other'));
			assert.ok((await named(other)).includes('from-other'));
			// And it is not an entry of either data database.
			assert.deepEqual(await fieldsIn(first, 'datum'), []);
			assert.deepEqual(await fieldsIn(first, 'other'), []);
		}, onSeam(''));
	});
});

describe('the cross-tab refresh is keyed by database', () => {
	/** A second listener on the change channel, so a test can tell it arrived. */
	const countAnnouncements = (app) =>
		app.page.evaluate(() => {
			window.__told = 0;
			new BroadcastChannel('datum-db-changes').addEventListener(
				'message',
				() => (window.__told += 1)
			);
		});

	test('a write in an `other` tab reaches the other `other` tab and not the `datum` one', async () => {
		await withApp(async (writer) => { // second and third tabs
			const peer = await writer.newTab('repl?db=other');
			const elsewhere = await writer.newTab('repl');
			const heardByPeer = await peer.spyOnRunner();
			const heardElsewhere = await elsewhere.spyOnRunner();
			// The channel object the app opens is made before this one, so by
			// the time this has counted a message the app's listener has had
			// it and decided (see $lib/changes.ts).
			await countAnnouncements(elsewhere);
			await peer.idle();
			await elsewhere.idle();
			const ran = (argvs) => argvs.filter((argv) => argv.includes('tail'));

			assert.equal((await writer.run(['occur', 'coffee', '-t', '0700'])).code, 0);

			// The peer on the same database re-reads…
			await until(heardByPeer, (argvs) => ran(argvs).length > 0);

			// …and the tab on `datum`, which heard the very same broadcast,
			// did not.
			await until(
				() => elsewhere.page.evaluate(() => window.__told),
				(told) => told > 0
			);
			await elsewhere.idle();
			assert.deepEqual(
				ran(await heardElsewhere()),
				[],
				'a `datum` tab re-read on a write to another database'
			);

			// The control, from the same tab and so in the same order: a write
			// to `datum` IS one that tab hears. Named explicitly, which the
			// seam leaves alone.
			assert.equal((await writer.run(['--db', 'datum', 'occur', 'tea', '-t', '0800'])).code, 0);
			await until(heardElsewhere, (argvs) => ran(argvs).length > 0);
		}, onSeam('?db=other'));
	});
});

describe('two databases behind one worker', () => {
	// The worker opens a sync access handle per database file and holds them
	// as one set (da-dvzx). A page on `?db=other` makes that set three files
	// where it was two, so the question is whether the second database is a
	// second LOCK — whether a write to `other` and a write to `datum` from
	// the same worker can block each other, or lose a row between them.
	test('forty writes split across both all land, once each, and check is clean', async () => {
		await withApp(async (app) => { // writes both databases
			const logged = await Promise.all(
				Array.from({ length: 40 }, (_, i) =>
					i % 2
						? app.run(['occur', 'ping', '-t', `07${String(i).padStart(2, '0')}`])
						: app.run(['--db', 'datum', 'occur', 'pong', '-t', `07${String(i).padStart(2, '0')}`])
				)
			);
			logged.forEach((r, i) => assert.equal(r.code, 0, `${i}: ${r.stderr}`));

			// Counted by hid in each database, not by "more than before": a
			// delta-blind probe cannot tell twenty rows from ten written twice.
			for (const [db, field] of [['other', 'ping'], ['datum', 'pong']]) {
				const rows = await entriesIn(app, db);
				assert.deepEqual(
					[...new Set(rows.map((row) => row.field))],
					[field],
					`${db} holds rows of another database's field`
				);
				assert.equal(rows.length, 20, `${db} kept ${rows.length} of its twenty ${field}s`);
				assert.equal(new Set(rows.map((row) => row.hid)).size, 20);

				const check = await app.page.evaluate(
					(name) => window.datum.runQuiet(['--db', name, 'check']),
					db
				);
				assert.equal(check.code, 0, check.stdout + check.stderr);
			}
		}, onSeam('?db=other'));
	});
});

// The three findings of vremya's REQUEST_CHANGES on PR #22, each as the
// test she asked for. All three go red on ddc41b50, the head she reviewed.
describe('the seam under review', () => {
	test('a per-command flag that is not --db does not suppress the injection', async () => {
		await onOther(async (app) => {
			// `-D` is the CLI's `--full-day`
			// (`crates/datum-parse/src/alias/scope.rs`), not a database flag.
			// The seam's scanner read it as one, so a command carrying it
			// looked like a command that had already chosen its database and
			// got no `--db other` — the day landed in `datum`.
			assert.equal((await app.run(['occur', 'coffee', '-D'])).code, 0);
			assert.deepEqual(await fieldsIn(app, 'other'), ['coffee']);
			assert.deepEqual(await fieldsIn(app, 'datum'), []);

			// And the flag still MEANT full-day, so the injection did not
			// eat it on the way past: datum stores a day as its midnight
			// with the uncertainty of a whole day over it, and slugs it
			// without a time.
			const [row] = await entriesIn(app, 'other');
			assert.equal(row.occur_time?.uncertainty, 'P1D', `-D did not log a full day: ${JSON.stringify(row)}`);
			assert.match(row.slug, /^coffee:\d{4}-\d{2}-\d{2}$/, `a full day is slugged without a time: ${row.slug}`);
		});
	});

	test('db purge names the database it will destroy, and destroys only that one', async () => {
		await withApp(async (app) => { // writes both databases
			const { page } = app;
			assert.equal((await app.run(['occur', 'coffee', '-t', '0700'])).code, 0);
			assert.equal((await app.run(['--db', 'datum', 'occur', 'tea', '-t', '0800'])).code, 0);

			// The dialog stands in for the CLI's TTY prompt, so it must name
			// the database the flag names. It named `datum` — the reader was
			// asked to type the name of a database that would survive, and
			// the one they were standing in was destroyed.
			const ask = () => app.submit('db purge');
			const dialog = page.locator('dialog.confirm');
			const name = dialog.locator('input');

			let said = ask();
			await dialog.waitFor();
			assert.equal(
				await dialog.locator('p').innerText(),
				'This will destroy ALL data in other, every entry and its history, and the copies of it kept before upgrades.\n' +
					'On this device it clears the command history and what the widgets last showed too.'
			);
			assert.match(await dialog.locator('label').innerText(), /Type\s+other\s+to confirm/);

			// The OTHER database's name is now the wrong answer, and refused.
			await name.fill('datum');
			await name.press('Enter');
			await said;
			assert.match((await app.outputs()).at(-1), /aborted/);
			assert.deepEqual(await fieldsIn(app, 'other'), ['coffee']);
			assert.deepEqual(await fieldsIn(app, 'datum'), ['tea']);

			// The right one empties `other` and leaves `datum` alone.
			said = ask();
			await dialog.waitFor();
			await name.fill('other');
			await name.press('Enter');
			await said;
			assert.deepEqual(await fieldsIn(app, 'other'), []);
			assert.deepEqual(await fieldsIn(app, 'datum'), ['tea']);
		}, onSeam('?db=other'));
	});

	// da-0hay9: off a TTY the CLI refuses a chained purge naming --leave and
	// --rejoin, and the dialog asked the name first, for nothing. Now it asks
	// which, as the CLI does on a TTY, and the name only for a purge that runs.
	test('db purge on a chained copy asks leave, rejoin or the whole chain before the name, and runs the answer', async () => {
		await withApp(async (app) => { // purges its database
			const { page } = app;
			assert.equal((await app.run(['occur', 'coffee', '-t', '0700'])).code, 0);
			// A lone member of an open chain: `new-invite` starts one.
			assert.equal((await app.run(['sync', 'new-invite', '--expires', '1d'])).code, 0);
			const dialog = page.locator('dialog.confirm');
			const name = dialog.locator('input');
			const button = (label) => dialog.getByRole('button', { name: label, exact: true });

			// The question comes first, with no name to type; cancel runs nothing.
			let said = app.submit('db purge');
			await dialog.waitFor();
			assert.match(await dialog.locator('p').innerText(), /^This copy is in sync chain \S+: leave .*rejoin .*chain .*\nLeave, rejoin, or purge the whole chain\?$/s);
			assert.equal(await name.count(), 0);
			// Neither answer destroys (the name is asked next), so neither is red.
			assert.ok(await inks(button('leave'), '--accent'));
			assert.ok(await inks(button('rejoin'), '--accent'));
			assert.ok(await inks(button('chain'), '--accent'));
			assert.ok(!(await inks(button('cancel'), '--error')));
			await button('cancel').click();
			await said;
			assert.match((await app.outputs()).at(-1), /aborted \(none of leave, rejoin or chain\)/);
			assert.deepEqual(await fieldsIn(app, 'datum'), ['coffee']);

			// Rejoin, with no other member to refetch from: the CLI's own
			// refusal, and no name asked for it.
			said = app.submit('db purge');
			await button('rejoin').click();
			await said;
			assert.match((await app.outputs()).at(-1), /no other member of sync chain \S+ holds everything to refetch from/);
			assert.equal(await dialog.count(), 0);
			assert.deepEqual(await fieldsIn(app, 'datum'), ['coffee']);

			// The chain (da-3ni5m): the CLI's own warning naming every copy, then
			// the name; a lone member's copy purges now and stays in its chain.
			said = app.submit('db purge');
			await button('chain').click();
			await name.waitFor();
			const warned = await dialog.locator('p').innerText();
			assert.match(warned, /^every copy in sync chain \S+ drops every entry, op and history; its members stay:\n/);
			assert.match(warned, /\n {2}this copy purges now, after a backup of its entries beside the database\nOn this device it clears/);
			await name.fill('datum');
			await name.press('Enter');
			await said;
			assert.match((await app.outputs()).at(-1), /purged: every other copy follows when it next connects/);
			assert.deepEqual(await fieldsIn(app, 'datum'), []);
			assert.equal((await app.run(['sync', 'chain', 'list'])).code, 0, 'a chain purge left the chain');

			// Leave: the name, then a lone member leaves telling no one and is wiped.
			assert.equal((await app.run(['occur', 'coffee', '-t', '0800'])).code, 0);
			said = app.submit('db purge');
			await button('leave').click();
			await name.waitFor();
			assert.match(await dialog.locator('p').innerText(), /\nIt leaves sync chain \S+ first\.\nOn this device/);
			await name.fill('datum');
			await name.press('Enter');
			await said;
			assert.match((await app.outputs()).at(-1), /left sync chain/);
			assert.deepEqual(await fieldsIn(app, 'datum'), []);
			assert.notEqual((await app.run(['sync', 'chain', 'list'])).code, 0, 'still in a chain');
		}, onSeam(''));
	});

	// A RACE, so it is run several times and at several delays. `hold()` used
	// to record the database list as it stood when the acquisition RESOLVED,
	// but `open()` chose its files when its loop STARTED — so a first-time
	// `--db <name>` arriving between those awaits was recorded as covered and
	// was not: the CLI made the database inside the process's own memory
	// filesystem, exited 0, and the row was gone. Measured 36/36 lost between
	// 1 ms and 30 ms (vremya). The handles are asked what they cover now.
	test('a first-time --db that lands mid-acquisition still gets a real file', async () => {
		await withApp(async (app) => { // reloads
			const lost = [];
			for (const delay of [1, 3, 5, 10, 20, 30]) {
				const db = `fresh${delay}`;
				const stored = await app.page.evaluate(
					async ([name, d]) => {
						// The queue has drained, so the first command takes a
						// fresh hold — and the second arrives inside it.
						const first = window.datum.runQuiet(['tail']);
						await new Promise((r) => setTimeout(r, d));
						const second = window.datum.runQuiet(['--db', name, 'occur', 'x', '-t', '0700']);
						const [, wrote] = await Promise.all([first, second]);
						return wrote.code;
					},
					[db, delay]
				);
				assert.equal(stored, 0, `the write itself failed on ${db}`);

				// Read back after a RELOAD: a database in the process's own
				// memory filesystem answers its own writes for as long as that
				// process lives, so only a new page proves the file is real.
				await app.reload();
				if ((await fieldsIn(app, db)).length !== 1) lost.push(delay);
			}
			assert.deepEqual(lost, [], `the row evaporated at these delays (ms): ${lost.join(', ')}`);
		}, onSeam(''));
	});
});
