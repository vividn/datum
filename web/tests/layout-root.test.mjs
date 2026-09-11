// The layout engine (da-jdop) at the page's root: the default layout, the
// draft, the phone's column and its swipe, holders, the lock — driven
// through the built app in a real browser.
//
// Every assertion here is about what is ON SCREEN — titles, widths, tab
// labels — never "more panes than before": a delta-blind probe passes
// against a page that grew the wrong pane.
//
// The phone cases use Playwright's iPhone 12 profile whole (a touch
// pointer, a device pixel ratio, overlay scrollbars) and drive the swipe
// through CDP's `Input.dispatchTouchEvent`, which is a real touch — the
// page's mouse cannot make one, and a swipe made of mouse events would be
// a test of the wrong handler.

import test, { before, after, describe } from 'node:test';
import assert from 'node:assert/strict';
import { startApp, stopApp, withApp } from './support/app.mjs';
import {
	DESKTOP,
	PHONE,
	activeTab,
	bar,
	boxOf,
	chooseBar,
	choose,
	draft,
	menuLines,
	pane,
	settled,
	titles,
	widgetPane
} from './support/layout.mjs';

before(startApp);
after(stopApp);

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
			// Each swipe is read once the page has settled, not waited for:
			// the same barrier that shows these two landing is what shows the
			// last one, over a scroll, did not.
			await swipe(page, box.x + box.width * 0.8, mid, -120, 0);
			await settled(page);
			assert.equal(await activeTab(page, 'side'), 'nowview', 'a swipe left did not move to the next tab');
			// Drawn before the swipe back: while the picture is arriving the
			// body is briefly wider than its box, and a swipe over a sideways
			// scroll is — by the rule under test — not a swipe.
			await pane(page, 'side').locator('svg.svg').waitFor();
			await swipe(page, box.x + box.width * 0.2, mid, 120, 0);
			await settled(page);
			assert.equal(await activeTab(page, 'side'), 'tail', 'a swipe right did not move back');

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
			await settled(page);
			assert.equal(await activeTab(page, 'repl'), 'repl', 'a swipe over a sideways scroll changed the tab');
		}, PHONE);
	});
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
				await settled(page);
				assert.deepEqual(await cellWidths(page), was, 'a drag across a locked border moved it');
				await page.evaluate(() => document.body.focus());
				await page.keyboard.press('ArrowRight');
				await settled(page);
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

function lockedOnce(json) {
	if (localStorage.getItem('datum.layout.draft.v1.day')) return;
	localStorage.setItem('datum.layout.draft.v1.day', json);
	localStorage.setItem('datum.layout.locked.v1.day', '1');
}
