// The layout tests' vocabulary (da-jdop): finding panes, driving the
// title's menu and the borders, and writing a layout down before the app
// loads.
//
// Shared by the `layout-*.test.mjs` files, which were one file until it
// outgrew the runner's per-file timeout (da-5xg3). The harness is still
// `app.mjs`; nothing here opens a browser.
//
// Panes are found by the id the default layout gives them
// (`$lib/layout/defaults.ts`: `tail`, `repl`, `strip`, …) or by their
// `data-title`, which is the h2 with one tab and the active tab's label
// with more.

import assert from 'node:assert/strict';
import { devices } from 'playwright';
import { until } from './until.mjs';

export const DESKTOP = { viewport: { width: 1280, height: 900 } };
export const PHONE = { device: devices['iPhone 12'] };

/**
 * A pane of a ROOM, by the id `day` gives it (da-xamd).
 *
 * A room is `day`'s tree inlined a SECOND time — `day` itself is still a
 * root tab — and `inline` re-mints an id the tree already holds as
 * `<id>@<name>` (shell.ts), so `day`'s own `tail` is `tail@acts` inside
 * the room called `acts`.
 */
export const inRoom = (room, id) => `${id}@${room}`;

/** Every layout key this device has stored: the open list, the drafts, the locks. */
export const layoutStorage = (page) =>
	page.evaluate(() =>
		Object.fromEntries(
			Object.keys(localStorage)
				.filter((k) => k.startsWith('datum.layout.'))
				.sort()
				.map((k) => [k, localStorage.getItem(k)])
		)
	);

/**
 * Take a room in a shared app, and hand back the assertion that it left no
 * trace (da-xamd).
 *
 * A preview is written NOWHERE, so this device's layout storage at the end
 * of the test is what it was the moment the room opened, byte for byte,
 * and the room's own name is in none of it. The `clean()` a test calls
 * last is the whole of what makes one app safe for the next test.
 */
export async function takeRoom(app, name) {
	await app.room(name);
	const before = await layoutStorage(app.page);
	assert.ok(!JSON.stringify(before).includes(name), `the room “${name}” was written on the way in`);
	return async () => {
		assert.deepEqual(await layoutStorage(app.page), before, `the room “${name}” wrote to this device`);
	};
}

/** The widget panes' titles, in DOM order. */
export const titles = (page) =>
	page.evaluate(() => [...document.querySelectorAll('section.pane')].map((p) => p.dataset.title));

/** A pane, by the id the layout gives it. */
export const pane = (page, id) => page.locator(`section[data-pane="${id}"]`);

/**
 * A pane by its TITLE, which is what a copy of a layout keeps when its ids
 * are re-minted (da-xamd): `day`'s `tail` is `tail@acts` inside the room
 * called `acts`, and its title is `tail` in both.
 */
export const titledPane = (page, title) => page.locator(`section.pane[data-title="${title}"]`);

/** What opens a titled pane's menu: its title, or — with several tabs — the tab in front. */
export const titledMenuButton = (page, title) =>
	titledPane(page, title)
		.locator('> .titlebar button.title, > .titlebar [role="tab"][aria-selected="true"]')
		.first();

export const boxOf = (locator) => locator.boundingBox();

export const activeTab = (page, id) =>
	pane(page, id).locator('[role="tab"][aria-selected="true"]').innerText();

/**
 * Wait until two animation frames in a row lay the page out the same.
 *
 * What a test waits for after an input whose effect is the layout: a drop,
 * a swipe, a press on a locked border. Playwright's input calls return once
 * the page has handled the event, and every handler here updates a store,
 * so by the second identical frame whatever the input did is on screen —
 * and whatever it did not do is not coming. The deadline is the test's.
 */
export const settled = (page) =>
	page.waitForFunction(async () => {
		const frame = () => new Promise((resolve) => requestAnimationFrame(resolve));
		const shape = () =>
			JSON.stringify(
				[...document.querySelectorAll('section, .cell, [role="separator"], [role="menu"], [role="tab"]')].map((el) => {
					const r = el.getBoundingClientRect();
					return [r.x, r.y, r.width, r.height, el.getAttribute('aria-selected')];
				})
			);
		await frame();
		const before = shape();
		await frame();
		return before === shape();
	});

/**
 * What opens a pane's menu (da-9vtw): its title, or — with several tabs —
 * the tab in front. The `…` badge is gone; the `+` beside it adds a tab.
 */
export const menuButton = (page, id) =>
	pane(page, id).locator('.titlebar button.title, .titlebar [role="tab"][aria-selected="true"]').first();

/** A pane's `+`: the badge where the `…` was. */
export const addButton = (page, id) => pane(page, id).locator('.titlebar button.add').first();

/** A pane's maximize badge (da-00am): at the bar's far corner, away from the tabs (da-jlgp). Its own, not a child's. */
export const maximizeButton = (page, id) => pane(page, id).locator('> .titlebar button.maximize');

/**
 * What opens the PAGE's menu (da-wjob): the active root tab's name. The
 * root bar is the pane title bar formatted as a top bar, so this is the
 * same `PaneMenu` with the same items, less the ones the root has no use
 * for — see `menu.ts`.
 */
export const rootMenuButton = (page) => page.locator('.root-bar [role="tab"][aria-selected="true"]');

/** Open the root menu and choose an item by its name — `Save layout`, `split: a new pane to the left`. */
export async function chooseRoot(page, label) {
	await rootMenuButton(page).click();
	await page.getByRole('menuitem', { name: label, exact: true }).click();
	await page.locator('[role="menu"]').waitFor({ state: 'detached' });
}

/** Open a pane's menu and choose an item by its name — `Choose…`, `break out: this widget below`. */
export async function choose(page, id, label) {
	await menuButton(page, id).click();
	await page.getByRole('menuitem', { name: label, exact: true }).click();
	await page.locator('[role="menu"]').waitFor({ state: 'detached' });
}

/** Add a tab to a pane: the `+`, then `name` in the chooser it opens. */
export async function addTab(page, id, name) {
	await addButton(page, id).click();
	await chooser(page).waitFor();
	await chooser(page).locator('button').filter({ hasText: exactly(name) }).click();
	await chooser(page).waitFor({ state: 'detached' });
}

/** The open menu, one string per line: an item's words, or a row's label and its buttons. */
export const menuLines = (page) =>
	page.locator('[role="menu"]').first().evaluate((menu) =>
		[...menu.children].map((el) =>
			el.getAttribute('role') === 'group'
				? [el.getAttribute('aria-label'), ...[...el.querySelectorAll('button')].map((b) => b.textContent.trim())].join(' ')
				: el.textContent.trim()
		)
	);

/** A menu item's whole label. */
export const exactly = (label) => new RegExp(`^\\s*${label.replace(/[.*+?^${}()|[\]\\]/g, '\\$&')}\\s*$`);

/** The one chooser (da-uyx1): the modal that fills a tab, open. */
export const chooser = (page) => page.locator('dialog.chooser');

/** What the chooser offers, in order: the layouts, then the widgets. */
export const offers = (page) => chooser(page).locator('button').allInnerTexts();

/** Open the chooser from an empty pane's one button. */
export async function openChooser(page) {
	await page.locator('section.pane[data-title="empty"] .choose button').click();
	await chooser(page).waitFor();
}

/** Open the chooser from an empty pane and choose `name` — a layout or a widget. */
export async function fill(page, name) {
	await openChooser(page);
	await chooser(page).locator('button').filter({ hasText: exactly(name) }).click();
	await chooser(page).waitFor({ state: 'detached' });
}

/**
 * A real touch swipe from (x, y) by (dx, dy), through CDP.
 *
 * `Input.dispatchTouchEvent` is a real touch — the page's mouse cannot
 * make one, and a swipe made of mouse events would be a test of the wrong
 * handler. It moves in several steps because the gesture's axis is
 * committed at the first move that has gone far enough (da-kcmp), and
 * because the content follows the finger through those moves.
 */
export async function swipe(page, x, y, dx, dy, steps = 6) {
	const cdp = await page.context().newCDPSession(page);
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

/**
 * Swipe by (dx, dy) from a point in `locator`'s box, given as fractions of
 * it — its middle unless `at` says otherwise. The element is brought on
 * screen first: a touch outside the viewport lands nowhere.
 */
export async function swipeOver(page, locator, dx, dy, at = { x: 0.5, y: 0.5 }) {
	await locator.evaluate((el) => el.scrollIntoView({ block: 'nearest' }));
	const box = await locator.boundingBox();
	await swipe(page, box.x + box.width * at.x, box.y + box.height * at.y, dx, dy);
}

/**
 * Put `locator`'s scroll at its `end` (or its start) along x, and leave it
 * there.
 *
 * What a swipe test says before it swipes: a scroller with room in the
 * finger's direction keeps the gesture and a scroller at its end yields it
 * (da-kcmp), so a test about the FRAME's half of that rule pins the scroll
 * rather than hoping the widget under the finger is narrow. A no-op where
 * nothing scrolls.
 *
 * The scroll the LAST gesture started is waited out first: a touch scroll
 * settles over several frames, and a write made while it is still running
 * is overtaken by it — which is a scroller that yielded a gesture in the
 * test's words and kept it in the browser's.
 */
export async function scrollX(locator, end) {
	let last = null;
	await until(
		() => locator.evaluate((el) => el.scrollLeft),
		(at) => {
			const stopped = at === last;
			last = at;
			return stopped;
		}
	);
	await locator.evaluate((el, end) => (el.scrollLeft = end ? el.scrollWidth : 0), end);
	await until(
		() => locator.evaluate((el) => [el.scrollLeft, el.scrollWidth - el.clientWidth]),
		([at, most]) => (end ? at >= most - 1 : at === 0)
	);
}

/**
 * Wait until `locator` is at rest: nothing of the swipe still in flight.
 *
 * The barrier a swipe wants, `settled` being blind to it: a transform
 * moves what is drawn and nothing that is laid out, so the page's boxes
 * are identical from one frame to the next while the content is sliding.
 * Both endings come back to no transform at all — the tab switched, or
 * the swipe snapped back — so this is where the tab may be read, and a
 * swipe that should have moved NOTHING has had its chance to.
 */
export const atRest = (locator) =>
	until(
		() => locator.evaluate((el) => getComputedStyle(el).transform),
		(transform) => transform === 'none',
		5000
	);

/** The `Bar ▸` submenu, open. */
export const barMenu = (page) => page.locator('[role="menu"][aria-label="Bar"]');

/**
 * Open a pane's menu, then its `Bar ▸`.
 *
 * By click: a title on the page's outer edge hangs half into the page's
 * margin, and the margin is empty furniture-wise since the four `…` edge
 * handles went (da-wjob), so the pointer reaches it. It did not while
 * they were there (da-ywp1), and this opened the menu with Enter to get
 * past them; the pointer's own path is tested again now.
 */
export async function openBar(page, id) {
	await menuButton(page, id).click();
	await page.locator('[role="menu"] button').filter({ hasText: exactly('Bar') }).click();
	await barMenu(page).waitFor();
}

/** Choose a `Bar ▸` icon by its word — `left`, `end` — and wait for the menus to close. */
export async function chooseBar(page, id, word) {
	await openBar(page, id);
	await barMenu(page).locator(`[aria-label="${word}"]`).click();
	await page.waitForFunction(() => !document.querySelector('[role="menu"]'));
}

/** The box of a pane's picture, as the picture says it (its `width`). */
export const canvasWidth = (page, id) =>
	pane(page, id).locator('svg.svg').evaluate((svg) => svg.width.baseVal.value);

/** How many times each drawn command has run, from `spyOnRunner`'s list. */
export const drawnRuns = (argvs) => ({
	dayview: argvs.filter((a) => a[0] === 'dayview').length,
	nowview: argvs.filter((a) => a[0] === 'nowview').length
});

/**
 * Drag from the middle of `border` by (dx, dy) at pointer rate for
 * `ms`, and leave the button down. Time-bounded rather than counted:
 * the measurement is redraws PER SECOND of dragging, so the 16ms between
 * moves is the pointer's rate, not a wait for anything.
 */
export async function dragFor(page, border, dx, dy, ms) {
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

/** Press the pointer on the middle of `border`; the point pressed. */
export async function grab(page, border) {
	const box = await border.boundingBox();
	const at = { x: box.x + box.width / 2, y: box.y + box.height / 2 };
	await page.mouse.move(at.x, at.y);
	await page.mouse.down();
	return at;
}

/** The extent, in px, of each cell of the first split along `direction`. */
export const cellExtents = (page, direction) =>
	page.evaluate(
		(d) => [...document.querySelectorAll(`.split.${d} > .cell`)].map((c) => c.getBoundingClientRect()[d === 'row' ? 'width' : 'height']),
		direction
	);

/**
 * A top-bar widget pane's frame (Layout.svelte's `chromeOf`, da-13bc):
 * the outline, and the inside half of the title's 16px row across the
 * top (da-om9v).
 */
export const CHROME = { width: 2, height: 2 + 8 };

export const bar = { edge: 'top', align: 'start' };

export const widgetPane = (id, kind, settings = {}, edge = 'top') => ({
	id,
	tabs: [{ id: `${id}-tab`, content: { id: `${id}-widget`, kind, settings } }],
	active: 0,
	bar: { ...bar, edge }
});

/** A root holder over `children` along `direction`, in equal shares. */
export const along = (direction, ...children) => ({
	version: 1,
	name: 'default',
	root: {
		id: 'root',
		tabs: [{ id: 'root-tab', content: { id: 'split', direction, ratios: children.map(() => 1 / children.length), children } }],
		active: 0,
		bar
	}
});

/** A draft of one REPL pane for the `day` tab, written before the app loads. */
export const ONE_PANE = JSON.stringify({
	version: 1,
	name: 'day',
	root: {
		id: 'only',
		tabs: [{ id: 'only-tab', content: { id: 'only-widget', kind: 'repl', settings: {} } }],
		active: 0,
		bar: { edge: 'top', align: 'start' }
	}
});

/** An init script: `json` as the `day` tab's draft. */
export function draft(json) {
	localStorage.setItem('datum.layout.draft.v1.day', json);
}
