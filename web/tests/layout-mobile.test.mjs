// `mobile` (da-shzs, da-hkjp): the layout a phone opened first — and
// still does, as the tree of its `collect` tab (da-u7vz8), behind
// `welcome` and `setup` (welcome.test.mjs). So the phones here are one tap
// into their first visit (`ON_COLLECT`), with that tree in front.
//
// vividn, 2026-09-17: «a layout "mobile" that is the default one loaded when
// the site is opened on a phone», over the app's own bottom bar. So this file
// holds the tree, that every pane of it is on the screen at a phone's sizes,
// that a log works inside it, and that a device with an open list keeps it.
//
// vividn, 2026-10-30 (da-uzi5x): «[tail] 1/3 [nowview] 1/4 [fields] 5/12 then
// have a dayview tab be a whole tab». So the strip's own tests here are on
// that tab (`ON_DAYVIEW`): what it draws in a box of its own, at the four
// sizes the tree's geometry has been measured at since da-shzs.
//
// The commands pane at the foot went in da-mwp4m. vividn, 2026-10-27:
// «remove the command widget on the collect tab (it can remain a widget in
// general)», and the bar's output, open by default over the input, has its
// room.

import test, { before, after, describe } from 'node:test';
import assert from 'node:assert/strict';
import { ON_COLLECT, sharedApp, startApp, stopApp, withApp } from './support/app.mjs';
import { fold as foldBar } from './support/bar.mjs';
import { DESKTOP, pane, titles, toggleOption } from './support/layout.mjs';
import { FREEZE, MAX_DAY } from './support/daystrip.mjs';
import { until } from './support/until.mjs';

before(startApp);
after(stopApp);

/** A phone on its first visit, `collect` in front, at `width`×`height`. */
const phone = (width = 390, height = 844) => ({
	viewport: { width, height },
	device: { isMobile: true, hasTouch: true },
	home: ON_COLLECT
});

/** The tabs a first visit opens (da-u7vz8, da-uzi5x). */
const FIRST = ['welcome', 'setup', 'collect', 'dayview'];

/** A phone one tap further on, the dayview tab in front. */
const onDayview = (width, height) => ({ ...phone(width, height), home: { names: FIRST, active: 3 } });

/** The whole-tab strip's pane, by its title: a root tab re-mints its ids. */
const stripPane = (page) => page.locator('section.pane[data-title="dayview"]');

/** `collect`'s two trees, by their panes' titles: the phone's, and the shipped desktop's (`desk: null`). */
const MOBILE_TREE = ['tail', 'nowview', 'fields'];
const DESKTOP_TREE = ['dayview', 'tail', 'fields', 'nowview', 'stats'];

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
 * The four screens the tree's geometry is measured at (da-shzs): a tall
 * iPhone, the same one in Safari with its bars showing, an older Android,
 * and the shortest of the four.
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

/** Every widget pane's box, and the top of the bottom bar. */
const screen = (page) =>
	page.evaluate(() => ({
		width: innerWidth,
		prompt: document.querySelector('.prompt-bar').getBoundingClientRect().top,
		// The DASHBOARD's panes: `main`'s, not the page's. The log under the
		// bar holds a CLI widget of its own, and it mounts on the first drag
		// (da-ckw3) — which is now how the output is unfolded, since the `^`
		// button went. A pane below the fold is not a pane on this screen.
		panes: [...document.querySelectorAll('main section.pane')].map((p) => {
			const { left, right, top, bottom } = p.getBoundingClientRect();
			return { id: p.dataset.pane, left, right, top, bottom };
		})
	}));

const fieldsBar = (page) => pane(page, 'bar').locator('.bar').first();

/** The hour ruler under today's band (`AXIS_HEIGHT`, #lib/panes.ts). */
const AXIS = 15;
/** The line between the past and a pinned today (`SEAM`, #lib/panes.ts). */
const SEAM = 3;

/**
 * The day strip's own box, and how much of each band it reveals.
 *
 * «Just 1 day» (vividn, 2026-09-21) is a claim about what is DRAWN, so it is
 * read off the bands: today's whole, and how much of yesterday. The strip
 * holds a fortnight whatever its box shows (da-bewc), so counting the
 * bands in the DOM would answer fourteen at every size — what the box
 * reveals of each is the only honest count.
 */
const bands = (page) =>
	stripPane(page).evaluate((p) => {
		const box = p.querySelector('.strip').getBoundingClientRect();
		const seen = (back) => {
			const el = p.querySelector(`[data-days-ago="${back}"]`);
			if (!el) return null;
			const r = el.getBoundingClientRect();
			// A day in the scroller is seen through the scroller.
			const clip = el.closest('.stack')?.getBoundingClientRect() ?? box;
			return {
				of: Math.round(r.height),
				seen: Math.round(Math.max(0, Math.min(r.bottom, clip.bottom) - Math.max(r.top, clip.top))),
				top: r.top,
				bottom: r.bottom
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

/**
 * Whether the keyboard's day line (da-5wa91), the divider, is an edge of
 * today's band: the seam over a pinned today (da-2hd21), the line under
 * one that scrolls.
 */
const lineOnToday = (page) =>
	stripPane(page).evaluate((p) => {
		const line = p.querySelector('.today-line').getBoundingClientRect();
		const today = p.querySelector('[data-days-ago="0"]').getBoundingClientRect();
		const mid = (line.top + line.bottom) / 2;
		return Math.min(Math.abs(mid - today.top), Math.abs(mid - today.bottom)) <= 3;
	});

/** A pane's body type size, in px — what the tail's `type` option sets. */
const typeSize = (page, id) =>
	pane(page, id).evaluate((p) => parseFloat(getComputedStyle(p.querySelector('.body')).fontSize));

describe('a fresh phone', () => {
	test('`collect` is `mobile`: tail / nowview / fields, the output open over the input', async () => {
		await asOpened(async (app) => {
			const { page } = app;
			await app.spyOnRunner();
			assert.deepEqual(await rootNames(page), FIRST);
			assert.deepEqual(await titles(page), MOBILE_TREE);
			// No command pane: the bar's output stands where it was, open.
			assert.equal(await page.locator('.prompt-bar .section.output').count(), 1, 'the output is not open');
			// The root bar's own tab is not the dashboard's.
			assert.equal(await page.locator('main [role="tab"]').count(), 0, 'something in `mobile` is still tabbed');
			// Unmodified as it opens: its extract is its baseline, and nothing
			// was written for it.
			assert.equal(await page.locator('.root-bar').evaluate((el) => el.classList.contains('modified')), false);
			assert.equal(await page.evaluate(() => localStorage.getItem('datum.layout.draft.v1.collect')), null);

			// The nowview is the default window, asked for
			// without anything being tapped: its far handle says so. (A write
			// asks it for its day's data alone now, not its window, da-0e06p.)
			await page.locator('section.pane[data-title="nowview"] .nowview-handles .handle[data-at="-30m"]').waitFor();
			assert.deepEqual(app.errors, []);
		});
	});

	// vividn: «decrease fontsize of tail a little bit». It is a declared
	// option of the tail KIND rather than a size written into the layout
	// (da-hkjp), and what it sets is a SCALE on the one `font-size` rule
	// every pane body shares. So the claim to assert is that ratio, against
	// a pane that did not ask — not merely "smaller than its neighbour".
	//
	// The baseline has to be a plain widget pane. The fields widget draws
	// its own 13px inside its bar, so measuring against it reads 0.89 and
	// says nothing about the option; the nowview is at the shared 13.6px.
	test("the tail's type is 0.85 of the rule every other pane body keeps", async () => {
		await asOpened(async ({ page }) => {
			const tail = await typeSize(page, 'tail');
			const plain = await typeSize(page, 'nowview');
			assert.ok(
				Math.abs(tail - plain * 0.85) < 0.01,
				`the tail's type is ${tail}px, not 0.85 of the ${plain}px every other body keeps`
			);
		});
	});

	// vividn: «decrease dayview height to be just 1 day», and (da-r0ydu) «a
	// day height that is equal to the height of the widget» — then
	// (da-2hd21) «it should definitely be always scrollable … make the
	// dayview at the top a little narrower, maybe half the height it is
	// right now, and yes show a little bit of yesterday», with today
	// starting unfrozen. So at every size the strip opens with today in the
	// scroller, no freeze button, and a past to scroll up into.
	test('the strip opens unfrozen and scrollable into the past at every size', async () => {
		for (const [width, height] of SIZES)
			await withApp(async ({ page }) => {
				const strip = stripPane(page);
				assert.equal(await strip.locator('button.freeze').count(), 0, `${width}×${height}: a freeze button in the pane`);
				assert.equal(await strip.locator('.row.pinned').count(), 0, `${width}×${height}: today is pinned`);
				await strip.locator('.stack [data-days-ago="0"]').waitFor({ state: 'attached' });
				const stack = await strip.locator('.stack').evaluate((el) => ({ scroll: el.scrollHeight, client: el.clientHeight }));
				assert.ok(stack.scroll > stack.client, `${width}×${height}: the stack does not scroll: ${stack.scroll} in ${stack.client}`);
			}, onDayview(width, height));
	});

	// vividn, 2026-10-15 (da-2hd21): «yes show a little bit of yesterday». The
	// day FOLLOWS the box: it is `dayUnderSliver` of it, floored at 40.
	for (const [width, height] of SIZES)
		test(`at ${width}×${height}, unfrozen, today whole and a sliver of yesterday over the seam`, async () => {
			await withApp(async ({ page }) => {
				await page.evaluate(() => window.datum.run(['occur', 'coffee', '-y']));
				await stripPane(page).locator('[data-days-ago="1"] svg').waitFor({ state: 'attached' });
				const b = await bands(page);
				const at = `${width}×${height}: ${JSON.stringify(b)}`;
				assert.equal(b.today.seen, b.today.of, `today is clipped, ${at}`);
				assert.ok(b.yesterday?.seen >= 3, `no sliver of yesterday, ${at}`);
				assert.ok(b.today.top - b.yesterday.bottom >= SEAM - 0.5, `no seam between them, ${at}`);
			}, onDayview(width, height));
		});

	test('at 390×844, the day line on today, frozen or not', async () => {
		await withApp(async (app) => {
			const { page } = app;
			await page.evaluate(() => window.datum.run(['occur', 'coffee', '-y']));
			const strip = stripPane(page);
			await strip.locator('[data-days-ago="1"] svg').waitFor({ state: 'attached' });
			const b = await bands(page);
			// `dayUnderSliver`, up to `MAX_DAY_HEIGHT`: a whole tab holds more past.
			assert.equal(b.today.of, Math.min(MAX_DAY, Math.round((b.inner - AXIS - SEAM) / 1.25)), `a ${b.today.of}px today in a ${b.inner}px box`);
			assert.equal(await lineOnToday(page), true, 'the day line is not on today');

			// The freeze is the widget's option, in the root tab's menu on a tab
			// of one widget, and pinned the same holds: yesterday over today, the
			// day line the seam between them.
			await toggleOption(page, null, FREEZE);
			await strip.locator('.row.pinned').waitFor();
			await page.waitForFunction(() => {
				const stack = document.querySelector('.stack');
				return stack.scrollTop + stack.clientHeight >= stack.scrollHeight - 1;
			});
			const pinned = await bands(page);
			assert.ok(pinned.yesterday?.seen > 0, `pinned, none of yesterday shows: ${JSON.stringify(pinned)}`);
			assert.equal(await lineOnToday(page), true, 'pinned, the day line is not on today');
		}, onDayview());
	});

	// vividn, 2026-10-30 (da-uzi5x): 1/3, 1/4, 5/12, each the full width.
	test('at 390×844 the rows are his 1/3, 1/4 and 5/12, each the full width', async () => {
		await withApp(async ({ page }) => {
			const { panes } = await screen(page);
			// The splits' shares are of what the borders between the rows leave.
			const height = panes[2].bottom - panes[0].top - (panes[1].top - panes[0].bottom) * 2;
			assert.deepEqual(panes.map((p) => p.id), ['tail', 'nowview', 'bar']);
			[1 / 3, 1 / 4, 5 / 12].forEach((share, i) => {
				const h = panes[i].bottom - panes[i].top;
				assert.ok(Math.abs(h - height * share) <= 2, `${panes[i].id} is ${h}px of ${height}`);
				assert.ok(Math.abs(panes[i].right - panes[i].left - (panes[0].right - panes[0].left)) <= 1, `${panes[i].id} is not the full width`);
			});
		}, phone());
	});

	for (const [width, height] of SIZES) {
		test(`at ${width}x${height} every pane is on the screen, the output open or folded`, async (t) => {
			await withApp(async (app) => {
				const { page } = app;
				await app.declare('coffee');
				await app.reload();
				// A phone opens with the output OPEN (da-mwp4m), which is the
				// state this layout is drawn for now; folded gives the dashboard
				// the output's room back. Both are asked: nothing is cut in
				// either, the room the commands pane took being the output's.
				for (const folded of [false, true]) {
					if (folded) await foldBar(page);
					// `main` clips rather than scrolls, so the scroll metrics say
					// nothing: each pane's own box against the screen does.
					const s = await screen(page);
					assert.equal(s.panes.length, 3);
					const under = s.panes.filter((p) => p.bottom > s.prompt + 0.5).map((p) => p.id);
					for (const p of s.panes)
						assert.ok(p.left >= 0 && p.right <= s.width, `${p.id} is off the side: ${p.left}…${p.right} of ${s.width}`);
					assert.deepEqual(under, [], `${folded ? 'folded' : 'open'}, ${under.join(', ')} runs under the bottom bar`);
					t.diagnostic(`${width}×${height} ${folded ? 'folded' : 'open'}: ${s.panes.map((p) => `${p.id} ${Math.round(p.bottom - p.top)}`).join(' ')}`);
				}
				assert.deepEqual(app.errors, []);
			}, phone(width, height));
		});
	}

	// The commands pane's two taps were `occur` and then the field. With no
	// commands pane, a field's own press is the log (fields-press.test.mjs),
	// and the bar is where `undo` is typed.
	test('a tap logs an entry, tail shows it, and `undo` at the bar takes it back', async () => {
		await withApp(async (app) => {
			const { page } = app;
			await app.declare('coffee');
			await app.reload();
			await fieldsBar(page).locator('[data-field="coffee"]').click();
			await until(() => app.entries(5), (es) => es.some((e) => e.field === 'coffee'));
			await app.idle();
			await page.waitForFunction(() =>
				[...document.querySelectorAll('section[data-pane="tail"] tbody tr')].some((tr) => tr.textContent.includes('coffee'))
			);
			assert.equal((await app.entries(5)).length, 1);

			await page.locator('.prompt-bar input').fill('undo');
			await page.locator('.prompt-bar input').press('Enter');
			await until(() => app.entries(5), (es) => es.length === 0);
			assert.deepEqual(app.errors, []);
		}, phone(390, 664));
	});
});

// The strip left collect for its own tab (da-uzi5x): an instant picked on
// collect's nowview is the page's, and the dayview tab draws it.
test('a time tapped on collect’s nowview stands on the dayview tab, and its pill with it', async () => {
	await withApp(async ({ page }) => {
		const pills = () => page.evaluate(() => window.datum.bar.pills().map((p) => `${p.kind}:${p.text}`));
		const band = page.locator('section.pane[data-title="nowview"] .nowview-band');
		await band.locator('svg.svg .gridline').first().waitFor({ state: 'attached' });
		const box = await band.boundingBox();
		await page.touchscreen.tap(box.x + box.width / 2, box.y + box.height / 2);
		const picked = await until(pills, (seen) => seen.length > 0);
		await page.evaluate(() => window.datum.shell.activate(3));
		await stripPane(page).locator('.band[data-days-ago="0"] svg.svg line.cursor').first().waitFor({ state: 'attached' });
		assert.deepEqual(await pills(), picked);
	}, phone());
});

describe('who gets it', () => {
	test('a device with an open list keeps it, phone or desktop', async () => {
		for (const device of [phone(), DESKTOP])
			await withApp(async ({ page }) => {
				assert.deepEqual(await rootNames(page), ['tail']);
			}, { ...device, home: 'tail' });
	});

	// The tabs are the same on every device; what follows the DEVICE is
	// `collect`'s tree, as `main`'s (da-u7vz8) — and which device this is, is
	// decided at its first load and kept (da-lsjs9). vividn, 2026-10-09: «once
	// per device i would say … I would definitely keep the layouts
	// consistent per device». Until then it was the screen's width at each
	// load, so a phone on its side — 844px, past the 720 that says phone —
	// reloaded into the desktop tree.
	test('a phone turned on its side and reloaded keeps the mobile tree', async () => {
		await withApp(async (app) => {
			assert.deepEqual(await titles(app.page), MOBILE_TREE);
			await app.page.setViewportSize({ width: 844, height: 390 });
			await app.reload();
			assert.deepEqual(await rootNames(app.page), FIRST);
			assert.deepEqual(await titles(app.page), MOBILE_TREE);
		}, { ...phone(), desk: null });
	});

	test('a desktop narrowed to a phone’s width and reloaded keeps the desktop tree', async () => {
		await withApp(async (app) => {
			assert.deepEqual(await titles(app.page), DESKTOP_TREE);
			await app.page.setViewportSize({ width: 600, height: 900 });
			await app.reload();
			assert.deepEqual(await rootNames(app.page), FIRST);
			assert.deepEqual(await titles(app.page), DESKTOP_TREE);
		}, { ...DESKTOP, home: ON_COLLECT, desk: null });
	});

	// The way to change it: a reset is a first visit again, and a first
	// visit decides — by the screen as it is then.
	test('Reset default layouts decides the device again, by the screen it is pressed on', async () => {
		await withApp(async (app) => {
			const { page } = app;
			await page.setViewportSize({ width: 1200, height: 900 });
			await app.reload();
			assert.deepEqual(await titles(page), MOBILE_TREE);
			await page.locator('.root-bar button.gear').click();
			await page.getByRole('menuitem', { name: 'Reset default layouts', exact: true }).click();
			await page.locator('dialog.confirm').getByRole('button', { name: 'reset', exact: true }).click();
			await page.locator('dialog.confirm').waitFor({ state: 'detached' });
			// A reset lands on `welcome`, some time after its dialog goes;
			// `collect` is the third tab.
			await until(
				async () => (await page.evaluate(() => window.datum.shell.activate(2)), titles(page)),
				(t) => t.join() === DESKTOP_TREE.join()
			);
			await app.reload();
			assert.deepEqual(await titles(page), DESKTOP_TREE);
		}, { ...phone(), desk: null });
	});

	test('Reset default layouts lands on the first visit’s tabs, on a phone and on a desktop', async () => {
		for (const device of [phone(), FRESH_DESKTOP])
			await withApp(async ({ page }) => {
				await page.evaluate(() => window.datum.shell.open('tail'));
				await until(() => rootNames(page), (names) => names.join() === [...FIRST, 'tail'].join());
				for (const _ of FIRST) await page.evaluate(() => window.datum.shell.close(0));
				assert.deepEqual(await rootNames(page), ['tail']);
				await page.locator('.root-bar button.gear').click();
				await page.getByRole('menuitem', { name: 'Reset default layouts', exact: true }).click();
				const dialog = page.locator('dialog.confirm');
				await dialog.getByRole('button', { name: 'reset', exact: true }).click();
				await dialog.waitFor({ state: 'detached' });
				await until(() => rootNames(page), (names) => names.join() === FIRST.join());
			}, device);
	});

	test('`/mobile` draws on a desktop, and keeps `?db=`', async () => {
		await withApp(async (app) => {
			const { page } = app;
			await app.goto('/mobile?db=x');
			await page.locator('.root-bar [role="tab"][aria-selected="true"]', { hasText: 'mobile' }).waitFor();
			assert.equal(await page.locator('main .notice').count(), 0);
			assert.deepEqual(await titles(page), MOBILE_TREE);
			assert.match(await page.evaluate(() => location.search), /db=x/);
			assert.deepEqual(app.errors, []);
		}, DESKTOP);
	});
});
