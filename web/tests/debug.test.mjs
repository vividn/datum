// The debug widgets, and the DEBUG database that gates them (da-o2wm8, da-6ecm5).
//
// vividn, 2026-09-21: "A db named DEBUG, which has a layout preloaded into it
// this DEBUG widget. It only gets loaded for the DEBUG db. […] The DEBUG
// widget is only available from the DEBUG db, and is one of the premade
// widgets."
//
// Two claims, each measured on the page: the GATE — `?db=DEBUG` opens onto a
// layout that already holds the widgets, and no other database offers the
// kind — and the sample SEEDER.
//
// The tuning knobs' own tests are gone (da-s2g46). vividn, 2026-09-24: "we
// don't need to lock in that behavior … skip the actual testing for CI
// purposes because they are not important in userland." Like every
// `tests/debug*.test.mjs`, this file is skipped when `CI` is set, and runs
// locally or with `DATUM_TEST_DEBUG=1` (web/scripts/test.mjs).

import test, { before, after, describe } from 'node:test';
import assert from 'node:assert/strict';
import { startApp, stopApp, withApp } from './support/app.mjs';
import { manage } from './support/layout.mjs';

before(startApp);
after(stopApp);

/** The debug database, and the layout it carries. Both `defaults.ts`'s. */
const DEBUG = { query: '?db=DEBUG' };


/**
 * The widget itself, by the class it draws — never by a pane's title.
 *
 * A one-widget layout is lifted into its root tab (`inline`, shell.ts), so
 * the pane that draws a debug widget may be `shell`, and its `data-title`
 * whichever root tab is in front. So neither the pane id nor the title
 * names this widget, and `.debug-widget` and its kind do.
 */
const PANE = '.debug-widget[data-kind="debug"]';

/** Every pane on the page, by name — said out loud before anything is measured. */
const paneNames = (page) =>
	page.evaluate(() => [...document.querySelectorAll('section[data-pane]')].map((s) => s.dataset.pane));

/** The root bar's tabs: this device's open layouts, by name. */
const rootTabs = (page) => page.locator('.root-bar [role="tab"]').allInnerTexts();

describe('the DEBUG database', () => {
	test('opens onto a layout that already holds the debug widget', async () => {
		await withApp(async (app) => {
			const { page } = app;
			// SAY WHAT IS THERE and fail on the name, not on its absence: a
			// probe that measures a missing widget reads as "the feature is
			// not there" whatever the reason.
			const tabs = await rootTabs(page);
			assert.ok(tabs.includes('debug'), `the root bar has no debug tab: ${tabs.join(', ')}`);
			assert.equal(
				await page.locator(PANE).count(),
				1,
				`the debug widget is not drawn; the panes are ${(await paneNames(page)).join(', ')}`
			);
			await page.locator(`${PANE} .pair`).waitFor();

			// The tree it opened is the DATABASE's, not a built-in: the row
			// is in the data db, carrying the kit that shipped it.
			const rows = JSON.parse((await app.run(['tail', 'layout', '--json', '-n', '10'])).stdout).rows;
			const shipped = rows.filter((r) => r.data?.kit_id === 'debug');
			assert.equal(shipped.length, 1, `the DEBUG db carries ${shipped.length} debug layouts`);
			assert.equal(shipped[0].data.name, 'debug');
			assert.equal(shipped[0].slug, 'layout:debug');
		}, DEBUG);
	});

	test('installs that layout once, not once per load', async () => {
		await withApp(async (app) => {
			await app.page.locator(`${PANE} .pair`).waitFor();
			for (const round of [1, 2]) {
				await app.reload();
				await app.page.locator(`${PANE} .pair`).waitFor();
				const rows = JSON.parse((await app.run(['tail', 'layout', '--json', '-n', '10'])).stdout).rows;
				assert.equal(
					rows.filter((r) => r.data?.kit_id === 'debug').length,
					1,
					`reload ${round} installed the layout again`
				);
			}
		}, DEBUG);
	});

	// The gate is the whole reason vividn changed the design — "That seems
	// safer" — so it is asserted from the reader's own door: the chooser.
	test('is the only database whose chooser offers the kind', async () => {
		await withApp(async (app) => {
			const { page } = app;
			const offered = async () => {
				await manage(page, 'tail', 'Change Widget');
				await page.locator('dialog.chooser').waitFor();
				const kinds = await page
					.locator('dialog.chooser [role="group"][aria-label="Widgets"] button')
					.allInnerTexts();
				await page.keyboard.press('Escape');
				await page.locator('dialog.chooser').waitFor({ state: 'detached' });
				return kinds;
			};

			const here = await offered();
			assert.ok(here.includes('tail'), `the chooser offered nothing familiar: ${here.join(', ')}`);
			assert.ok(!here.includes('debug'), `the default database offered debug: ${here.join(', ')}`);

			// The SAME page, the same menu, one query away: the gate is the
			// database and nothing else about the device or the layout.
			await page.goto(`${page.url()}?db=DEBUG`);
			await page.locator('.root-bar [role="tab"]').first().waitFor();
			await page.locator('.root-bar [role="tab"]', { hasText: 'main' }).click();
			const inDebug = await offered();
			assert.ok(inDebug.includes('debug'), `DEBUG did not offer it either: ${inDebug.join(', ')}`);
			assert.deepEqual(app.errors, []);
		});
	});

	// ⚠ A saved layout is the DEVICE's — it lives in `_internal` and opens
	// in every database this device visits — so a layout naming the debug
	// widget WILL be opened somewhere the chooser would not have offered
	// it. An empty pane and a crash were the other two possible answers;
	// neither is one a reader can act on, so the widget says where its
	// contents live.
	test('a saved layout holding the widget says where it lives, elsewhere', async () => {
		await withApp(async (app) => {
			const root = {
				id: 'mine',
				tabs: [{ id: 'mine-tab', content: { id: 'mine-widget', kind: 'debug', settings: {} } }],
				active: 0,
				bar: { edge: 'top', align: 'start' }
			};
			const saved = await app.run([
				'--db',
				'_internal',
				'occur',
				'layout',
				'--id',
				'%name',
				'-k',
				'name="mine"',
				'-k',
				'version=1',
				'-k',
				`root=${JSON.stringify(root)}`
			]);
			assert.equal(saved.code, 0, `could not save the layout: ${saved.stderr}`);

			await app.page.goto(`${app.page.url().replace(/\/$/, '')}/mine`);
			await app.page.locator('.prompt-bar input:not([disabled])').waitFor();
			await app.page.locator(PANE).waitFor();
			assert.match(
				await app.page.locator(PANE).innerText(),
				/the debug widgets live in the DEBUG database/,
				'a debug widget outside DEBUG drew something other than its notice'
			);
			assert.equal(await app.page.locator(`${PANE} .pair`).count(), 0, 'the knobs were drawn anyway');
			assert.deepEqual(app.errors, []);
		});
	});
});

// The second debug widget (da-6ecm5): a sample week, written into the
// database it runs in. vividn on #118: "adding sample data shouldn't delete
// existing data or vice versa." Every count below is read back through the
// seam, never taken from the widget's own list.
describe('the sample seeder', () => {
	const SEED = '.debug-widget[data-kind="seed"]';

	/** The entries per field, as the database counts them. */
	const counts = async (app) =>
		Object.fromEntries(
			JSON.parse((await app.run(['count', '--group-by', 'field', '--json'])).stdout).map((g) => [g.key, g.count])
		);

	/** The sample's entries, as the database finds them by their tag. */
	const tagged = async (app) =>
		(await app.run(['grep', '"sample":true', '-o', '%full_hid%'])).stdout
			.split('\n')
			.filter((l) => /^[0-9a-z]+$/.test(l)).length;

	/** Press one of the widget's buttons and wait for its report, which a press takes down first. */
	async function press(page, label) {
		await page.locator(`${SEED} button`, { hasText: label }).click();
		await page.locator(`${SEED} .report`).waitFor({ timeout: 240000 });
		return page.locator(`${SEED} .report`).innerText();
	}

	/** The commands the log shows that wrote a sample entry, and whether each came back with a CREATE. */
	const written = (page) =>
		page.evaluate((seed) =>
			[...document.querySelectorAll(`${seed} .log li`)]
				.filter((li) => li.querySelector('code').textContent.endsWith(' sample=true'))
				.map((li) => ({
					argv: li.querySelector('code').textContent,
					created: /^CREATE /m.test(li.querySelector('pre')?.textContent ?? ''),
					failed: li.classList.contains('failed')
				})),
		SEED);

	test('seeds seven days ending now, one block still open, and says what ran', async () => {
		await withApp(async (app) => {
			const { page } = app;
			const report = await press(page, 'seed the last 7 days');
			const n = Number(/^(\d+) sample entries in this database$/.exec(report)?.[1]);
			assert.ok(n > 60, `a week of sample is ${report}`);
			assert.equal(await tagged(app), n, 'the report is not what the database holds');

			// The log shows every command against THIS database, each dated and
			// timed, and each answered with a CREATE.
			const log = await written(page);
			assert.equal(log.length, n);
			for (const { argv, created } of log) {
				assert.match(argv, /^❯ datum --db DEBUG \w+ \w+ .*-d \d{4}-\d\d-\d\d -t \d\d:\d\d:\d\d\.\d{3} sample=true$/);
				assert.ok(created, `${argv} came back without a CREATE`);
			}

			// Seven days of it, nothing in the future, and a block still running.
			const now = JSON.parse((await app.run(['now', '--json'])).stdout);
			const open = now.active.filter((a) => ['work', 'sleep', 'read'].includes(a.field) && a.state === true);
			assert.equal(open.length, 1, `open blocks: ${JSON.stringify(now.active)}`);
			const rows = JSON.parse((await app.run(['tail', '--json', '-n', '1000'])).stdout).rows;
			const ago = rows.filter((r) => r.data?.sample).map((r) => (Date.parse(now.now) - Date.parse(r.time)) / 86400000);
			assert.equal(ago.length, n);
			assert.ok(Math.min(...ago) >= 0, 'a sample entry is in the future');
			assert.ok(Math.max(...ago) <= 7 && Math.max(...ago) > 6, `the oldest is ${Math.max(...ago)} days ago`);
			assert.deepEqual(app.errors, []);
		}, DEBUG);
	});

	// What DEBUG is FOR is logging by hand beside the sample, in the sample's
	// own fields as much as any other — so those are the entries to keep.
	test('seeding only adds, and clearing takes only what seeding wrote', async () => {
		await withApp(async (app) => {
			const { page } = app;
			const hand = [
				['occur', 'coffee', '-t', '-5'],
				['switch', 'mood', 'calm', '-t', '-2'],
				['occur', 'posture', '-t', '-3']
			];
			for (const argv of hand) assert.equal((await app.run(argv)).code, 0, argv.join(' '));

			const first = Number(/^(\d+) sample entries/.exec(await press(page, 'seed the last 7 days'))[1]);
			const again = await press(page, 'seed the last 7 days');
			const [, total, earlier] =
				/^(\d+) sample entries in this database — (\d+) from an earlier seed, which seeding does not remove$/.exec(
					again
				) ?? [];
			assert.equal(Number(earlier), first, again);
			assert.ok(Number(total) > first * 1.5, again);
			assert.equal(await tagged(app), Number(total));
			assert.equal(Object.values(await counts(app)).reduce((a, b) => a + b), Number(total) + hand.length + 1);

			assert.equal(await press(page, 'clear the sample'), '0 sample entries in this database');
			assert.deepEqual(
				await counts(app),
				{ coffee: 1, layout: 1, mood: 1, posture: 1 },
				'the clear took more than the sample'
			);
			assert.deepEqual(app.errors, []);
		}, DEBUG);
	});

	test('stops at a command that fails, and says which', async () => {
		await withApp(async (app) => {
			const { page } = app;
			// Every day has a morning; refuse the SECOND one, so there is a
			// sample before the failure and a week after it that must not run.
			await page.evaluate(() => {
				let mornings = 0;
				const previous = window.datum.setRunner((argv, ...rest) =>
					argv.includes('end') && argv.includes('sleep') && ++mornings === 2
						? Promise.resolve({ argv, code: 1, stdout: '', stderr: 'Error: refused\n' })
						: previous(argv, ...rest)
				);
			});
			const report = await press(page, 'seed the last 7 days');
			assert.match(report, /^stopped: `datum end sleep -d [\d-]+ -t [\d:.]+ sample=true` exited 1: Error: refused$/);
			const log = await written(page);
			assert.ok(log.at(-1).failed, 'the last command shown is not the one that failed');
			assert.equal(log.filter((l) => l.failed).length, 1);
			assert.equal(await tagged(app), log.length - 1, 'the database holds more than ran before the failure');
			assert.ok(log.length > 1);
		}, DEBUG);
	});

	// A DEBUG database opened before this widget carries the knobs alone:
	// its kit row is brought up to the shipped tree, once.
	test('an older DEBUG layout gains the seeder', async () => {
		await withApp(async (app) => {
			const { page } = app;
			await page.locator(SEED).waitFor();
			const old = {
				id: 'debug-only',
				tabs: [{ id: 'debug-only-tab', content: { id: 'debug-only-widget', kind: 'debug', settings: {} } }],
				active: 0,
				bar: { edge: 'top', align: 'start' }
			};
			const put = await app.run(['update', 'layout:debug', '-k', `root=${JSON.stringify(old)}`]);
			assert.equal(put.code, 0, put.stderr);
			for (const round of [1, 2]) {
				await app.reload();
				await page.locator(SEED).waitFor();
				const rows = JSON.parse((await app.run(['tail', 'layout', '--json', '-n', '10'])).stdout).rows;
				const shipped = rows.filter((r) => r.data?.kit_id === 'debug');
				assert.equal(shipped.length, 1, `reload ${round}: ${shipped.length} debug layouts`);
			}
			const trail = JSON.parse((await app.run(['history', 'layout:debug', '--json'])).stdout);
			assert.equal(trail.length, 3, 'the shipped tree was written again on a reload that needed nothing');
		}, DEBUG);
	});
});
