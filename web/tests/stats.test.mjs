// The stats widget at /stats (da-n7r0, revamped by da-ptuwr).
//
// The widget is a frame: every number on it is `datum stats`'s. So each
// assertion holds what is on screen to what the CLI prints for the same
// argv against the same database — run through the same seam in the same
// test — never to "some numbers appeared".
//
// What da-ptuwr changed: there is no field selector. Opened fresh the
// widget runs `stats --month` (da-80do8; bare `stats`, all time, until
// then) — every field, ranked by tracked time — and the only thing that
// narrows it to a field is the field pill another widget mints. The narrowing is driven by the real
// gesture in `fieldpill.test.mjs`; this file is the widget itself.

import test, { before, after, describe } from 'node:test';
import assert from 'node:assert/strict';
import { sharedApp, startApp, stopApp, withApp as withAnyApp } from './support/app.mjs';
import { chooseRoot, widgetPane } from './support/layout.mjs';

before(startApp);
after(stopApp);

// `stats` measures up to now, so two runs a second apart can print two
// different durations. Every command here sees one fixed clock, which is
// what lets the widget's run and the test's run of the same argv agree.
const AT = Date.UTC(2026, 8, 10, 15, 0);
const withApp = (fn, options = {}) => withAnyApp(fn, { clock: AT, ...options });

// ONE APP for the tests on this clock (da-6b3dj, sharedApp in
// support/app.mjs), each on `main` alone with an empty database — or on
// `main` from a stored draft, `solo`'s second argument; `openStats`
// navigates, and the stats tab it opens is closed by the next `solo`. What
// still opens an app of its own goes to another database (a shared app
// keeps its query), opens a second page, reloads, or starts from blocked
// storage or a screen of its own.
const shared = sharedApp({ clock: AT });

/** What a stats pane runs when nothing chose its window: this month (da-80do8). */
const MONTH = ['stats', '--month'];

/** Terminal text as a reader sees it: no colour codes, runs of space as one. */
const plain = (s) => s.replace(/\x1b\[[0-9;]*m/g, '').replace(/\s+/g, ' ').trim();

/** Log a small history: two coffees, a mood, and a block of work. */
async function seed({ run }) {
	for (const argv of [
		['occur', 'coffee', '-t', '-30'],
		['occur', 'coffee', '-t', '-10'],
		['switch', 'mood', 'happy', '-t', '-20'],
		['start', 'work', '-t', '-90'],
		['end', 'work', '-t', '-40']
	]) {
		const r = await run(argv);
		assert.equal(r.code, 0, `${argv.join(' ')}: ${r.stderr}`);
	}
}

/** Go to /stats the way a reader does: by its URL, which opens the stats tab (da-0a5o). */
async function openStats(app, query = '') {
	await app.goto('/stats' + query);
	await app.page.locator('section.pane[data-title="stats"]').waitFor();
}

/** The tiles over the table, in order: `[label, value]` as a reader reads them. */
const tiles = (page) =>
	page.$$eval('.stats .tile', (els) =>
		els.map((el) => [el.querySelector('dt').textContent, el.querySelector('dd').textContent])
	);

/**
 * Wait until the widget shows `argv` and the CLI's own text for it.
 *
 * The expected text is captured through the seam first, so the comparison
 * is against this database, not against a string written down here.
 */
async function expectStats({ page, run }, argv) {
	const cli = await run(argv);
	assert.equal(cli.code, 0, `${argv.join(' ')}: ${cli.stderr}`);
	const want = { argv: argv.join(' '), text: plain(cli.stdout) };
	await page
		.waitForFunction(
			({ argv, text }) =>
				document.querySelector('.stats .argv')?.textContent === argv &&
				(document.querySelector('.stats .body')?.textContent ?? '').replace(/\s+/g, ' ').trim() === text,
			want
		)
		.catch(async () => {
			const shown = await page.locator('.stats').innerText();
			assert.fail(`wanted ${JSON.stringify(want)}, the widget shows:\n${shown}`);
		});
}

describe('/stats', () => {
	test('opened fresh it is every field, and there is no field selector left', async () => {
		await shared.solo(async (app) => {
			await seed(app);
			await openStats(app);
			await expectStats(app, MONTH);
			assert.equal(
				await app.page.locator('.stats button.field, .stats button.state').count(),
				0,
				'a field picker is still in the widget'
			);
			assert.equal(
				await app.page.locator('.stats .range button[aria-pressed="true"]').innerText(),
				'month'
			);
			assert.deepEqual(app.errors, []);
		});
	});

	// da-vpuxg. vividn: "when nothing is selected, maybe can show total
	// tracked fields, time spent tracking […] let's just get some nice top
	// level statistics there". Before this the un-narrowed widget was a bar
	// and a table with no headline at all, so the claim is that the three
	// tiles ARE there and hold the CLI's own numbers — it fails when the
	// empty state is empty.
	test('nothing selected: the headline is the database — entries, tracked, fields', async () => {
		await shared.solo(async (app) => {
			await seed(app);
			await openStats(app);
			await expectStats(app, MONTH);

			const json = await app.run([...MONTH, '--json']);
			assert.equal(json.code, 0, json.stderr);
			const { fields, total_count, tracked_duration_seconds, earliest } = JSON.parse(json.stdout);
			assert.ok(tracked_duration_seconds > 0, 'the seed tracked no time: this proves nothing');
			// The seed's first entry is 90 minutes before AT, on its day.
			assert.match(earliest, /^2026-09-10T/);
			assert.ok(fields.length > 1, `the seed gave ${fields.length} fields`);

			// The duration's SPELLING is the CLI's too, read off the header
			// line of the very text the tile sits over — not off a formatter
			// written down a second time here.
			const header = plain((await app.run(MONTH)).stdout).match(
				/^all fields · (\S+) tracked over (\d+) entries/
			);
			assert.ok(header, 'the CLI stopped printing an all-fields header');
			assert.equal(header[2], String(total_count));

			assert.deepEqual(await tiles(app.page), [
				['entries', String(total_count)],
				['tracked', header[1]],
				['fields', String(fields.length)],
				['since', '10 Sep']
			]);
			assert.deepEqual(app.errors, []);
		});
	});

	// da-cddng: the table and the head came from two runs, `stats` and then
	// `stats --json`, and the page's one queue ran them back to back — the
	// whole computation twice, 7.4 s per write on five years of data. One
	// run now, and the widget still shows exactly what `stats` prints.
	test('a write costs the widget ONE stats run, and it still shows the CLI text', async () => {
		await shared.solo(async (app) => {
			await seed(app);
			await openStats(app);
			await expectStats(app, MONTH);
			await app.idle();
			const seen = await app.spyOnRunner();
			// The loud door, as a pane's write is: /stats has no bar to type in.
			const r = await app.page.evaluate(() => window.datum.run(['occur', 'coffee', '-t', '-5']));
			assert.equal(r.code, 0, r.stderr);
			await app.idle();
			await app.page.waitForTimeout(500);
			await app.idle();
			const runs = (await seen()).filter((argv) => argv[0] === 'stats').map((argv) => argv.join(' '));
			assert.deepEqual(runs, ['stats --month --show all']);
			await expectStats(app, MONTH);
			const { total_count } = JSON.parse((await app.run([...MONTH, '--json'])).stdout);
			assert.deepEqual((await tiles(app.page))[0], ['entries', String(total_count)]);
		});
	});

	// A figure whose scope the reader has to guess at is worse than no
	// figure, and `fields` is the one that invites the guess: a window
	// leaves out a field with nothing in it. So the widget says which
	// database and which window, and both are read rather than printed.
	test('the tiles say what they cover: this database, this window', async () => {
		await shared.solo(async (app) => {
			await seed(app);
			await openStats(app);
			await expectStats(app, MONTH);
			assert.equal(await app.page.locator('.stats .scope').innerText(), 'datum · this month');

			await app.page.locator('.stats .range button', { hasText: 'week' }).click();
			await expectStats(app, ['stats', '--week']);
			assert.equal(await app.page.locator('.stats .scope').innerText(), 'datum · this week');

			// And the numbers moved with the window rather than staying this
			// month's — so the line is about these tiles and not decoration.
			const json = await app.run(['stats', '--week', '--json']);
			assert.equal(json.code, 0, json.stderr);
			const { fields, total_count } = JSON.parse(json.stdout);
			const shown = await tiles(app.page);
			assert.deepEqual(
				[shown[0], shown[2]],
				[
					['entries', String(total_count)],
					['fields', String(fields.length)]
				]
			);
		});
	});

	// The name is the page's own database (da-vk1x), not the word `datum`.
	test('on another database the scope line names that one', async () => {
		await withApp(async (app) => {
			await openStats(app, '?db=notes');
			const r = await app.run(['occur', 'coffee', '-t', '-10']);
			assert.equal(r.code, 0, r.stderr);
			await app.page.waitForFunction(
				() => document.querySelector('.stats .scope')?.textContent === 'notes · this month'
			);
			assert.deepEqual(app.errors, []);
		});
	});

	// A database with nothing in it is the widget WORKING, not an error and
	// not a pane still loading. It says so in zeroes — and with no `since`,
	// since `earliest` is null and there is no day to name (da-rcmwc).
	test('an empty database is three zeroes, not a blank pane', async () => {
		await shared.solo(async (app) => {
			await openStats(app);
			await expectStats(app, MONTH);
			assert.deepEqual(await tiles(app.page), [
				['entries', '0'],
				['tracked', '0s'],
				['fields', '0']
			]);
			assert.deepEqual(app.errors, []);
		});
	});

	test('the share bar is the ratio column: one slice per field, in the CLI’s own colours', async () => {
		await shared.solo(async (app) => {
			await seed(app);
			await openStats(app);
			await expectStats(app, MONTH);

			const json = await app.run([...MONTH, '--json']);
			assert.equal(json.code, 0, json.stderr);
			const { fields } = JSON.parse(json.stdout);
			// A field with no share of the time — coffee, two instants — is
			// not a slice; the table under the bar still counts it.
			const want = fields.filter((f) => f.ratio > 0);
			assert.ok(want.length >= 2, `the seed gave ${want.length} fields with time`);

			const slices = await app.page.$$eval('.stats .share .slice', (els) =>
				els.map((el) => ({
					text: el.textContent,
					color: getComputedStyle(el).backgroundColor,
					grow: getComputedStyle(el).flexGrow
				}))
			);
			assert.deepEqual(
				slices.map((s) => s.text),
				want.map((f) => f.field),
				'the slices are not the ranked fields'
			);
			for (const [i, f] of want.entries()) {
				const [r, g, b] = [1, 3, 5].map((at) => parseInt(f.color.slice(at, at + 2), 16));
				assert.equal(slices[i].color, `rgb(${r}, ${g}, ${b})`, `${f.field} is painted wrong`);
				assert.ok(
					Math.abs(Number(slices[i].grow) - f.ratio) < 1e-6,
					`${f.field} takes ${slices[i].grow} of the bar, not its ${f.ratio}`
				);
			}
			assert.deepEqual(app.errors, []);
		});
	});

	test('the range adds its flags: a period, or --since / --until', async () => {
		await shared.solo(async (app) => {
			await seed(app);
			await openStats(app);
			await expectStats(app, MONTH);

			// All time is one chip away, and it is bare `stats` (da-80do8).
			await app.page.locator('.stats .range button', { hasText: 'all time' }).click();
			await expectStats(app, ['stats']);

			await app.page.locator('.stats .range button', { hasText: 'today' }).click();
			await expectStats(app, ['stats', '--today']);

			await app.page.locator('.stats .range button', { hasText: 'custom' }).click();
			await expectStats(app, ['stats']);
			await app.page.locator('.stats .custom input').nth(0).fill('2026-01-01T00:00');
			await expectStats(app, ['stats', '--since', '2026-01-01T00:00']);
			await app.page.locator('.stats .custom input').nth(1).fill('2099-01-01T00:00');
			await expectStats(app, [
				'stats',
				'--since',
				'2026-01-01T00:00',
				'--until',
				'2099-01-01T00:00'
			]);
		});
	});

	test('a write in another tab changes the numbers without a reload', async () => {
		await withApp(async (app) => {
			await seed(app);
			await openStats(app);
			await expectStats(app, MONTH);
			const before = await app.page.locator('.stats .body').innerText();

			// The day frame, in a second tab on the same database.
			const other = await app.page.context().newPage();
			await other.goto(new URL('/', app.page.url()).href);
			await other.waitForFunction(() => typeof window.datum?.run === 'function');
			const r = await other.evaluate(() => window.datum.run(['occur', 'coffee', '-t', '-1']));
			assert.equal(r.code, 0, r.stderr);

			await app.page.waitForFunction(
				(was) => document.querySelector('.stats .body')?.innerText !== was,
				before
			);
			await expectStats(app, MONTH);
		});
	});

	// da-vhwq: the range chips move a setting of the widget's OWN, so what
	// they pick is THIS SESSION'S — it changes what this tab shows and the
	// layout does not hear of it. A reload therefore starts back at what the
	// layout says, and «Save options with layout» is what hands the window
	// over. Since da-ptuwr the window is the whole of what there is to hand.
	test('a reload goes back to what the layout says; the save is what keeps the window', async () => {
		await withApp(async (app) => {
			const pick = async () => {
				await app.page.locator('.stats .range button', { hasText: 'week' }).click();
				await expectStats(app, ['stats', '--week']);
			};
			const reloaded = async () => {
				await app.page.reload();
				await app.page.waitForFunction(() => typeof window.datum?.run === 'function');
				await app.page.locator('section.pane[data-title="stats"]').waitFor();
				// Drawn from the last visit before its layouts are read (da-bhudz).
				await app.page.evaluate(() => window.datum.shell.boot());
			};

			await seed(app);
			await openStats(app);
			await pick();

			await reloaded();
			// Back at what the LAYOUT says, which is this month over every field.
			await expectStats(app, MONTH);

			// `/stats` is one widget, so the acts are on the ROOT tab's menu
			// (da-ze1x).
			await pick();
			await chooseRoot(app.page, 'Save options with layout');
			await reloaded();
			await expectStats(app, ['stats', '--week']);
		});
	});

	test('a field saved by an older layout is not a selection the widget comes back to', async () => {
		await shared.solo(
			async (app) => {
				await seed(app);
				await app.goto('/');
				await app.page.locator('section.pane[data-title="stats"]').waitFor();
				// The tree still carries `field`, and the widget is every field
				// all the same: da-ptuwr took it out of the settings, so there
				// is nothing left for a stale layout to select. And its stored
				// all time is the reader's, kept as it is: bare `stats`.
				await expectStats(app, ['stats']);
				assert.deepEqual(app.errors, []);
			},
			JSON.stringify({
				version: 1,
				name: 'default',
				root: widgetPane('stats', 'stats', { field: 'mood', state: 'happy', range: 'all' })
			})
		);
	});

	test('blocked storage: the default range, and no error', async () => {
		await withApp(
			async (app) => {
				await seed(app);
				await openStats(app);
				await expectStats(app, MONTH);
				assert.equal(
					await app.page.locator('.stats .range button[aria-pressed="true"]').innerText(),
					'month'
				);
				assert.deepEqual(app.errors, [], 'the page threw with storage blocked');
			},
			{
				prepare: (context) =>
					context.addInitScript(() => {
						Object.defineProperty(window, 'localStorage', {
							configurable: true,
							get() {
								throw new DOMException('storage is blocked', 'SecurityError');
							}
						});
					})
			}
		);
	});

	// da-ptuwr: «A widget that is showing everything must not be unreadable
	// on a phone… If every field cannot fit there, the answer is what it
	// shows FIRST and how it scrolls — not a smaller font.»
	//
	// So: enough fields that the table CANNOT fit the pane — measured, not
	// assumed — and then the claim is about what is in view at the top and
	// about the box being a scroller. Nothing is cut sideways either: the
	// chips wrap and the share bar is as wide as the box and no wider.
	test('crowded on a phone: the bar and the head of the table are in view, the rest scrolls', async () => {
		await withApp(
			async (app) => {
				for (let i = 0; i < 12; i++) {
					const r = await app.run(['start', `f${i}`, '-t', `-${(i + 1) * 7}`]);
					assert.equal(r.code, 0, r.stderr);
				}
				await openStats(app);
				await expectStats(app, MONTH);

				const fit = await app.page.evaluate(() => {
					const widget = document.querySelector('section.pane .stats');
					const box = widget.closest('.content');
					const r = (el) => el.getBoundingClientRect();
					const share = r(widget.querySelector('.share'));
					const body = r(widget.querySelector('.body'));
					const view = r(box);
					const line = parseFloat(getComputedStyle(widget.querySelector('.body')).lineHeight) || 16;
					return {
						crowded: box.scrollHeight - box.clientHeight,
						sideways: box.scrollWidth - box.clientWidth,
						scrolls: ['auto', 'scroll'].includes(getComputedStyle(box).overflowY),
						shareIn: share.top >= view.top - 1 && share.bottom <= view.bottom + 1,
						shareWidth: share.width - view.width,
						linesOfTable: Math.floor((view.bottom - body.top) / line),
						since: !!widget.querySelector('.head .tile.since'),
						headRows: [...widget.querySelectorAll('.head .tile, .head .source')].reduce((rows, el) => {
							const b = r(el);
							const row = rows.find((w) => b.top < w.bottom && w.top < b.bottom);
							if (row) row.bottom = Math.max(row.bottom, b.bottom);
							else rows.push({ top: b.top, bottom: b.bottom });
							return rows;
						}, []).length
					};
				});
				assert.ok(fit.crowded > 0, 'the table fit the pane: this proves nothing');
				assert.ok(fit.scrolls, 'the crowded pane does not scroll');
				assert.equal(fit.sideways, 0, 'the widget is cut sideways');
				assert.ok(fit.shareIn, 'the share bar is not in view at the top');
				assert.ok(fit.shareWidth <= 1, `the share bar is ${fit.shareWidth}px wider than the pane`);
				assert.ok(fit.linesOfTable >= 3, `only ${fit.linesOfTable} lines of the table are in view`);
				// da-rcmwc: the fourth figure rides down onto the source's line,
				// which a phone has anyway — not onto a line of its own.
				assert.ok(fit.since, 'no since tile: this proves nothing');
				assert.ok(fit.headRows <= 2, `the head is ${fit.headRows} lines`);
				assert.deepEqual(app.errors, []);
			},
			{ viewport: { width: 390, height: 300 } }
		);
	});
});
