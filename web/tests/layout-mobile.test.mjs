// `mobile` (da-shzs): the layout a phone opens first.
//
// vividn, 2026-09-17: «a layout "mobile" that is the default one loaded when
// the site is opened on a phone» — dayview / tail | a tiny nowview /
// [fields, nowview] / commands, over the app's own bottom bar. So this file
// holds the tree, that every pane of it is on the screen at a phone's
// sizes, that the two taps of a log work inside it, and that only a FRESH
// phone gets it: a device with an open list keeps it, and a desktop still
// opens `day`.

import test, { before, after, describe } from 'node:test';
import assert from 'node:assert/strict';
import { startApp, stopApp, withApp } from './support/app.mjs';
import { unfold as openBar } from './support/bar.mjs';
import { DESKTOP, pane, titles } from './support/layout.mjs';
import { until } from './support/until.mjs';

before(startApp);
after(stopApp);

/** A phone that has never opened the site, at `width`×`height`. */
const phone = (width = 390, height = 844) => ({
	viewport: { width, height },
	device: { isMobile: true, hasTouch: true },
	home: null
});

/** A desktop that has never opened the site. */
const FRESH_DESKTOP = { ...DESKTOP, home: null };

/** The names the root bar's tabs hold, in order. */
const rootNames = (page) =>
	page.evaluate(() => window.datum.shell.rootTabs(window.datum.layout.current()).map((t) => t.layout));

/** Wait until the seam (`spyOnRunner`) has been handed a nowview of `--history h`. */
const asked = (page, h) =>
	page.waitForFunction(
		(h) => window.__datumSeen.some((a) => a[0] === 'nowview' && a[a.indexOf('--history') + 1] === h),
		h
	);

/** Every widget pane's box, and the top of the bottom bar. */
const screen = (page) =>
	page.evaluate(() => ({
		width: innerWidth,
		prompt: document.querySelector('.prompt-bar').getBoundingClientRect().top,
		// The DASHBOARD's panes: `main`'s, not the page's. The log under the
		// bar holds a REPL pane of its own, and it mounts on the first drag
		// (da-ckw3) — which is now how the output is unfolded, since the `^`
		// button went. A pane below the fold is not a pane on this screen.
		panes: [...document.querySelectorAll('main section.pane')].map((p) => {
			const { left, right, top, bottom } = p.getBoundingClientRect();
			return { id: p.dataset.pane, left, right, top, bottom };
		})
	}));

/** The buttons of `bar` drawn whole, inside the bar's box AND the screen. */
const whole = (bar) =>
	bar.evaluate((el) => {
		const box = el.getBoundingClientRect();
		return [...el.querySelectorAll('button')]
			.filter((b) => {
				const r = b.getBoundingClientRect();
				return (
					r.width > 0 &&
					r.left >= Math.max(box.left, 0) - 0.5 &&
					r.right <= Math.min(box.right, innerWidth) + 0.5 &&
					r.top >= box.top - 0.5 &&
					r.bottom <= box.bottom + 0.5
				);
			})
			.map((b) => b.textContent.trim());
	});

const commandBar = (page) => pane(page, 'command').locator('.bar.commands');
const fieldsBar = (page) => pane(page, 'fields').locator('.bar').first();

describe('a fresh phone', () => {
	test('opens `mobile`: dayview / tail | nowview / [fields, nowview] / command', async () => {
		await withApp(async (app) => {
			const { page } = app;
			await app.spyOnRunner();
			assert.deepEqual(await rootNames(page), ['mobile']);
			assert.deepEqual(await titles(page), ['dayview', 'tail', 'nowview', 'fields', 'command']);
			assert.deepEqual(await pane(page, 'fields').locator('[role="tab"]').allInnerTexts(), ['fields', 'nowview']);
			assert.equal(
				await pane(page, 'fields').locator('[role="tab"][aria-selected="true"]').innerText(),
				'fields'
			);
			// Unmodified as it opens: its extract is its baseline, and nothing
			// was written for it.
			assert.equal(await page.locator('.root-bar').evaluate((el) => el.classList.contains('modified')), false);
			assert.equal(await page.evaluate(() => localStorage.getItem('datum.layout.draft.v1.mobile')), null);

			// The row's nowview is the snapshot, asked again on a write; the
			// one behind `fields` is the default window, asked once in front.
			await page.evaluate(() => window.datum.run(['occur', 'coffee']));
			await asked(page, '0');
			await pane(page, 'fields').locator('[role="tab"]', { hasText: 'nowview' }).click();
			await asked(page, '30m');
			assert.deepEqual(app.errors, []);
		}, phone());
	});

	for (const [width, height] of [
		[390, 844],
		[390, 664],
		[360, 640]
	]) {
		test(`at ${width}x${height} every pane is on the screen, and the bars' buttons are whole`, async () => {
			await withApp(async (app) => {
				const { page } = app;
				await app.declare('coffee');
				await app.reload();
				// A fresh phone opens FOLDED (da-ckw3), which is the state
				// this layout is drawn for and which gives it back the
				// output section's ~58px. Both states are asked, because the
				// reader may pull the bar open: what is CUT when the output
				// is showing is what the second pass says out loud.
				for (const folded of [true, false]) {
					if (!folded) await openBar(page);
					// `main` clips rather than scrolls, so the scroll metrics say
					// nothing: each pane's own box against the screen does.
					const s = await screen(page);
					assert.equal(s.panes.length, 5);
					const under = s.panes.filter((p) => p.bottom > s.prompt).map((p) => p.id);
					for (const p of s.panes)
						assert.ok(p.left >= 0 && p.right <= s.width, `${p.id} is off the side: ${p.left}…${p.right} of ${s.width}`);
					if (folded)
						assert.deepEqual(under, [], `folded, ${under.join(', ')} runs under the bottom bar`);
					else if (under.length)
						// Not a failure, a MEASUREMENT: the tree sits on its own
						// floors at these sizes, so an open bar takes the last
						// pane's tail. Named here so it is a decision and not a
						// surprise, and it is why a phone starts folded.
						assert.deepEqual(
							under,
							['command'],
							`open, the panes cut are ${under.join(', ')} — not the last one alone`
						);
					const commands = await whole(commandBar(page));
					// Four at 360: the fifth, `end`, is a scroll away, since the
					// five are wider than a 360px phone's pane (da-shzs).
					if (folded)
						for (const name of ['Undo', 'Redo', 'occur', 'start', ...(width > 360 ? ['end'] : [])])
							assert.ok(commands.includes(name), `${name} is not whole: ${commands.join(' · ')}`);
					assert.ok((await whole(fieldsBar(page))).includes('Track +'), 'Track + is not whole');
				}
				assert.deepEqual(app.errors, []);
			}, phone(width, height));
		});
	}

	test('two taps log an entry, tail shows it, and Undo takes it back', async () => {
		await withApp(async (app) => {
			const { page } = app;
			await app.declare('coffee');
			await app.reload();
			await commandBar(page).locator('button[data-command="occur"]').click();
			await page.waitForFunction(() => document.querySelector('.prompt-bar input').value === 'occur ');
			await fieldsBar(page).locator('[data-field="coffee"]').click();
			await until(() => app.entries(5), (es) => es.some((e) => e.field === 'coffee'));
			await app.idle();
			await page.waitForFunction(() =>
				[...document.querySelectorAll('section[data-pane="tail"] tbody tr')].some((tr) => tr.textContent.includes('coffee'))
			);
			const logged = (await app.entries(5)).length;
			assert.equal(logged, 1);

			await commandBar(page).locator('button[data-command="undo"]').click();
			await app.idle();
			assert.equal((await app.entries(5)).length, 0, 'Undo did not take the entry back');
			assert.deepEqual(app.errors, []);
		}, phone(390, 664));
	});
});

describe('who gets it', () => {
	test('a fresh desktop still opens `day`', async () => {
		await withApp(async ({ page }) => {
			assert.deepEqual(await rootNames(page), ['day']);
		}, FRESH_DESKTOP);
	});

	test('a device with an open list keeps it, phone or desktop', async () => {
		for (const device of [phone(), DESKTOP])
			await withApp(async ({ page }) => {
				assert.deepEqual(await rootNames(page), ['tail']);
			}, { ...device, home: 'tail' });
	});

	test('decided once: a phone widened to a desktop keeps `mobile` after a reload', async () => {
		await withApp(async (app) => {
			assert.deepEqual(await rootNames(app.page), ['mobile']);
			await app.page.setViewportSize({ width: 1200, height: 900 });
			await app.reload();
			assert.deepEqual(await rootNames(app.page), ['mobile']);
		}, phone());
	});

	test('Reset default layouts lands on `mobile` on a phone and `day` on a desktop', async () => {
		for (const [device, want] of [
			[phone(), 'mobile'],
			[FRESH_DESKTOP, 'day']
		])
			await withApp(async ({ page }) => {
				await page.evaluate(() => window.datum.shell.open('tail'));
				await page.waitForFunction(() => window.datum.shell.rootTabs(window.datum.layout.current()).length === 2);
				await page.evaluate(() => window.datum.shell.close(0));
				assert.deepEqual(await rootNames(page), ['tail']);
				await page.locator('.root-bar button.gear').click();
				await page.getByRole('menuitem', { name: 'Reset default layouts…', exact: true }).click();
				const dialog = page.locator('dialog.confirm');
				await dialog.getByRole('button', { name: 'reset', exact: true }).click();
				await dialog.waitFor({ state: 'detached' });
				await until(() => rootNames(page), (names) => names.length === 1 && names[0] === want);
			}, device);
	});

	test('`/mobile` draws on a desktop, and keeps `?db=`', async () => {
		await withApp(async (app) => {
			const { page } = app;
			await app.goto('/mobile?db=x');
			await page.locator('.root-bar [role="tab"][aria-selected="true"]', { hasText: 'mobile' }).waitFor();
			assert.equal(await page.locator('main .notice').count(), 0);
			assert.deepEqual(await titles(page), ['dayview', 'tail', 'nowview', 'fields', 'command']);
			assert.match(await page.evaluate(() => location.search), /db=x/);
			assert.deepEqual(app.errors, []);
		}, DESKTOP);
	});
});
