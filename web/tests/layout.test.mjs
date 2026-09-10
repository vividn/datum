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
			// column is scrolled, and a finger there is on the bar.
			const content = pane(page, 'side').locator('.content');
			await content.evaluate((el) => el.scrollIntoView({ block: 'start' }));
			let box = await content.boundingBox();
			await swipe(page, box.x + box.width * 0.8, box.y + 24, -120, 0);
			await page.waitForFunction(
				() => document.querySelector('section[data-pane="side"] [role="tab"][aria-selected="true"]')?.textContent === 'nowview'
			);
			// Drawn before the swipe back: while the picture is arriving the
			// body is briefly wider than its box, and a swipe over a sideways
			// scroll is — by the rule under test — not a swipe.
			await pane(page, 'side').locator('img.svg').waitFor();
			await swipe(page, box.x + box.width * 0.2, box.y + 24, 120, 0);
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
			await swipe(page, box.x + box.width * 0.8, box.y + 24, -120, 0);
			await page.waitForTimeout(300);
			assert.equal(await activeTab(page, 'repl'), 'repl', 'a swipe over a sideways scroll changed the tab');
		}, PHONE);
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
