// The layout engine (da-jdop): panes, tabs, splits, the `…` menu, the
// borders, the draft — driven through the built app in a real browser.
//
// Every assertion here is about what is ON SCREEN — titles, widths, tab
// labels, SVG sizes, the computed writing-mode — never "more panes than
// before": a delta-blind probe passes against a page that grew the wrong
// pane. Panes are found by the id the default layout gives them
// (`$lib/layout/defaults.ts`: `tail`, `repl`, `strip`, …) or by their
// `data-title`, which is the h2 with one tab and the active tab's label
// with more.
//
// The phone cases use Playwright's iPhone 12 profile whole (a touch
// pointer, a device pixel ratio, overlay scrollbars) and drive the swipe
// through CDP's `Input.dispatchTouchEvent`, which is a real touch — the
// page's mouse cannot make one, and a swipe made of mouse events would be
// a test of the wrong handler.

import test, { before, after, describe } from 'node:test';
import assert from 'node:assert/strict';
import { devices } from 'playwright';
import { startApp, stopApp, withApp } from './support/app.mjs';

before(startApp);
after(stopApp);

const DESKTOP = { viewport: { width: 1280, height: 900 } };
const PHONE = { device: devices['iPhone 12'] };

/** The widget panes' titles, in DOM order. */
const titles = (page) =>
	page.evaluate(() => [...document.querySelectorAll('section.pane')].map((p) => p.dataset.title));

/** A pane, by the id the layout gives it. */
const pane = (page, id) => page.locator(`section[data-pane="${id}"]`);

/** Open a pane's `…` menu and choose an item by its name — `Add tab`, `split side by side`. */
async function choose(page, id, label) {
	await pane(page, id).locator('.titlebar button.more').first().click();
	await page.getByRole('menuitem', { name: label, exact: true }).click();
	await page.locator('[role="menu"]').waitFor({ state: 'detached' });
}

/** The open `…` menu, one string per line: an item's words, or a row's label and its buttons. */
const menuLines = (page) =>
	page.locator('[role="menu"]').first().evaluate((menu) =>
		[...menu.children].map((el) =>
			el.getAttribute('role') === 'group'
				? [el.getAttribute('aria-label'), ...[...el.querySelectorAll('button')].map((b) => b.textContent.trim())].join(' ')
				: el.textContent.trim()
		)
	);

/** A menu item's whole label. */
const exactly = (label) => new RegExp(`^\\s*${label.replace(/[.*+?^${}()|[\]\\]/g, '\\$&')}\\s*$`);

/** The `Bar ▸` submenu, open. */
const barMenu = (page) => page.locator('[role="menu"][aria-label="Bar"]');

/** Open a pane's `…` menu, then its `Bar ▸`. */
async function openBar(page, id) {
	await pane(page, id).locator('.titlebar button.more').first().click();
	await page.locator('[role="menu"] button').filter({ hasText: exactly('Bar') }).click();
	await barMenu(page).waitFor();
}

/** Choose a `Bar ▸` icon by its word — `left`, `end` — and wait for the menus to close. */
async function chooseBar(page, id, word) {
	await openBar(page, id);
	await barMenu(page).locator(`[aria-label="${word}"]`).click();
	await page.waitForFunction(() => !document.querySelector('[role="menu"]'));
}

/** The box of a pane's picture, as the picture says it (its `width`). */
const canvasWidth = (page, id) =>
	pane(page, id).locator('svg.svg').evaluate((svg) => svg.width.baseVal.value);

/** A real touch swipe from (x, y) by (dx, dy), through CDP. */
async function swipe(page, x, y, dx, dy) {
	const cdp = await page.context().newCDPSession(page);
	const steps = 6;
	await cdp.send('Input.dispatchTouchEvent', { type: 'touchStart', touchPoints: [{ x, y }] });
	for (let i = 1; i <= steps; i++) {
		await cdp.send('Input.dispatchTouchEvent', {
			type: 'touchMove',
			touchPoints: [{ x: x + (dx * i) / steps, y: y + (dy * i) / steps }]
		});
	}
	await cdp.send('Input.dispatchTouchEvent', { type: 'touchEnd', touchPoints: [] });
	await cdp.detach();
}

const activeTab = (page, id) =>
	pane(page, id).locator('[role="tab"][aria-selected="true"]').innerText();

describe('the default layout', () => {
	test('shows the strip, the REPL, nowview, tail and the widget bar in panes', async () => {
		await withApp(async ({ page }) => {
			assert.deepEqual(await titles(page), ['dayview', 'repl', 'nowview', 'tail', 'command', 'fields']);
		}, DESKTOP);
	});

	// The root column's ratios are [0.2, 0.7, 0.1] with the content-sized
	// widget bar holding the 0.1, so the growing cells' flex-grow summed
	// to 0.9 — and flexbox hands out only that fraction of the free
	// space, which left a tenth of it empty under the bar (da-6twf).
	test('fills its column: the last cell reaches the bottom of the split', async () => {
		await withApp(async ({ page }) => {
			const { last, split } = await page.evaluate(() => {
				const el = document.querySelector('[data-split="main"]');
				return {
					last: [...el.querySelectorAll(':scope > .cell')].at(-1).getBoundingClientRect().bottom,
					split: el.getBoundingClientRect().bottom
				};
			});
			assert.ok(Math.abs(split - last) <= 1, `the column ends at ${last}px in a split that ends at ${split}px`);
		}, DESKTOP);
	});

	test('a command through the REPL still reaches tail and nowview', async () => {
		await withApp(async (app) => {
			await app.submit('switch mood happy');
			await app.page.waitForFunction(
				() =>
					document.querySelector('section[data-pane="tail"] .body')?.textContent.includes('mood') &&
					(document.querySelector('section[data-pane="nowview"] svg.svg')?.outerHTML ?? '').includes(
						'data-field="mood"'
					)
			);
		}, DESKTOP);
	});
});

describe('the … menu', () => {
	test('Split | on the tail pane makes two panes, the new one a picker, and the pick renders', async () => {
		await withApp(async ({ page }) => {
			await choose(page, 'tail', 'split side by side');
			assert.deepEqual(await titles(page), ['dayview', 'repl', 'nowview', 'tail', 'empty', 'command', 'fields']);
			const picker = page.locator('section.pane[data-title="empty"] .picker');
			// The layouts it could hold come first (`day` is open above this
			// pane, so it is not offered — da-0a5o), then the kinds.
			assert.deepEqual(await picker.locator('button').allInnerTexts(), [
				'dayview',
				'repl',
				'nowview',
				'tail',
				'command',
				'stats',
				'fields',
				'input'
			]);
			await picker.locator('button', { hasText: 'nowview' }).click();
			// The new pane is titled by its kind and draws: a second nowview
			// beside the tail, at its own box's size.
			await page.locator('section.pane[data-title="nowview"]').nth(1).locator('svg.svg').waitFor();
			assert.deepEqual(await titles(page), ['dayview', 'repl', 'nowview', 'tail', 'nowview', 'command', 'fields']);
		}, DESKTOP);
	});

	test('Add tab twice makes a tab strip', async () => {
		await withApp(async ({ page }) => {
			await choose(page, 'tail', 'Add tab');
			await choose(page, 'tail', 'Add tab');
			const tabs = pane(page, 'tail').locator('[role="tablist"] [role="tab"]');
			assert.deepEqual(await tabs.allInnerTexts(), ['tail', 'empty', 'empty']);
			assert.equal(await activeTab(page, 'tail'), 'empty');
			assert.equal(await pane(page, 'tail').locator('h2').count(), 0, 'a tabbed pane has no h2');
			// Split Tab is only offered with 2+ tabs — and here it is.
			await pane(page, 'tail').locator('.titlebar button.more').click();
			assert.deepEqual(await menuLines(page), ['Add tab', 'Split | −', 'Split Tab | −', 'Close tab', 'Bar']);
			await page.keyboard.press('Escape');
		}, DESKTOP);
	});

	test('Split Tab is not offered with one tab', async () => {
		await withApp(async ({ page }) => {
			await pane(page, 'tail').locator('.titlebar button.more').click();
			assert.deepEqual(await menuLines(page), ['Add tab', 'Split | −', 'Close pane', 'Bar']);
			await page.keyboard.press('Escape');
		}, DESKTOP);
	});

	test('Bar ▸ left turns the title vertical, reading up; Bar ▸ right reads down', async () => {
		await withApp(async ({ page }) => {
			await chooseBar(page, 'tail', 'left');
			const h2 = pane(page, 'tail').locator('h2');
			assert.equal(await h2.evaluate((el) => getComputedStyle(el).writingMode), 'vertical-rl');
			const bar = pane(page, 'tail').locator('.titlebar');
			assert.notEqual(await bar.evaluate((el) => getComputedStyle(el).transform), 'none', 'turned round');
			const [barBox, bodyBox] = await Promise.all([
				bar.boundingBox(),
				pane(page, 'tail').locator('.body').boundingBox()
			]);
			assert.ok(barBox.x + barBox.width <= bodyBox.x + 1, 'the bar is on the left of the content');

			await chooseBar(page, 'tail', 'right');
			assert.equal(await h2.evaluate((el) => getComputedStyle(el).writingMode), 'vertical-rl');
			assert.equal(await bar.evaluate((el) => getComputedStyle(el).transform), 'none');
			const [right, body] = await Promise.all([
				bar.boundingBox(),
				pane(page, 'tail').locator('.body').boundingBox()
			]);
			assert.ok(right.x >= body.x + body.width - 1, 'the bar is on the right of the content');
		}, DESKTOP);
	});

	// The last pane cannot be closed — `tree.test.mjs` holds `closeTab` to
	// it — and the root frame, which used to grey the item, draws no bar
	// of its own now (da-2a5y): its tabs are the shell's top bar.
	test('the root frame draws no bar: a lone root widget has no title, no … and no inset', async () => {
		await withApp(
			async ({ page }) => {
				assert.deepEqual(await titles(page), ['repl']);
				const root = page.locator('section.root');
				assert.equal(await root.locator('.titlebar').count(), 0);
				assert.equal(await root.locator('button.more').count(), 0);
				assert.equal(await root.locator('h2').count(), 0);
				assert.equal(await root.locator('> .content').evaluate((el) => getComputedStyle(el).paddingTop), '0px');
			},
			{ ...DESKTOP, prepare: (context) => context.addInitScript(draft, ONE_PANE) }
		);
	});

	test('Close tab is disabled on the last root tab', async () => {
		await withApp(
			async ({ page }) => {
				// The root's tabs are the root bar's (da-0a5o), and the last
				// of them cannot be closed from its `…`.
				await page.locator('.root-bar button[aria-label="menu of the root bar"]').click();
				const close = page.locator('[role="menu"] button').filter({ hasText: exactly('Close tab') });
				assert.equal(await close.isDisabled(), true);
				await page.keyboard.press('Escape');
			},
			{ ...DESKTOP, prepare: (context) => context.addInitScript(draft, ONE_PANE) }
		);
	});

	test('closing panes unnests their split', async () => {
		await withApp(async ({ page }) => {
			await choose(page, 'command', 'Close pane');
			assert.deepEqual(await titles(page), ['dayview', 'repl', 'nowview', 'tail', 'fields']);
			await choose(page, 'nowview', 'Close pane');
			assert.deepEqual(await titles(page), ['dayview', 'repl', 'tail', 'fields']);
			// The side column is gone: the pane that held it holds the tail
			// itself now, as the row's second child.
			assert.equal(await page.locator('section.pane[data-pane="side"]').getAttribute('data-title'), 'tail');
			assert.equal(await page.locator('section.holder[data-pane="side"]').count(), 0);
		}, DESKTOP);
	});
});

describe('a border', () => {
	test('dragged 100px moves the ratio and redraws the picture at the new width', async () => {
		await withApp(async ({ page }) => {
			const border = page.locator('.split.row > [role="separator"]');
			const repl = pane(page, 'repl');
			const before = await repl.boundingBox();
			const drawn = await canvasWidth(page, 'nowview');
			const box = await border.boundingBox();
			const x = box.x + box.width / 2;
			const y = box.y + box.height / 2;
			await page.mouse.move(x, y);
			await page.mouse.down();
			for (let i = 1; i <= 5; i++) await page.mouse.move(x + i * 20, y);
			await page.mouse.up();
			await page.waitForFunction(
				([id, was]) =>
					Math.abs(document.querySelector(`section[data-pane="${id}"]`).getBoundingClientRect().width - was - 100) <= 2,
				['repl', before.width]
			);
			// The picture is re-asked at the narrower box, not squeezed into it.
			await page.waitForFunction(
				(was) => document.querySelector('section[data-pane="nowview"] svg.svg')?.width.baseVal.value < was,
				drawn
			);
			const img = pane(page, 'nowview').locator('svg.svg');
			const shown = await img.evaluate((el) => ({ natural: el.width.baseVal.value, box: el.getBoundingClientRect().width }));
			assert.ok(Math.abs(shown.natural - shown.box) <= 1, `scaled: ${JSON.stringify(shown)}`);
		}, DESKTOP);
	});

	// The kinds have no floor (da-zh2o): the side column, a holder over
	// nowview and the tail, drags down to its panes' outlines and no further.
	test('drags the side column down to its panes\' outlines and no further', async () => {
		await withApp(async ({ page }) => {
			const border = page.locator('.split.row > [role="separator"]');
			const box = await border.boundingBox();
			const x = box.x + box.width / 2;
			const y = box.y + box.height / 2;
			await page.mouse.move(x, y);
			await page.mouse.down();
			for (let i = 1; i <= 20; i++) await page.mouse.move(x + i * 80, y);
			await page.mouse.up();
			await page.waitForTimeout(300);
			const side = await pane(page, 'side').boundingBox();
			assert.ok(Math.abs(side.width - CHROME.width) <= 1, `the side column was dragged to ${side.width}px`);
		}, DESKTOP);
	});
});

describe('the draft', () => {
	test('is back after a reload, and a layout with no draft is the default', async () => {
		await withApp(async (app) => {
			const { page } = app;
			await choose(page, 'tail', 'split side by side');
			await page.locator('.picker button', { hasText: 'nowview' }).click();
			await chooseBar(page, 'strip', 'left');
			const arranged = await titles(page);
			await app.reload();
			assert.deepEqual(await titles(page), arranged);
			assert.equal(
				await pane(page, 'strip').locator('h2').evaluate((el) => getComputedStyle(el).writingMode),
				'vertical-rl'
			);
		}, DESKTOP);
	});

	test('a blocked localStorage renders the default with no error', async () => {
		await withApp(
			async (app) => {
				assert.deepEqual(await titles(app.page), ['dayview', 'repl', 'nowview', 'tail', 'command', 'fields']);
				await choose(app.page, 'tail', 'Add tab');
				assert.equal(await activeTab(app.page, 'tail'), 'empty');
				assert.deepEqual(app.errors, []);
			},
			{
				...DESKTOP,
				prepare: (context) =>
					context.addInitScript(() => {
						Object.defineProperty(window, 'localStorage', {
							get() {
								throw new Error('localStorage is blocked');
							}
						});
					})
			}
		);
	});
});

describe('on a phone', () => {
	test('the mobile default is one column with the bar on screen, tail, nowview and command as tabs', async () => {
		await withApp(async ({ page }) => {
			assert.deepEqual(await titles(page), ['dayview', 'repl', 'tail', 'fields']);
			const boxes = await page.evaluate(() =>
				[...document.querySelectorAll('section.pane')].map((p) => {
					const b = p.getBoundingClientRect();
					return { x: Math.round(b.x), w: Math.round(b.width) };
				})
			);
			assert.equal(new Set(boxes.map((b) => `${b.x}:${b.w}`)).size, 1, `not a column: ${JSON.stringify(boxes)}`);
			assert.deepEqual(await pane(page, 'side').locator('[role="tab"]').allInnerTexts(), ['tail', 'nowview', 'command']);
			const add = await page.locator('.bar button.add-button').boundingBox();
			const screen = await page.evaluate(() => document.documentElement.clientHeight);
			assert.ok(add.y >= 0 && add.y + add.height <= screen, `the + is off-screen: ${JSON.stringify(add)}`);
		}, PHONE);
	});

	test('a swipe moves between tabs, and never over a scroll', async () => {
		await withApp(async (app) => {
			const { page } = app;
			// Scrolled to the top of the screen first: on a 664px phone the
			// tabbed pane sits mostly behind the pinned widget bar until the
			// column is scrolled, and a finger there is on the bar. And from
			// the middle of the content, not its top edge: the tabs straddle
			// the frame's outline and hang into the content (da-jgrn), and
			// Chromium snaps a touch a few pixels under a tab onto the tab.
			const content = pane(page, 'side').locator('.content');
			await content.evaluate((el) => el.scrollIntoView({ block: 'start' }));
			let box = await content.boundingBox();
			const mid = box.y + box.height / 2;
			await swipe(page, box.x + box.width * 0.8, mid, -120, 0);
			await page.waitForFunction(
				() => document.querySelector('section[data-pane="side"] [role="tab"][aria-selected="true"]')?.textContent === 'nowview'
			);
			// Drawn before the swipe back: while the picture is arriving the
			// body is briefly wider than its box, and a swipe over a sideways
			// scroll is — by the rule under test — not a swipe.
			await pane(page, 'side').locator('svg.svg').waitFor();
			await swipe(page, box.x + box.width * 0.2, mid, 120, 0);
			await page.waitForFunction(
				() => document.querySelector('section[data-pane="side"] [role="tab"][aria-selected="true"]')?.textContent === 'tail'
			);

			// A pane whose content scrolls along the bar's axis keeps the
			// gesture: the REPL's `CREATE <hid> <slug>` line is wider than a
			// phone and its body scrolls sideways to it.
			await app.submit('occur coffee');
			await choose(page, 'repl', 'Add tab');
			await pane(page, 'repl').locator('[role="tab"]', { hasText: 'repl' }).click();
			const body = pane(page, 'repl').locator('.body');
			assert.ok(
				await body.evaluate((el) => el.scrollWidth > el.clientWidth),
				'the REPL body does not scroll sideways — this case needs a wider line'
			);
			await body.evaluate((el) => el.scrollIntoView({ block: 'start' }));
			box = await body.boundingBox();
			await swipe(page, box.x + box.width * 0.8, box.y + box.height / 2, -120, 0);
			await page.waitForTimeout(300);
			assert.equal(await activeTab(page, 'repl'), 'repl', 'a swipe over a sideways scroll changed the tab');
		}, PHONE);
	});
});

/** How many times each drawn command has run, from `spyOnRunner`'s list. */
const drawnRuns = (argvs) => ({
	dayview: argvs.filter((a) => a[0] === 'dayview').length,
	nowview: argvs.filter((a) => a[0] === 'nowview').length
});

/**
 * The days of the past the strip is holding. The past is one picture of
 * several days (da-hnyb) — a fortnight at most each — so the days are what
 * its pictures claim, not how many there are.
 */
const pastDays = (page) =>
	pane(page, 'strip')
		.locator('.stack .band')
		.evaluateAll((els) => els.reduce((n, b) => n + Number(b.dataset.days), 0));

/** A drawn pane's content box, whole pixels down, as `paneArgv` asks for it. */
const contentWidth = (page, id) =>
	pane(page, id)
		.locator('.body')
		.evaluate((el) => {
			const s = getComputedStyle(el);
			return Math.floor(el.clientWidth - parseFloat(s.paddingLeft) - parseFloat(s.paddingRight));
		});

const boxOf = (locator) => locator.boundingBox();

/**
 * Drag from the middle of `border` by (dx, dy) at pointer rate for
 * `ms`, and leave the button down. Time-bounded rather than counted:
 * the measurement below is redraws PER SECOND of dragging.
 */
async function dragFor(page, border, dx, dy, ms) {
	const box = await border.boundingBox();
	const x = box.x + box.width / 2;
	const y = box.y + box.height / 2;
	await page.mouse.move(x, y);
	await page.mouse.down();
	const t0 = Date.now();
	for (let i = 1; ; i++) {
		const t = Math.min(1, (Date.now() - t0) / ms);
		await page.mouse.move(x + dx * t, y + dy * t);
		if (t >= 1) break;
		await page.waitForTimeout(16);
	}
	return Date.now() - t0;
}

// vividn's first look (da-jgrn): dragging a border was jittery, because every
// drawn pane re-ran its CLI command at every size it passed through. His
// second (da-2a5y): "have the widgets update some when dragging things
// around, rather than only when you let go, maybe just have a debounce
// baked in". So a drag redraws a drawn pane at most every quarter second
// (`THROTTLE_MS`, resizing.ts) — never at pointer rate — the drop flushes
// the newest box at once, and a burst of nudges is one redraw. Counted
// through the seam, never eyeballed, with the denominator printed.
describe('resizing', () => {
	test('a border dragged 200px over a second redraws a drawn pane 2–6 times, and once more at the drop', async () => {
		await withApp(async (app) => {
			const { page } = app;
			const seen = await app.spyOnRunner();
			const border = page.locator('.split.row > [role="separator"]');
			const took = await dragFor(page, border, 200, 0, 1000);
			const during = drawnRuns(await seen());
			console.log(`  drag: ${took}ms, nowview ran ${during.nowview}x during it, dayview ${during.dayview}x`);
			assert.ok(during.nowview >= 2 && during.nowview <= 6, `nowview ran ${during.nowview} times during a ${took}ms drag`);
			// The strip spans the page above this split: its box did not move,
			// so it has nothing to redraw — and redraws nothing.
			assert.equal(during.dayview, 0, `the strip redrew ${during.dayview}x for a border that did not touch it`);
			// Meanwhile the picture is still up, scaled to its box — never blank.
			assert.ok((await canvasWidth(page, 'nowview')) > 0, 'the picture went away during the drag');

			await page.mouse.up();
			const dropped = Date.now();
			// The drop is the leading edge: the newest box is asked for at
			// once, not a throttle period later — unless the last tick had it
			// already, in which case there is nothing left to ask.
			await page.waitForFunction(
				([before, at]) =>
					window.__datumSeen.filter((a) => a[0] === 'nowview').length > before || Date.now() - at > 200,
				[during.nowview, dropped]
			);
			await app.idle();
			const after = drawnRuns(await seen());
			const atDrop = after.nowview - during.nowview;
			console.log(`  drop: nowview ran ${atDrop}x, asked ${Date.now() - dropped}ms after it`);
			assert.ok(atDrop <= 1, `nowview ran ${atDrop} times at the drop`);
			assert.equal(after.dayview, 0);
			// And the picture is the box's size: asked for at the final width,
			// not the old one stretched to it.
			assert.equal(await canvasWidth(page, 'nowview'), await contentWidth(page, 'nowview'));
			const img = pane(page, 'nowview').locator('svg.svg');
			const shown = await img.evaluate((el) => ({ natural: el.width.baseVal.value, box: el.getBoundingClientRect().width }));
			assert.ok(Math.abs(shown.natural - shown.box) <= 1, `scaled: ${JSON.stringify(shown)}`);
		}, DESKTOP);
	});

	test('ten arrow nudges in 300ms are one redraw', async () => {
		await withApp(async (app) => {
			const { page } = app;
			const seen = await app.spyOnRunner();
			const border = page.locator('.split.row > [role="separator"]');
			await border.focus();
			for (let i = 0; i < 10; i++) {
				await page.keyboard.press('ArrowRight');
				await page.waitForTimeout(30);
			}
			await page.waitForTimeout(400);
			await app.idle();
			const runs = drawnRuns(await seen());
			assert.equal(runs.nowview, 1, `nowview ran ${runs.nowview} times for ten nudges`);
			assert.equal(runs.dayview, 0, `the strip, untouched by this border, ran ${runs.dayview} bands`);
			assert.equal(await canvasWidth(page, 'nowview'), await contentWidth(page, 'nowview'));
		}, DESKTOP);
	});

	// At 1000px tall, not 900: the row under the strip sits near its floor
	// (three panes stacked in the side column), so a drag can only take the
	// strip up to that floor — and whether that is one more day or none
	// is a threshold a few pixels of chrome move (the shell's bars, da-0a5o,
	// moved it). A taller screen leaves the strip room for a third day.
	test('the strip, dragged taller over a second, reaches further back as it goes and draws each depth once', async () => {
		await withApp(async (app) => {
			const { page } = app;
			const seen = await app.spyOnRunner();
			const before = await pastDays(page);
			// The root's column border, under the strip: down is a taller strip.
			const border = page.locator('section.root > .content > .split.column > [role="separator"]').first();
			const took = await dragFor(page, border, 0, 200, 1000);
			const during = drawnRuns(await seen());
			const mid = await pastDays(page);
			console.log(`  drag: ${took}ms, dayview ran ${during.dayview}x, nowview ${during.nowview}x; days ${before} → ${mid}`);
			assert.ok(mid > before, `the strip held ${before} days before and ${mid} during the drag: it did not grow live`);
			assert.ok(during.nowview >= 2 && during.nowview <= 6, `nowview ran ${during.nowview} times during a ${took}ms drag`);
			await page.mouse.up();
			await app.idle();
			const after = drawnRuns(await seen());
			const days = await pastDays(page);
			assert.ok(days > before, `a 200px taller strip holds no more days (${before} → ${days})`);
			// Only the past is re-drawn, one picture per depth the drag passed
			// through and never the same one twice: the width did not change,
			// so today's picture, and each asked for at an earlier tick, is kept.
			const views = (await seen()).filter((a) => a[0] === 'dayview').map((a) => a.join(' '));
			assert.ok(views.every((v) => v.includes('--end-date')), `today was re-drawn: ${views.join(' | ')}`);
			assert.equal(new Set(views).size, views.length, `a picture was drawn twice: ${views.join(' | ')}`);
			assert.ok(
				after.dayview >= 1 && after.dayview <= days - before,
				`${after.dayview} pictures for ${days - before} more days`
			);
			assert.ok(after.nowview - during.nowview <= 1, 'nowview, now shorter, redraws at most once at the drop');
		}, { viewport: { width: 1280, height: 1000 } });
	});

	test('a border is grabbed right beside a … badge, which hangs half over the gap', async () => {
		await withApp(async ({ page }) => {
			// The tail's badge is centred on its top outline, so half of it
			// hangs into the gap above — the column border between nowview and
			// tail — and 4px past it, over nowview's outline (da-13bc).
			const border = pane(page, 'side').locator('.split.column > [role="separator"]').first();
			const badge = pane(page, 'tail').locator('.titlebar button.more');
			const [gap, more, tailBefore] = await Promise.all([boxOf(border), boxOf(badge), boxOf(pane(page, 'tail'))]);
			const middle = more.y + more.height / 2;
			assert.ok(Math.abs(middle - (tailBefore.y + 0.5)) <= 1, `the badge's middle (${middle}) is not on the outline (${tailBefore.y})`);
			assert.ok(Math.abs(more.y - (gap.y + gap.height - 12)) <= 1, `the badge's top (${more.y}) is not 12px above the gap's end (${gap.y + gap.height})`);
			const x = more.x + more.width + 6;
			const y = gap.y + gap.height / 2;
			await page.mouse.move(x, y);
			await page.mouse.down();
			for (let i = 1; i <= 5; i++) await page.mouse.move(x, y - i * 6);
			await page.mouse.up();
			// The tail grows upward — by less than the 30px if nowview is at
			// its floor, but it grows: this was a drag, not a press on the badge.
			await page.waitForFunction(
				([was]) => document.querySelector('section[data-pane="tail"]').getBoundingClientRect().height > was + 8,
				[tailBefore.height]
			);
			assert.equal(await page.locator('[role="menu"]').count(), 0, 'the drag opened the menu instead');
		}, DESKTOP);
	});
});

// vividn (da-nk4k): "break out the Bar: and Align: options into a Bar
// submenu, make the menu horizontal with small icons no text […] 4 icons
// for the bar placement | 3 icons for alignment. The 3 alignment icons
// change depending on the bar placement." Read through the frame's
// classes, the buttons' words and `aria-checked`, and the icons' own
// rects — never "the menu looks different".
describe('the Bar submenu', () => {
	/** The row, left to right: `|` for the separator, else a button's words, check, colour and bar. */
	const row = (page) =>
		barMenu(page).evaluate((menu) =>
			[...menu.children].map((el) => {
				if (el.getAttribute('role') === 'separator') return '|';
				const bar = el.querySelector('svg rect:not(.frame)');
				const [x, y, width, height] = ['x', 'y', 'width', 'height'].map((a) => Number(bar.getAttribute(a)));
				return {
					role: el.getAttribute('role'),
					label: el.getAttribute('aria-label'),
					title: el.getAttribute('title'),
					text: el.textContent.trim(),
					checked: el.getAttribute('aria-checked') === 'true',
					color: getComputedStyle(el).color,
					bar: { x, y, width, height }
				};
			})
		);
	const words = (cells) => cells.map((c) => (c === '|' ? c : c.label));
	const checked = (cells) => cells.filter((c) => c !== '|' && c.checked).map((c) => c.label);

	test('one Bar item for the seven; it opens four placements | three alignments, the checked ones in the accent', async () => {
		await withApp(async ({ page }) => {
			await pane(page, 'tail').locator('.titlebar button.more').click();
			const items = page.locator('[role="menu"] button');
			assert.deepEqual(await menuLines(page), ['Add tab', 'Split | −', 'Close pane', 'Bar']);
			assert.equal(await items.filter({ hasText: /Bar:|Align:/ }).count(), 0);
			await items.filter({ hasText: exactly('Bar') }).click();
			const cells = await row(page);
			assert.deepEqual(words(cells), ['top', 'bottom', 'left', 'right', '|', 'start', 'center', 'end']);
			assert.deepEqual(checked(cells), ['top', 'start']);
			for (const cell of cells.filter((c) => c !== '|')) {
				assert.equal(cell.role, 'menuitemradio');
				assert.equal(cell.text, '', `${cell.label} has words on it`);
				assert.equal(cell.color, cell.checked ? 'rgb(124, 124, 255)' : 'rgb(85, 85, 138)', cell.label);
			}
			assert.deepEqual(
				cells.slice(0, 4).map((c) => c.title),
				['bar on the top', 'bar on the bottom', 'bar on the left', 'bar on the right']
			);
		}, DESKTOP);
	});

	test('right moves the frame\'s bar there and turns the alignment icons; the align stays', async () => {
		await withApp(async ({ page }) => {
			await openBar(page, 'tail');
			const before = (await row(page)).slice(5);
			assert.deepEqual(before.map((c) => c.title), [
				'align to the start (left)',
				'align to the center (center)',
				'align to the end (right)'
			]);
			assert.ok(before.every((c) => c.bar.width > c.bar.height), 'along a top bar the marks lie across');
			await barMenu(page).locator('[aria-label="right"]').click();
			await page.waitForFunction(() => !document.querySelector('[role="menu"]'));
			assert.match(await pane(page, 'tail').getAttribute('class'), /\bedge-right\b/);

			await openBar(page, 'tail');
			const cells = await row(page);
			assert.deepEqual(checked(cells), ['right', 'start']);
			const after = cells.slice(5);
			assert.deepEqual(after.map((c) => c.title), [
				'align to the start (top)',
				'align to the center (middle)',
				'align to the end (bottom)'
			]);
			assert.ok(after.every((c) => c.bar.height > c.bar.width), 'along a right bar the marks stand up');
			assert.ok(after[0].bar.y < after[2].bar.y, 'start is drawn above end');
			await page.keyboard.press('Escape');
			await page.keyboard.press('Escape');
		}, DESKTOP);
	});

	// The left-hand bar is turned round (it reads up), so its start is at
	// the BOTTOM — and its icon says so, where the title really is.
	test('on a left bar the start icon and the title are both at the bottom', async () => {
		await withApp(async ({ page }) => {
			await chooseBar(page, 'tail', 'left');
			const [frame, h2] = await Promise.all([boxOf(pane(page, 'tail')), boxOf(pane(page, 'tail').locator('h2'))]);
			assert.ok(h2.y > frame.y + frame.height / 2, 'align start on a left bar is not at the bottom');
			await openBar(page, 'tail');
			const start = (await row(page))[5];
			assert.equal(start.title, 'align to the start (bottom)');
			assert.ok(start.bar.y >= 8, `the start mark is at y=${start.bar.y}, not at the bottom`);
		}, DESKTOP);
	});

	test('by keyboard: in with ArrowRight, across, Enter chooses; back out with ArrowLeft, Escape, Tab', async () => {
		await withApp(async ({ page }) => {
			const more = pane(page, 'tail').locator('.titlebar button.more');
			const focused = () =>
				page.evaluate(() => document.activeElement.getAttribute('aria-label') ?? document.activeElement.textContent.trim());
			/** From the badge: open, ArrowDown until `Bar` is focused. */
			async function toBar() {
				await page.keyboard.press('Enter');
				for (let i = 0; i < 8 && (await focused()) !== 'Bar'; i++) await page.keyboard.press('ArrowDown');
				assert.equal(await focused(), 'Bar');
			}
			await more.focus();
			await toBar();
			await page.keyboard.press('ArrowRight');
			assert.equal(await focused(), 'top');
			for (let i = 0; i < 3; i++) await page.keyboard.press('ArrowRight');
			assert.equal(await focused(), 'right');
			await page.keyboard.press('Enter');
			assert.match(await pane(page, 'tail').getAttribute('class'), /\bedge-right\b/);
			assert.equal(await page.locator('[role="menu"]').count(), 0);
			assert.ok(await more.evaluate((el) => el === document.activeElement), 'choosing did not hand focus to the badge');

			await toBar();
			await page.keyboard.press('ArrowRight');
			await page.keyboard.press('End');
			assert.equal(await focused(), 'end');
			await page.keyboard.press('ArrowRight');
			assert.equal(await focused(), 'top', 'ArrowRight does not wrap');
			await page.keyboard.press('ArrowLeft');
			assert.equal(await focused(), 'Bar', 'ArrowLeft on the first did not go back');
			assert.equal(await barMenu(page).count(), 0);
			await page.keyboard.press('ArrowRight');
			await page.keyboard.press('Escape');
			assert.equal(await focused(), 'Bar', 'Escape did not go back to Bar');
			await page.keyboard.press('Escape');
			assert.ok(await more.evaluate((el) => el === document.activeElement), 'Escape twice did not reach the badge');

			await toBar();
			await page.keyboard.press('ArrowRight');
			await page.keyboard.press('Tab');
			assert.equal(await page.locator('[role="menu"]').count(), 0, 'Tab left a menu open');
		}, DESKTOP);
	});
});

// The bar, after vividn's first look (da-jgrn): with one tab no tab strip at
// all — the title sits on the frame's outline and the `…` is a badge right
// after it; with more, the strip, and the badge right after the last tab.
describe('the title on the frame', () => {
	/** Does `inner` straddle the top edge of `outer`? */
	const straddlesTop = (inner, outer) => inner.y < outer.y && inner.y + inner.height > outer.y;

	test('one tab: no tab strip, the title on the outline, the … right after it, on the keyboard', async () => {
		await withApp(async ({ page }) => {
			const tail = pane(page, 'tail');
			assert.equal(await tail.locator('[role="tablist"]').count(), 0);
			const [frame, h2, more] = await Promise.all([boxOf(tail), boxOf(tail.locator('h2')), boxOf(tail.locator('button.more'))]);
			assert.ok(straddlesTop(h2, frame), `the title ${JSON.stringify(h2)} is not on the outline of ${JSON.stringify(frame)}`);
			assert.ok(straddlesTop(more, frame), `the badge ${JSON.stringify(more)} is not on the outline`);
			const apart = more.x - (h2.x + h2.width);
			assert.ok(apart >= 0 && apart <= 8, `the badge is ${apart}px from the title`);
			assert.ok(more.width >= 24 && more.height >= 24, `the badge is ${more.width}x${more.height}`);
			await tail.locator('button.more').focus();
			await page.keyboard.press('Enter');
			await page.locator('[role="menu"]').waitFor();
			await page.keyboard.press('Escape');
		}, DESKTOP);
	});

	test('two tabs: the strip, and the … right after the last tab', async () => {
		await withApp(async ({ page }) => {
			await choose(page, 'tail', 'Add tab');
			const tail = pane(page, 'tail');
			const tabs = tail.locator('[role="tablist"] [role="tab"]');
			assert.equal(await tabs.count(), 2);
			const [frame, last, more] = await Promise.all([boxOf(tail), boxOf(tabs.last()), boxOf(tail.locator('button.more'))]);
			assert.ok(straddlesTop(last, frame), 'the tabs are not on the outline');
			const apart = more.x - (last.x + last.width);
			assert.ok(apart >= 0 && apart <= 8, `the badge is ${apart}px from the last tab`);
			// No h2 to carry the widget's own title, so it is nowhere: not a
			// stray line at the top of the content, under the tabs (da-13bc).
			await tail.locator('[role="tab"]', { hasText: 'tail' }).click();
			const shown = await tail.locator('> .content').evaluate((el) =>
				[...el.querySelectorAll('.hosted-title')].filter((s) => getComputedStyle(s).display !== 'none').length
			);
			assert.equal(shown, 0, "the tail's title shows in its content");
		}, DESKTOP);
	});

	test('on a vertical bar the badge follows the title along it', async () => {
		await withApp(async ({ page }) => {
			const tail = pane(page, 'tail');
			await chooseBar(page, 'tail', 'right');
			let [h2, more] = await Promise.all([boxOf(tail.locator('h2')), boxOf(tail.locator('button.more'))]);
			let apart = more.y - (h2.y + h2.height);
			assert.ok(apart >= 0 && apart <= 8, `right bar, reading down: the badge is ${apart}px below the title`);
			await chooseBar(page, 'tail', 'left');
			[h2, more] = await Promise.all([boxOf(tail.locator('h2')), boxOf(tail.locator('button.more'))]);
			apart = h2.y - (more.y + more.height);
			assert.ok(apart >= 0 && apart <= 8, `left bar, reading up: the badge is ${apart}px above the title`);
		}, DESKTOP);
	});
});

// The margins, after vividn's first look (da-jgrn): the gap between panes
// from 12px to 8px — the floor a finger needs on a border — and the body's
// padding a third off, 0.5rem 0.6rem → 0.33rem 0.4rem. Asserted from the
// computed styles, on a desktop and on a phone.
describe('the margins', () => {
	for (const [name, viewport] of [
		['1280', { width: 1280, height: 900 }],
		['390', { width: 390, height: 844 }]
	]) {
		test(`the gap is 8px and the body padding 0.33rem 0.4rem at ${name}`, async () => {
			await withApp(async ({ page }) => {
				const seen = await page.evaluate(() => {
					const border = document.querySelector('.split .border:not(.static)');
					const b = border.getBoundingClientRect();
					const body = getComputedStyle(document.querySelector('section.pane .body'));
					return {
						gap: Math.min(b.width, b.height),
						padding: [body.paddingTop, body.paddingRight, body.paddingBottom, body.paddingLeft].map(parseFloat)
					};
				});
				assert.equal(seen.gap, 8);
				const rem = await page.evaluate(() => parseFloat(getComputedStyle(document.documentElement).fontSize));
				const near = (a, b) => Math.abs(a - b) < 0.05;
				assert.ok(near(seen.padding[0], 0.33 * rem) && near(seen.padding[2], 0.33 * rem), `top/bottom ${seen.padding}`);
				assert.ok(near(seen.padding[1], 0.4 * rem) && near(seen.padding[3], 0.4 * rem), `left/right ${seen.padding}`);
			}, { viewport });
		});
	}
});

// The three review threads folded in (da-jgrn): Copilot on #167 and #163.
describe('review follow-ups', () => {
	test('a malformed `fields --json` does not throw, and the bar keeps its last good listing', async () => {
		await withApp(async (app) => {
			const { page } = app;
			await app.submit('occur coffee');
			await page.locator('.bar [data-field="coffee"]').waitFor();
			await page.evaluate(() => {
				const previous = window.datum.setRunner((argv, stdin) =>
					argv[0] === 'fields'
						? Promise.resolve({ argv, code: 0, stdout: '{"cut off', stderr: '' })
						: previous(argv, stdin)
				);
			});
			await app.submit('occur tea');
			await app.idle();
			assert.deepEqual(app.errors, []);
			assert.equal(await page.locator('.bar [data-field="coffee"]').count(), 1);
		}, DESKTOP);
	});
});

// vividn's second look (da-2a5y): "There are some container widgets, that
// throw the spacing off, they have their own ... button floating there,
// but aren't really necessary." A pane whose one tab holds a split is a
// box that is not there; with two or more tabs it keeps its strip and its
// `…`, whose menu leaves splitting to the children.
describe('holders', () => {
	test('a holder with one tab draws no bar and no …, beside a widget pane that has both', async () => {
		await withApp(async ({ page }) => {
			const side = page.locator('section.holder[data-pane="side"]');
			assert.equal(await side.count(), 1);
			assert.equal(await side.locator('> .titlebar').count(), 0);
			assert.equal(await side.locator('> .titlebar button.more').count(), 0);
			// The control: the tail, inside it, has its title and its badge.
			const tail = pane(page, 'tail');
			assert.equal(await tail.locator('> .titlebar h2').count(), 1);
			assert.equal(await tail.locator('> .titlebar button.more').count(), 1);
			// And the holder's box is its split's box: no row above it.
			const [holder, split] = await Promise.all([boxOf(side), boxOf(side.locator('> .content > .split'))]);
			assert.equal(Math.round(holder.y), Math.round(split.y), 'the split does not start where the holder does');
			assert.equal(Math.round(holder.height), Math.round(split.height));
		}, DESKTOP);
	});

	test('a holder with two tabs keeps its strip and its …, and its menu offers no Split row', async () => {
		await withApp(async ({ page }) => {
			// A second tab on the tail, then its first tab split: the tail pane
			// now holds a split under one tab and the empty tab beside it.
			await choose(page, 'tail', 'Add tab');
			await pane(page, 'tail').locator('[role="tab"]', { hasText: 'tail' }).click();
			await choose(page, 'tail', 'split tab side by side');
			const holder = page.locator('section.holder[data-pane="tail"]');
			assert.equal(await holder.count(), 1);
			assert.deepEqual(await holder.locator('> .titlebar [role="tab"]').allInnerTexts(), ['split', 'empty']);
			await holder.locator('> .titlebar button.more').click();
			assert.deepEqual(await menuLines(page), ['Add tab', 'Split Tab | −', 'Close tab', 'Bar']);
			await page.keyboard.press('Escape');
		}, DESKTOP);
	});

	// Copilot on #172: a root with two tabs must keep a strip, or nothing
	// could switch them. The shell (da-0a5o) draws the page root's tabs as
	// the root bar, so the page root draws none; a saved layout whose root
	// has two tabs is inlined whole under its root tab, and keeps its strip.
	test("a layout's root with two tabs keeps its strip; the page root's strip is the root bar (Copilot on #172)", async () => {
		await withApp(
			async ({ page }) => {
				assert.equal(await page.locator('section.root > .titlebar').count(), 0, 'the page root drew a strip');
				const inner = pane(page, 'root');
				assert.deepEqual(await inner.locator('> .titlebar [role="tab"]').allInnerTexts(), ['repl', 'tail']);
				await inner.locator('> .titlebar [role="tab"]', { hasText: 'tail' }).click();
				assert.deepEqual(await titles(page), ['tail']);
			},
			{ ...DESKTOP, prepare: (context) => context.addInitScript(draft, TWO_ROOT_TABS) }
		);
	});

	test('the root holder draws no bar: its split starts at the top of the frame', async () => {
		await withApp(async ({ page }) => {
			const root = page.locator('section.root');
			assert.equal(await root.locator('> .titlebar').count(), 0);
			const [frame, split] = await Promise.all([boxOf(root), boxOf(root.locator('> .content > .split'))]);
			assert.equal(Math.round(frame.y), Math.round(split.y));
		}, DESKTOP);
	});
});

// The lock (da-2a5y): "Layouts should have the ability to be locked,
// which prevents the resizing of panes […] no splitting, new tabs, or
// resizing." Where the lock is toggled from is the shell's; here it
// arrives as this device's lock on the `day` tab (per device, never in
// the tree — da-0a5o), and covers the whole tree.
describe('the lock', () => {

	/** The cells of the row split, by width. */
	const cellWidths = (page) =>
		page.evaluate(() => [...document.querySelectorAll('.split.row > .cell')].map((c) => c.getBoundingClientRect().width));

	test('a locked root: no separators, no …, a drag and an arrow move nothing, the tabs still switch', async () => {
		await withApp(
			async (app) => {
				const { page } = app;
				assert.deepEqual(await titles(page), ['repl', 'tail']);
				assert.equal(await page.locator('[role="separator"]').count(), 0);
				// The root bar's `+` and `…` (da-0a5o) are above the layout, not under its lock.
				assert.equal(await page.locator('section.root button.more').count(), 0);
				// The gap is still there, as a static border, and dragging across it moves nothing.
				const gap = page.locator('.split.row > .border.static');
				assert.equal(await gap.count(), 1);
				assert.equal(await gap.evaluate((el) => getComputedStyle(el).cursor), 'default');
				const was = await cellWidths(page);
				const box = await gap.boundingBox();
				const x = box.x + box.width / 2;
				const y = box.y + box.height / 2;
				await page.mouse.move(x, y);
				await page.mouse.down();
				for (let i = 1; i <= 5; i++) await page.mouse.move(x + i * 30, y);
				await page.mouse.up();
				await page.waitForTimeout(200);
				assert.deepEqual(await cellWidths(page), was, 'a drag across a locked border moved it');
				await page.evaluate(() => document.body.focus());
				await page.keyboard.press('ArrowRight');
				await page.waitForTimeout(200);
				assert.deepEqual(await cellWidths(page), was, 'an arrow moved a locked border');
				// Tabs are not an edit: the tabbed pane still switches.
				await pane(page, 'tabbed').locator('[role="tab"]', { hasText: 'nowview' }).click();
				assert.equal(await activeTab(page, 'tabbed'), 'nowview');
				assert.deepEqual(app.errors, []);

				// Unlocked, the controls are back — the one border, and a badge on
				// each widget pane (the root has none of its own).
				await page.evaluate(() => localStorage.removeItem('datum.layout.locked.v1.day'));
				await app.reload();
				assert.equal(await page.locator('.split.row > [role="separator"]').count(), 1);
				assert.equal(await pane(page, 'repl').locator('button.more').count(), 1);
				assert.equal(await pane(page, 'tabbed').locator('button.more').count(), 1);
				assert.equal(await page.locator('section.root button.more').count(), 2);
			},
			// The init script runs on every load, the reload included: it
			// writes the draft and the lock once and leaves the unlocked one alone.
			{ ...DESKTOP, prepare: (context) => context.addInitScript(lockedOnce, JSON.stringify(lockable())) }
		);
	});
});

// The scrollbars (da-2a5y): "The built in ones look terrible with the
// design, they should be more very tiny, invisible when not in use, and
// match the frame color." One rule for every scroll container, in app.css.
describe('scrollbars', () => {
	for (const [name, viewport] of [
		['1280', { width: 1280, height: 900 }],
		['390', { width: 390, height: 844 }]
	]) {
		test(`are thin on a pane's body and on the root scroller at ${name}`, async () => {
			await withApp(async ({ page }) => {
				const seen = await page.evaluate(() => {
					const style = (el) => {
						const s = getComputedStyle(el);
						return { width: s.scrollbarWidth, color: s.scrollbarColor };
					};
					return {
						body: style(document.querySelector('section.pane .body')),
						root: style(document.querySelector('section.root > .content'))
					};
				});
				assert.equal(seen.body.width, 'thin', `the body: ${JSON.stringify(seen.body)}`);
				assert.equal(seen.root.width, 'thin', `the root scroller: ${JSON.stringify(seen.root)}`);
				// No pointer over it: the thumb is transparent.
				assert.match(seen.body.color, /^rgba\(0, 0, 0, 0\) rgba\(0, 0, 0, 0\)$|^transparent transparent$/);
			}, { viewport });
		});
	}
});

/**
 * A top-bar widget pane's frame (Layout.svelte's `chromeOf`, da-13bc):
 * the outline, and the inside half of the title's row across the top.
 */
const CHROME = { width: 2, height: 2 + 12 };
/** The strip body's padding as the strip's snap counts it (registry.ts). */
const PADDING = 16;
const todayBox = (dayHeight) => dayHeight + 2 * 2 + 15; // panes.ts: the day, datum's margins and the hour axis

/** Press the pointer on the middle of `border`; the point pressed. */
async function grab(page, border) {
	const box = await border.boundingBox();
	const at = { x: box.x + box.width / 2, y: box.y + box.height / 2 };
	await page.mouse.move(at.x, at.y);
	await page.mouse.down();
	return at;
}

/** The extent, in px, of each cell of the first split along `direction`. */
const cellExtents = (page, direction) =>
	page.evaluate(
		(d) => [...document.querySelectorAll(`.split.${d} > .cell`)].map((c) => c.getBoundingClientRect()[d === 'row' ? 'width' : 'height']),
		direction
	);

// vividn's third look (da-zh2o): "let widgets get a lot smaller, until the
// display is gone and just a frame bar remains". Every kind a border can
// be dragged beside (the widget bar sizes itself, and the border beside
// it is no handle) shrinks to its frame, down a column AND across a row:
// the section is exactly the frame, and nothing of the widget is painted
// past it. The title lies OVER the content (da-13bc), so what is left of
// the content box is the inside half of the title's row — 12px down a
// column under a top bar, 0px across a row.
describe('a widget pane shrinks to its frame', () => {
	for (const kind of ['daystrip', 'repl', 'nowview', 'tail', 'command', 'stats']) {
		for (const direction of ['column', 'row']) {
			const axis = direction === 'row' ? 'width' : 'height';
			test(`${kind}, across a ${direction}: only its frame is left, ${CHROME[axis]}px ${axis}`, async () => {
				await withApp(
					async ({ page }) => {
						const at = await grab(page, page.locator(`.split.${direction} > [role="separator"]`));
						for (let i = 1; i <= 20; i++) {
							await page.mouse.move(direction === 'row' ? at.x - i * 80 : at.x, direction === 'row' ? at.y : at.y - i * 60);
						}
						await page.mouse.up();
						await page.waitForTimeout(300);
						const seen = await pane(page, 'a').evaluate((section, row) => {
							const content = section.querySelector(':scope > .content');
							const s = getComputedStyle(content);
							const b = section.getBoundingClientRect();
							// Halfway into the border gap past the frame, at every layer: nothing of the widget is there.
							const past = row ? [b.right + 4, b.top + b.height / 2] : [b.left + b.width / 2, b.bottom + 4];
							return {
								spills: document.elementsFromPoint(...past).some((el) => section.contains(el)),
								section: b[row ? 'width' : 'height'],
								content: row
									? content.clientWidth - parseFloat(s.paddingLeft) - parseFloat(s.paddingRight)
									: content.clientHeight - parseFloat(s.paddingTop) - parseFloat(s.paddingBottom)
							};
						}, direction === 'row');
						assert.ok(Math.abs(seen.content - (CHROME[axis] - 2)) <= 1, `${kind}: the content box is ${seen.content}px ${axis}, not the title's inside half`);
						assert.ok(!seen.spills, `${kind}: the widget paints outside its frame`);
						assert.ok(Math.abs(seen.section - CHROME[axis]) <= 1, `${kind}: the frame is ${seen.section}px ${axis}, not ${CHROME[axis]}`);
					},
					{ ...DESKTOP, prepare: (context) => context.addInitScript(draft, JSON.stringify(along(direction, widgetPane('a', kind), widgetPane('b', 'repl')))) }
				);
			});
		}
	}
});

// vividn's third look (da-13bc): "The pane tile currently still pushes the
// contents of the pane down, just overlay it directly on the frame (please
// center on the frame line)" — and, from the floor, "when dayview has the
// label on the left and right, the minimum is no longer the day height.
// but slightly larger". Measured in screen coordinates, on all four edges
// (the left-hand bar is rotated), never "less than before".
describe('the title on the line', () => {
	for (const edge of ['top', 'bottom', 'left', 'right']) {
		test(`bar on the ${edge}: the content has no padding and the title is centred on the outline`, async () => {
			await withApp(async ({ page }) => {
				if (edge !== 'top') await chooseBar(page, 'tail', edge);
				const tail = pane(page, 'tail');
				const seen = await tail.evaluate((section) => {
					const s = getComputedStyle(section.querySelector(':scope > .content'));
					const f = section.getBoundingClientRect();
					const h = section.querySelector('h2').getBoundingClientRect();
					return {
						padding: [s.paddingTop, s.paddingRight, s.paddingBottom, s.paddingLeft],
						// The 1px outline's own middle, on each edge.
						line: { top: f.top + 0.5, bottom: f.bottom - 0.5, left: f.left + 0.5, right: f.right - 0.5 },
						middle: { x: h.left + h.width / 2, y: h.top + h.height / 2 }
					};
				});
				assert.deepEqual(seen.padding, ['0px', '0px', '0px', '0px']);
				const at = edge === 'top' || edge === 'bottom' ? seen.middle.y : seen.middle.x;
				assert.ok(Math.abs(at - seen.line[edge]) <= 1, `the title's middle is at ${at}, the ${edge} line at ${seen.line[edge]}`);
			}, DESKTOP);
		});
	}

	test("a title on the page's top edge is whole, and the panes start where the page's margin ends", async () => {
		await withApp(async ({ page }) => {
			const seen = await page.evaluate(() => {
				const main = document.querySelector('main');
				const h2 = document.querySelector('section[data-pane="strip"] h2');
				const h = h2.getBoundingClientRect();
				return {
					clip: document.querySelector('section.root > .content').getBoundingClientRect().top,
					title: h.top,
					// The title's topmost row of pixels is the title's, not cut away.
					hit: document.elementFromPoint(h.left + h.width / 2, h.top + 1) === h2,
					pane: document.querySelector('section[data-pane="strip"]').getBoundingClientRect().top,
					margin: main.getBoundingClientRect().top + parseFloat(getComputedStyle(main).paddingTop)
				};
			});
			assert.ok(seen.title >= seen.clip, `the title starts at ${seen.title}, the scroller cuts at ${seen.clip}`);
			assert.ok(seen.hit, 'the top of the title is covered');
			assert.equal(seen.pane, seen.margin);
		}, DESKTOP);
	});

	// Its kind has no floor (da-zh2o), so what is left of a strip dragged
	// shut is its frame, and the frame follows the bar's edge: a left-hand
	// title adds width, not height, so that strip keeps only its outline.
	test('dragged shut, a left-bar strip keeps its outline and a top-bar one 12px more', async () => {
		await withApp(
			async ({ page }) => {
				const borders = page.locator('.split.column > [role="separator"]');
				for (const i of [0, 1]) {
					const at = await grab(page, borders.nth(i));
					for (let step = 1; step <= 10; step++) await page.mouse.move(at.x, at.y - step * 60);
					await page.mouse.up();
					await page.waitForTimeout(300);
				}
				const [left, top] = await cellExtents(page, 'column');
				assert.ok(Math.abs(left - 2) <= 0.5, `the left-bar strip stopped at ${left}px, its outline is 2px`);
				assert.ok(Math.abs(top - CHROME.height) <= 0.5, `the top-bar strip stopped at ${top}px, its frame is ${CHROME.height}px`);
			},
			{ ...DESKTOP, prepare: (context) => context.addInitScript(draft, JSON.stringify(twoStrips(85))) }
		);
	});
});

// "bar submenu should open immediately on mouse over" and "let's put the
// two split buttons on the same line" (da-13bc).
describe('the … menu by pointer', () => {
	const barItem = (page) => page.getByRole('menuitem', { name: /^Bar/ });

	test('Bar ▸ opens on the pointer with no click, stays open on the way to it, and closes on another item', async () => {
		await withApp(async ({ page }) => {
			await pane(page, 'tail').locator('.titlebar button.more').click();
			await barItem(page).hover();
			await barMenu(page).waitFor();
			assert.equal(await barItem(page).getAttribute('aria-expanded'), 'true');
			await page.getByRole('menuitem', { name: 'Close pane', exact: true }).hover();
			await barMenu(page).waitFor({ state: 'detached' });

			// From the item onto the submenu, in small steps across the gap.
			const from = await boxOf(barItem(page));
			await page.mouse.move(from.x + from.width / 2, from.y + from.height / 2);
			const icon = barMenu(page).locator('[aria-label="bottom"]');
			await icon.waitFor();
			const to = await boxOf(icon);
			await page.mouse.move(to.x + to.width / 2, to.y + to.height / 2, { steps: 12 });
			assert.equal(await barMenu(page).count(), 1, 'the submenu closed on the way to it');
			await page.mouse.down();
			await page.mouse.up();
			assert.match(await pane(page, 'tail').getAttribute('class'), /\bedge-bottom\b/);
		}, DESKTOP);
	});

	// A finger landing on `Bar` enters it too — Chromium fires a touch
	// `pointerenter` at the touch's start — and that must not open the
	// submenu; the tap does (Copilot on #176). A real touch, through CDP.
	test('a touch entering Bar ▸ does not open it; the tap does', async () => {
		await withApp(async ({ page }) => {
			await pane(page, 'repl').locator('.titlebar button.more').tap();
			const item = barItem(page);
			await item.evaluate((el) => {
				window.__entered = [];
				el.addEventListener('pointerenter', (e) => window.__entered.push(e.pointerType));
			});
			const box = await boxOf(item);
			const at = { x: box.x + box.width / 2, y: box.y + box.height / 2 };
			const cdp = await page.context().newCDPSession(page);
			await cdp.send('Input.dispatchTouchEvent', { type: 'touchStart', touchPoints: [at] });
			await page.waitForTimeout(200);
			assert.deepEqual(await page.evaluate(() => window.__entered), ['touch'], 'the touch did not enter Bar');
			assert.equal(await barMenu(page).count(), 0, 'a touch entering Bar opened it');
			await cdp.send('Input.dispatchTouchEvent', { type: 'touchEnd', touchPoints: [] });
			await cdp.detach();
			await barMenu(page).waitFor();
		}, PHONE);
	});

	// Copilot on #176: the row's own arrows, to both of its ends.
	test('ArrowRight and ArrowLeft walk the Split row and stop at its ends', async () => {
		await withApp(async ({ page }) => {
			const focused = () =>
				page.evaluate(() => document.activeElement.getAttribute('aria-label') ?? document.activeElement.textContent.trim());
			await pane(page, 'tail').locator('.titlebar button.more').focus();
			await page.keyboard.press('Enter');
			await page.keyboard.press('ArrowDown');
			assert.equal(await focused(), 'split side by side');
			const walk = [];
			for (const key of ['ArrowLeft', 'ArrowRight', 'ArrowRight', 'ArrowLeft', 'ArrowLeft']) {
				await page.keyboard.press(key);
				walk.push(await focused());
			}
			assert.deepEqual(walk, [
				'split side by side',
				'split top and bottom',
				'split top and bottom',
				'split side by side',
				'split side by side'
			]);
			// And ArrowDown from the row's second button goes on to the next item.
			await page.keyboard.press('ArrowRight');
			await page.keyboard.press('ArrowDown');
			assert.equal(await focused(), 'Close pane');
		}, DESKTOP);
	});

	test('Split is one line of two buttons, and − splits top and bottom', async () => {
		await withApp(async ({ page }) => {
			await pane(page, 'tail').locator('.titlebar button.more').click();
			const buttons = page.getByRole('group', { name: 'Split' }).getByRole('menuitem');
			assert.deepEqual(await buttons.evaluateAll((all) => all.map((b) => b.getAttribute('aria-label'))), [
				'split side by side',
				'split top and bottom'
			]);
			const [side, stack] = await Promise.all([boxOf(buttons.nth(0)), boxOf(buttons.nth(1))]);
			assert.equal(side.y, stack.y, 'the two are not on one line');
			for (const b of [side, stack]) assert.ok(b.width >= 24 && b.height >= 24, `a split button is ${b.width}x${b.height}`);
			await buttons.nth(1).click();
			await page.locator('[role="menu"]').waitFor({ state: 'detached' });
			assert.deepEqual(await titles(page), ['dayview', 'repl', 'nowview', 'tail', 'empty', 'command', 'fields']);
			const [tail, empty] = await Promise.all([boxOf(pane(page, 'tail')), boxOf(page.locator('section.pane[data-title="empty"]'))]);
			assert.equal(Math.round(empty.x), Math.round(tail.x));
			assert.equal(Math.round(empty.width), Math.round(tail.width));
			assert.ok(empty.y > tail.y + tail.height, 'the new pane is not under the tail');
		}, DESKTOP);
	});
});

/** A root column of [a left-bar strip / a top-bar strip / the REPL] at `dayHeight`. */
const twoStrips = (dayHeight) => ({
	version: 1,
	name: 'default',
	root: {
		id: 'root',
		tabs: [
			{
				id: 'root-tab',
				content: {
					id: 'col',
					direction: 'column',
					ratios: [0.34, 0.33, 0.33],
					children: [
						widgetPane('left', 'daystrip', { dayHeight }, 'left'),
						widgetPane('top', 'daystrip', { dayHeight }),
						widgetPane('repl', 'repl')
					]
				}
			}
		],
		active: 0,
		bar
	}
});

// "let the height of the dayview get smaller than 1 day, but put a
// snapping notch at exactly one day" (da-zh2o). The strip's snap is one
// whole day at its day height, plus its padding and its frame: a pointer
// within 8px of it lands the border ON it, 12px beyond goes past, and the
// drop keeps what the drag showed. Measured on the PREVIEW, button down.
describe("the strip's notch", () => {
	test('with 85px days the border above the strip lands on one day within ±8px, and passes 12px beyond', async () => {
		await withApp(
			async ({ page }) => {
				const snap = todayBox(85) + PADDING + CHROME.height;
				const [, strip] = await cellExtents(page, 'column');
				const at = await grab(page, page.locator('.split.column > [role="separator"]'));
				// Moving the border down by d shrinks the strip below it by d.
				const toward = async (extent) => {
					await page.mouse.move(at.x, at.y + strip - extent);
					return (await cellExtents(page, 'column'))[1];
				};
				for (const off of [8, 4, 0, -4, -8]) {
					const shown = await toward(snap + off);
					assert.ok(Math.abs(shown - snap) <= 0.5, `a pointer ${off}px off the notch showed the strip at ${shown}px, not ${snap}`);
				}
				for (const off of [12, -12]) {
					const shown = await toward(snap + off);
					assert.ok(Math.abs(shown - (snap + off)) <= 1, `a pointer ${off}px off the notch showed the strip at ${shown}px, not ${snap + off}`);
				}
				await toward(snap + 5);
				await page.mouse.up();
				await page.waitForTimeout(300);
				const dropped = (await cellExtents(page, 'column'))[1];
				assert.ok(Math.abs(dropped - snap) <= 0.5, `the drop kept ${dropped}px, not the ${snap}px the drag showed`);
			},
			{ ...DESKTOP, prepare: (context) => context.addInitScript(draft, JSON.stringify(along('column', widgetPane('repl', 'repl'), widgetPane('strip', 'daystrip', { dayHeight: 85 })))) }
		);
	});

	test('two strips in one column each snap to their own day height, 85px and 120px', async () => {
		await withApp(
			async ({ page }) => {
				const [top] = await cellExtents(page, 'column');
				// The border between the two strips; the REPL below them is what makes the app ready.
				const at = await grab(page, page.locator('.split.column > [role="separator"]').first());
				const snapA = todayBox(85) + PADDING + CHROME.height;
				await page.mouse.move(at.x, at.y - (top - snapA) + 6);
				const [a] = await cellExtents(page, 'column');
				assert.ok(Math.abs(a - snapA) <= 0.5, `the 85px strip showed ${a}px, not ${snapA}`);
				const snapB = todayBox(120) + PADDING + CHROME.height;
				const [, bottom] = await cellExtents(page, 'column');
				// The pointer now: at the notch A snapped to, and B is what is left.
				await page.mouse.move(at.x, at.y - (top - snapA) + (bottom - snapB) - 6);
				const [, b] = await cellExtents(page, 'column');
				assert.ok(Math.abs(b - snapB) <= 0.5, `the 120px strip showed ${b}px, not ${snapB}`);
				await page.mouse.up();
			},
			{
				...DESKTOP,
				prepare: (context) =>
					context.addInitScript(draft, JSON.stringify(along('column', widgetPane('a', 'daystrip', { dayHeight: 85 }), widgetPane('b', 'daystrip', { dayHeight: 120 }), widgetPane('repl', 'repl'))))
			}
		);
	});

	// Shorter than one day the strip clips today's band from the top and
	// keeps its hour axis on screen, and the pane's body does not scroll;
	// the day-height notch is clamped into the box, so it can still be had.
	test('in a pane 60px tall the strip clips today\'s band, keeps its axis, and the body does not scroll', async () => {
		await withApp(
			async ({ page }) => {
				const [, strip] = await cellExtents(page, 'column');
				const at = await grab(page, page.locator('.split.column > [role="separator"]'));
				await page.mouse.move(at.x, at.y + strip - 60);
				await page.mouse.up();
				await page.waitForTimeout(300);
				assert.ok(Math.abs((await cellExtents(page, 'column'))[1] - 60) <= 1);
				await pane(page, 'strip').locator('.band.today svg').waitFor();
				const seen = await pane(page, 'strip').evaluate((section) => {
					const box = (s) => section.querySelector(s).getBoundingClientRect();
					const body = section.querySelector('.body');
					return {
						scrolls: body.scrollHeight - body.clientHeight,
						strip: box('.strip'),
						today: box('.band.today'),
						grip: box('.day-grip')
					};
				});
				assert.ok(seen.scrolls <= 0, `the body scrolls by ${seen.scrolls}px`);
				assert.ok(seen.today.top < seen.strip.top - 1, 'today\'s band is not clipped at the top');
				assert.ok(Math.abs(seen.today.bottom - seen.strip.bottom) <= 1, 'today\'s band, and its axis, left the bottom of the strip');
				assert.ok(seen.grip.top >= seen.strip.top - 0.5 && seen.grip.bottom <= seen.strip.bottom, 'the day-height notch is outside the strip');
			},
			{ ...DESKTOP, prepare: (context) => context.addInitScript(draft, JSON.stringify(along('column', widgetPane('repl', 'repl'), widgetPane('strip', 'daystrip')))) }
		);
	});
});

const bar = { edge: 'top', align: 'start' };
const widgetPane = (id, kind, settings = {}, edge = 'top') => ({
	id,
	tabs: [{ id: `${id}-tab`, content: { id: `${id}-widget`, kind, settings } }],
	active: 0,
	bar: { ...bar, edge }
});

/** A root holder over [repl | a tabbed tail/nowview pane]. */
function lockable() {
	const tab = {
		id: 'root-tab',
		content: {
			id: 'row',
			direction: 'row',
			ratios: [0.5, 0.5],
			children: [
				widgetPane('repl', 'repl'),
				{
					id: 'tabbed',
					tabs: [
						{ id: 'tabbed-tail', content: { id: 'tabbed-tail-widget', kind: 'tail', settings: {} } },
						{ id: 'tabbed-nowview', content: { id: 'tabbed-nowview-widget', kind: 'nowview', settings: {} } }
					],
					active: 0,
					bar
				}
			]
		}
	};
	return { version: 1, name: 'day', root: { id: 'root', tabs: [tab], active: 0, bar } };
}

/** A root holder over `children` along `direction`, in equal shares. */
const along = (direction, ...children) => ({
	version: 1,
	name: 'default',
	root: {
		id: 'root',
		tabs: [{ id: 'root-tab', content: { id: 'split', direction, ratios: children.map(() => 1 / children.length), children } }],
		active: 0,
		bar
	}
});

/** A root pane with two widget tabs, the REPL and the tail. */
const TWO_ROOT_TABS = JSON.stringify({
	version: 1,
	name: 'default',
	root: {
		id: 'root',
		tabs: [
			{ id: 'root-repl', content: { id: 'root-repl-widget', kind: 'repl', settings: {} } },
			{ id: 'root-tail', content: { id: 'root-tail-widget', kind: 'tail', settings: {} } }
		],
		active: 0,
		bar
	}
});

/** A draft of one REPL pane for the `day` tab, written before the app loads. */
const ONE_PANE = JSON.stringify({
	version: 1,
	name: 'day',
	root: {
		id: 'only',
		tabs: [{ id: 'only-tab', content: { id: 'only-widget', kind: 'repl', settings: {} } }],
		active: 0,
		bar: { edge: 'top', align: 'start' }
	}
});

function draft(json) {
	localStorage.setItem('datum.layout.draft.v1.day', json);
}

function lockedOnce(json) {
	if (localStorage.getItem('datum.layout.draft.v1.day')) return;
	localStorage.setItem('datum.layout.draft.v1.day', json);
	localStorage.setItem('datum.layout.locked.v1.day', '1');
}
