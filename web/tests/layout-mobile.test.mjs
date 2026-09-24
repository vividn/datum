// `mobile` (da-shzs, da-hkjp): the layout a phone opens first.
//
// vividn, 2026-09-17: «a layout "mobile" that is the default one loaded when
// the site is opened on a phone» — dayview / tail | a tiny nowview /
// fields + nowview / commands, over the app's own bottom bar. So this file
// holds the tree, that every pane of it is on the screen at a phone's
// sizes, that the two taps of a log work inside it, and that only a FRESH
// phone gets it: a device with an open list keeps it, and a desktop still
// opens `main`.
//
// vividn, 2026-09-21, changed three things about it (da-hkjp): the strip
// shows ONE day, the tail's type is smaller, and row 3 is a SPLIT of the
// fields widget and a nowview rather than two tabs. The third is geometry
// that was MEASURED rather than reasoned about — see the note over
// `MOBILE_LAYOUT` — so the sweep below asserts it at the four sizes the
// measurement was taken at, the fourth (412×563) added here because it is
// where a column split of row 3 cut the commands pane.
//
// The FIRST is no longer geometry at all (da-r0ydu). It was a weight
// measured at one of the four screens, because a day was a fixed height
// inside a pane that is a proportion; the day follows the WIDGET now, so
// one day fits at every size by construction. What the sweep asserts is
// therefore stronger than it was: today's band WHOLE at all four, not
// whole at the tallest and clipped at the other three.

import test, { before, after, describe } from 'node:test';
import assert from 'node:assert/strict';
import { sharedApp, startApp, stopApp, withApp } from './support/app.mjs';
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

// ONE fresh phone for the first two tests (da-6b3dj, sharedApp in
// support/app.mjs), taken as it is rather than through `solo`, which puts
// back `main` and not the `mobile` a fresh phone opens: the first reads the
// app as it opened, the second only type sizes. Everything else opens its
// own — a screen of its own, a reload, a fresh device, a navigation.
const freshPhone = sharedApp(phone());
const asOpened = async (fn) => fn(await freshPhone());

/**
 * The four screens the tree's geometry was measured at (da-shzs, redone
 * for da-hkjp): a tall iPhone, the same one in Safari with its bars
 * showing, an older Android, and the shortest of the four — which is
 * where a COLUMN split of row 3 cut the commands pane off the bottom, and
 * so the size that decided row 3 is a row.
 */
const SIZES = [
	[390, 844],
	[390, 664],
	[360, 640],
	[412, 563]
];

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
// `fields` names the HOLDER of row 3's split now (da-hkjp); the widget
// itself is the pane `bar` inside it.
const fieldsBar = (page) => pane(page, 'bar').locator('.bar').first();

/** The hour ruler under today's band (`AXIS_HEIGHT`, $lib/panes.ts). */
const AXIS = 15;

/**
 * The day strip's own box, and how much of each band it reveals.
 *
 * «Just 1 day» (vividn, 2026-09-21) is a claim about what is DRAWN, so it is
 * read off the bands: today's whole, and nothing of yesterday. The strip
 * holds a fortnight whatever its box shows (da-bewc), so counting the
 * bands in the DOM would answer fourteen at every size — what the box
 * reveals of each is the only honest count.
 */
const bands = (page) =>
	pane(page, 'strip').evaluate((p) => {
		const box = p.querySelector('.strip').getBoundingClientRect();
		const seen = (back) => {
			const el = p.querySelector(`[data-days-ago="${back}"]`);
			if (!el) return null;
			const r = el.getBoundingClientRect();
			return {
				of: Math.round(r.height),
				seen: Math.round(Math.max(0, Math.min(r.bottom, box.bottom) - Math.max(r.top, box.top)))
			};
		};
		// FLOORED, to match what the widget derived from (da-r0ydu, canary
		// kimi-k3 on #73). The strip fills the pane body and DayStrip takes
		// `Math.floor(contentRect.height)` of it, so the band it draws is
		// `dayFilling(floor(h))`. Rounding here instead would disagree with
		// that by 1 the first time a ratio put the box on a fractional part
		// of .5 or more — 102.6 floors to 102 and draws 87, rounds to 103 —
		// and the band-fills-the-box assertion below would fail 102 !== 103
		// reading like a real regression. Flooring both sides keeps that
		// assertion EXACT rather than buying it off with a tolerance.
		return { inner: Math.floor(box.height), today: seen(0), yesterday: seen(1) };
	});

/** A pane's body type size, in px — what the tail's `type` option sets. */
const typeSize = (page, id) =>
	pane(page, id).evaluate((p) => parseFloat(getComputedStyle(p.querySelector('.body')).fontSize));

describe('a fresh phone', () => {
	test('opens `mobile`: dayview / tail | nowview / fields + nowview / command', async () => {
		await asOpened(async (app) => {
			const { page } = app;
			await app.spyOnRunner();
			assert.deepEqual(await rootNames(page), ['mobile']);
			// Six panes, not five: row 3 shows the fields widget and a
			// nowview side by side (da-hkjp) where it used to show one of
			// two tabs, so BOTH are on the screen and neither wears a tab.
			assert.deepEqual(await titles(page), [
				'dayview',
				'tail',
				'nowview',
				'fields',
				'nowview',
				'command'
			]);
			// The root bar's own tab is not the dashboard's: `main`'s panes
			// are what row 3 stopped tabbing.
			assert.equal(await page.locator('main [role="tab"]').count(), 0, 'something in `mobile` is still tabbed');
			// Unmodified as it opens: its extract is its baseline, and nothing
			// was written for it.
			assert.equal(await page.locator('.root-bar').evaluate((el) => el.classList.contains('modified')), false);
			assert.equal(await page.evaluate(() => localStorage.getItem('datum.layout.draft.v1.mobile')), null);

			// The row's nowview is the snapshot and the one beside `fields`
			// is the default window — both drawn at once now, so both are
			// asked for without anything being tapped.
			await page.evaluate(() => window.datum.run(['occur', 'coffee']));
			await asked(page, '0');
			await asked(page, '30m');
			assert.deepEqual(app.errors, []);
		});
	});

	// vividn: «decrease fontsize of tail a little bit». It is a declared
	// option of the tail KIND rather than a size written into the layout
	// (da-hkjp), and what it sets is a SCALE on the one `font-size` rule
	// every pane body shares. So the claim to assert is that ratio, against
	// a pane that did not ask — not merely "smaller than its neighbour".
	//
	// The baseline has to be a plain widget pane. `command` and the fields
	// widget draw their own 13px inside their bars, so measuring against
	// either reads 0.89 and says nothing about the option; the strip, the
	// snapshot and the windowed nowview are all at the shared 13.6px.
	test("the tail's type is 0.85 of the rule every other pane body keeps", async () => {
		await asOpened(async ({ page }) => {
			const tail = await typeSize(page, 'tail');
			const plain = await typeSize(page, 'nowview');
			assert.equal(plain, await typeSize(page, 'strip'), 'the panes that asked for nothing disagree');
			assert.ok(
				Math.abs(tail - plain * 0.85) < 0.01,
				`the tail's type is ${tail}px, not 0.85 of the ${plain}px every other body keeps`
			);
		});
	});

	// vividn: «decrease dayview height to be just 1 day», and (da-r0ydu) «a
	// day height that is equal to the height of the widget». One day is
	// today's band WHOLE, none of yesterday under the strip's box, and the
	// band exactly filling the box less the ruler — at every size, because
	// the day is derived from the box and not weighed against it.
	//
	// The third claim is the one that says the day FOLLOWED rather than
	// happening to fit: a band the size of its box cannot be a number
	// written down anywhere, since the four boxes differ.
	test('the strip shows one whole day at every size, the band filling its box', async () => {
		const drawn = [];
		for (const [width, height] of SIZES)
			await withApp(async ({ page }) => {
				const b = await bands(page);
				const yesterday = b.yesterday?.seen ?? 0;
				assert.equal(
					yesterday,
					0,
					`${width}×${height}: ${yesterday}px of yesterday shows — the strip is ${b.inner}px inside, and one day is ${b.today.of} of band plus ${AXIS} of axis`
				);
				// Whole, at all four: da-hkjp's weight clipped today from the
				// top at the three shorter sizes, which was the cost of
				// holding one day with a ratio.
				assert.equal(
					b.today.seen,
					b.today.of,
					`${width}×${height}: today is clipped to ${b.today.seen} of ${b.today.of}`
				);
				// And the band IS the box, less the ruler the strip draws
				// under it: `dayFilling` read back off the screen.
				assert.equal(
					b.today.of + AXIS,
					b.inner,
					`${width}×${height}: a ${b.today.of}px band and a ${AXIS}px ruler in a ${b.inner}px box`
				);
				drawn.push(b.today.of);
			}, phone(width, height));
		// Four boxes, four different days: a constant would have drawn one
		// number four times, and this is what says no constant is left.
		assert.equal(new Set(drawn).size, drawn.length, `the four sizes drew ${drawn.join(' / ')}`);
	});

	for (const [width, height] of SIZES) {
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
					assert.equal(s.panes.length, 6);
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
	test('a fresh desktop still opens `main`', async () => {
		await withApp(async ({ page }) => {
			assert.deepEqual(await rootNames(page), ['main']);
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

	test('Reset default layouts lands on `mobile` on a phone and `main` on a desktop', async () => {
		for (const [device, want] of [
			[phone(), 'mobile'],
			[FRESH_DESKTOP, 'main']
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
			assert.deepEqual(await titles(page), ['dayview', 'tail', 'nowview', 'fields', 'nowview', 'command']);
			assert.match(await page.evaluate(() => location.search), /db=x/);
			assert.deepEqual(app.errors, []);
		}, DESKTOP);
	});
});
