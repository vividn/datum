// The layout tests' vocabulary (da-jdop): finding panes, driving the `…`
// menu and the borders, and writing a layout down before the app loads.
//
// Shared by the `layout-*.test.mjs` files, which were one file until it
// outgrew the runner's per-file timeout (da-5xg3). The harness is still
// `app.mjs`; nothing here opens a browser.
//
// Panes are found by the id the default layout gives them
// (`$lib/layout/defaults.ts`: `tail`, `repl`, `strip`, …) or by their
// `data-title`, which is the h2 with one tab and the active tab's label
// with more.

import { devices } from 'playwright';

export const DESKTOP = { viewport: { width: 1280, height: 900 } };
export const PHONE = { device: devices['iPhone 12'] };

/** The widget panes' titles, in DOM order. */
export const titles = (page) =>
	page.evaluate(() => [...document.querySelectorAll('section.pane')].map((p) => p.dataset.title));

/** A pane, by the id the layout gives it. */
export const pane = (page, id) => page.locator(`section[data-pane="${id}"]`);

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

/** Open a pane's `…` menu and choose an item by its name — `Add tab`, `split side by side`. */
export async function choose(page, id, label) {
	await pane(page, id).locator('.titlebar button.more').first().click();
	await page.getByRole('menuitem', { name: label, exact: true }).click();
	await page.locator('[role="menu"]').waitFor({ state: 'detached' });
}

/** The open `…` menu, one string per line: an item's words, or a row's label and its buttons. */
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

/** The `Bar ▸` submenu, open. */
export const barMenu = (page) => page.locator('[role="menu"][aria-label="Bar"]');

/** Open a pane's `…` menu, then its `Bar ▸`. */
export async function openBar(page, id) {
	await pane(page, id).locator('.titlebar button.more').first().click();
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
