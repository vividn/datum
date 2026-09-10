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

/** Open a pane's `…` menu and choose an item by its exact label. */
async function choose(page, id, label) {
	await pane(page, id).locator('.titlebar button.more').first().click();
	await page.locator('[role="menu"] button').filter({ hasText: exactly(label) }).click();
	await page.locator('[role="menu"]').waitFor({ state: 'detached' });
}

/** A menu item's whole label — a checked one carries a ✓ before it. */
const exactly = (label) =>
	new RegExp(`^\\s*✓?\\s*${label.replace(/[.*+?^${}()|[\]\\]/g, '\\$&')}\\s*$`);

/** The box of a pane's picture, as the picture says it (`naturalWidth`). */
const canvasWidth = (page, id) =>
	pane(page, id).locator('img.svg').evaluate((img) => img.naturalWidth);

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
			assert.deepEqual(await titles(page), ['dayview', 'repl', 'nowview', 'tail', 'command', 'widgets']);
		}, DESKTOP);
	});

	test('a command through the REPL still reaches tail and nowview', async () => {
		await withApp(async (app) => {
			await app.submit('switch mood happy');
			await app.page.waitForFunction(
				() =>
					document.querySelector('section[data-pane="tail"] .body')?.textContent.includes('mood') &&
					decodeURIComponent(
						document.querySelector('section[data-pane="nowview"] img.svg')?.getAttribute('src') ?? ''
					).includes('data-field="mood"')
			);
		}, DESKTOP);
	});
});

describe('the … menu', () => {
	test('Split | on the tail pane makes two panes, the new one a picker, and the pick renders', async () => {
		await withApp(async ({ page }) => {
			await choose(page, 'tail', 'Split |');
			assert.deepEqual(await titles(page), ['dayview', 'repl', 'nowview', 'tail', 'empty', 'command', 'widgets']);
			const picker = page.locator('section.pane[data-title="empty"] .picker');
			assert.deepEqual(await picker.locator('button').allInnerTexts(), [
				'dayview',
				'repl',
				'nowview',
				'tail',
				'command',
				'stats',
				'widgets'
			]);
			await picker.locator('button', { hasText: 'nowview' }).click();
			// The new pane is titled by its kind and draws: a second nowview
			// beside the tail, at its own box's size.
			await page.locator('section.pane[data-title="nowview"]').nth(1).locator('img.svg').waitFor();
			assert.deepEqual(await titles(page), ['dayview', 'repl', 'nowview', 'tail', 'nowview', 'command', 'widgets']);
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
			assert.equal(
				await page.locator('[role="menu"] button').filter({ hasText: exactly('Split Tab |') }).count(),
				1
			);
			await page.keyboard.press('Escape');
		}, DESKTOP);
	});

	test('Split Tab is not offered with one tab', async () => {
		await withApp(async ({ page }) => {
			await pane(page, 'tail').locator('.titlebar button.more').click();
			assert.equal(await page.locator('[role="menu"] button').filter({ hasText: /Split Tab/ }).count(), 0);
			await page.keyboard.press('Escape');
		}, DESKTOP);
	});

	test('Bar: left turns the title vertical, reading up; Bar: right reads down', async () => {
		await withApp(async ({ page }) => {
			await choose(page, 'tail', 'Bar: left');
			const h2 = pane(page, 'tail').locator('h2');
			assert.equal(await h2.evaluate((el) => getComputedStyle(el).writingMode), 'vertical-rl');
			const bar = pane(page, 'tail').locator('.titlebar');
			assert.notEqual(await bar.evaluate((el) => getComputedStyle(el).transform), 'none', 'turned round');
			const [barBox, bodyBox] = await Promise.all([
				bar.boundingBox(),
				pane(page, 'tail').locator('.body').boundingBox()
			]);
			assert.ok(barBox.x + barBox.width <= bodyBox.x + 1, 'the bar is on the left of the content');

			await choose(page, 'tail', 'Bar: right');
			assert.equal(await h2.evaluate((el) => getComputedStyle(el).writingMode), 'vertical-rl');
			assert.equal(await bar.evaluate((el) => getComputedStyle(el).transform), 'none');
			const [right, body] = await Promise.all([
				bar.boundingBox(),
				pane(page, 'tail').locator('.body').boundingBox()
			]);
			assert.ok(right.x >= body.x + body.width - 1, 'the bar is on the right of the content');
		}, DESKTOP);
	});

	test('Close pane is disabled on the last pane', async () => {
		await withApp(
			async ({ page }) => {
				assert.deepEqual(await titles(page), ['repl']);
				await page.locator('.titlebar button.more').click();
				const close = page.locator('[role="menu"] button').filter({ hasText: exactly('Close pane') });
				assert.equal(await close.isDisabled(), true);
				await page.keyboard.press('Escape');
			},
			{ ...DESKTOP, prepare: (context) => context.addInitScript(draft, ONE_PANE) }
		);
	});

	test('closing panes unnests their split', async () => {
		await withApp(async ({ page }) => {
			await choose(page, 'command', 'Close pane');
			assert.deepEqual(await titles(page), ['dayview', 'repl', 'nowview', 'tail', 'widgets']);
			await choose(page, 'nowview', 'Close pane');
			assert.deepEqual(await titles(page), ['dayview', 'repl', 'tail', 'widgets']);
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
				(was) => document.querySelector('section[data-pane="nowview"] img.svg')?.naturalWidth < was,
				drawn
			);
			const img = pane(page, 'nowview').locator('img.svg');
			const shown = await img.evaluate((el) => ({ natural: el.naturalWidth, box: el.getBoundingClientRect().width }));
			assert.ok(Math.abs(shown.natural - shown.box) <= 1, `scaled: ${JSON.stringify(shown)}`);
		}, DESKTOP);
	});

	test('will not drag a child below its floor', async () => {
		await withApp(async ({ page }) => {
			const border = page.locator('.split.row > [role="separator"]');
			const box = await border.boundingBox();
			const x = box.x + box.width / 2;
			const y = box.y + box.height / 2;
			await page.mouse.move(x, y);
			await page.mouse.down();
			for (let i = 1; i <= 10; i++) await page.mouse.move(x + i * 80, y);
			await page.mouse.up();
			await page.waitForTimeout(300);
			const side = await pane(page, 'side').boundingBox();
			// nowview and tail both floor at 200px of content plus the frame.
			assert.ok(side.width >= 200, `the side column was dragged to ${side.width}px`);
		}, DESKTOP);
	});
});

describe('the draft', () => {
	test('is back after a reload, and a layout with no draft is the default', async () => {
		await withApp(async (app) => {
			const { page } = app;
			await choose(page, 'tail', 'Split |');
			await page.locator('.picker button', { hasText: 'nowview' }).click();
			await choose(page, 'strip', 'Bar: left');
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
				assert.deepEqual(await titles(app.page), ['dayview', 'repl', 'nowview', 'tail', 'command', 'widgets']);
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
			assert.deepEqual(await titles(page), ['dayview', 'repl', 'tail', 'widgets']);
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
			await pane(page, 'side').locator('img.svg').waitFor();
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

/** The bands the strip is showing — one `dayview` run per redraw. */
const bandCount = (page) => pane(page, 'strip').locator('img.svg').count();

/** A drawn pane's content box, whole pixels down, as `paneArgv` asks for it. */
const contentWidth = (page, id) =>
	pane(page, id)
		.locator('.body')
		.evaluate((el) => {
			const s = getComputedStyle(el);
			return Math.floor(el.clientWidth - parseFloat(s.paddingLeft) - parseFloat(s.paddingRight));
		});

const boxOf = (locator) => locator.boundingBox();

// vividn's first look (da-jgrn): dragging a border was jittery, because every
// drawn pane re-ran its CLI command at every size it passed through. Now a
// drag re-runs nothing until the drop, and a burst of nudges is one redraw.
// Counted through the seam, never eyeballed.
describe('resizing', () => {
	test('a border dragged 200px over a second redraws the drawn panes once, on the drop', async () => {
		await withApp(async (app) => {
			const { page } = app;
			const seen = await app.spyOnRunner();
			const border = page.locator('.split.row > [role="separator"]');
			const box = await border.boundingBox();
			const x = box.x + box.width / 2;
			const y = box.y + box.height / 2;
			await page.mouse.move(x, y);
			await page.mouse.down();
			const steps = 60;
			for (let i = 1; i <= steps; i++) {
				await page.mouse.move(x + (200 * i) / steps, y);
				await page.waitForTimeout(16);
			}
			const during = drawnRuns(await seen());
			assert.ok(during.nowview <= 2, `nowview ran ${during.nowview} times during the drag`);
			assert.ok(during.dayview <= 2, `dayview ran ${during.dayview} times during the drag`);
			// Meanwhile the picture is still up, scaled to its box — never blank.
			assert.ok((await canvasWidth(page, 'nowview')) > 0, 'the picture went away during the drag');

			await page.mouse.up();
			const dropped = Date.now();
			// The redraw is asked for on the drop itself, not after a delay.
			await page.waitForFunction(
				(before) => window.__datumSeen.filter((a) => a[0] === 'nowview').length > before,
				during.nowview
			);
			assert.ok(Date.now() - dropped <= 200, `the redraw came ${Date.now() - dropped}ms after the drop`);
			await app.idle();

			const after = drawnRuns(await seen());
			assert.equal(after.nowview - during.nowview, 1, 'nowview redraws once on the drop');
			// The strip spans the page above this split: its box did not move,
			// so it has nothing to redraw — and redraws nothing.
			assert.equal(after.dayview - during.dayview, 0, 'the strip redrew for a border that did not touch it');
			// And the picture is the box's size: asked for at the final width,
			// not the old one stretched to it.
			assert.equal(await canvasWidth(page, 'nowview'), await contentWidth(page, 'nowview'));
			const img = pane(page, 'nowview').locator('img.svg');
			const shown = await img.evaluate((el) => ({ natural: el.naturalWidth, box: el.getBoundingClientRect().width }));
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

	test('the strip, dragged taller over a second, draws its new bands once, on the drop', async () => {
		await withApp(async (app) => {
			const { page } = app;
			const seen = await app.spyOnRunner();
			const before = await bandCount(page);
			// The root's column border, under the strip: down is a taller strip.
			const border = page.locator('section.root > .content > .split.column > [role="separator"]').first();
			const box = await border.boundingBox();
			const x = box.x + box.width / 2;
			const y = box.y + box.height / 2;
			await page.mouse.move(x, y);
			await page.mouse.down();
			const steps = 60;
			for (let i = 1; i <= steps; i++) {
				await page.mouse.move(x, y + (200 * i) / steps);
				await page.waitForTimeout(16);
			}
			const during = drawnRuns(await seen());
			assert.ok(during.dayview <= 2, `dayview ran ${during.dayview} times during the drag`);
			assert.ok(during.nowview <= 2, `nowview ran ${during.nowview} times during the drag`);
			await page.mouse.up();
			await app.idle();
			const after = drawnRuns(await seen());
			const bands = await bandCount(page);
			assert.ok(bands > before, `a 200px taller strip holds no more days (${before} → ${bands})`);
			// One `dayview` per NEW band: the width is what every band is drawn
			// at, and it did not change, so the bands already up are kept.
			assert.equal(after.dayview - during.dayview, bands - before, 'the strip drew the wrong number of bands');
			assert.equal(after.nowview - during.nowview, 1, 'nowview, now shorter, redraws once');
		}, DESKTOP);
	});

	test('a border is grabbed right beside a … badge, and the badge stays inside the gap', async () => {
		await withApp(async ({ page }) => {
			// The tail's badge straddles its top outline, into the gap above it
			// — the column border between nowview and tail.
			const border = pane(page, 'side').locator('.split.column > [role="separator"]').first();
			const badge = pane(page, 'tail').locator('.titlebar button.more');
			const [gap, more, tailBefore] = await Promise.all([boxOf(border), boxOf(badge), boxOf(pane(page, 'tail'))]);
			assert.ok(more.y > gap.y, `the badge (top ${more.y}) reaches over the gap (top ${gap.y}) into the pane above`);
			assert.ok(more.y < gap.y + gap.height, 'the badge does not sit on the outline at all');
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
		}, DESKTOP);
	});

	test('on a vertical bar the badge follows the title along it', async () => {
		await withApp(async ({ page }) => {
			const tail = pane(page, 'tail');
			await choose(page, 'tail', 'Bar: right');
			let [h2, more] = await Promise.all([boxOf(tail.locator('h2')), boxOf(tail.locator('button.more'))]);
			let apart = more.y - (h2.y + h2.height);
			assert.ok(apart >= 0 && apart <= 8, `right bar, reading down: the badge is ${apart}px below the title`);
			await choose(page, 'tail', 'Bar: left');
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
			await page.locator('.bar button.widget', { hasText: 'coffee' }).waitFor();
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
			assert.equal(await page.locator('.bar button.widget', { hasText: 'coffee' }).count(), 1);
		}, DESKTOP);
	});
});

/** A draft of one REPL pane, written before the app loads. */
const ONE_PANE = JSON.stringify({
	version: 1,
	name: 'one',
	root: {
		id: 'only',
		tabs: [{ id: 'only-tab', content: { id: 'only-widget', kind: 'repl', settings: {} } }],
		active: 0,
		bar: { edge: 'top', align: 'start' }
	}
});

function draft(json) {
	localStorage.setItem('datum.layout.draft.v1.default', json);
}
