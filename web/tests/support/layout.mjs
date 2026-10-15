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
 * A pane of a ROOM, by the id `main` gives it (da-xamd).
 *
 * A room is `main`'s tree inlined a SECOND time — `main` itself is still a
 * root tab — and `inline` re-mints an id the tree already holds as
 * `<id>@<name>` (shell.ts), so `main`'s own `tail` is `tail@acts` inside
 * the room called `acts`.
 */
export const inRoom = (room, id) => `${id}@${room}`;

/** Every layout key this device has stored: the open list, the drafts. */
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
 * are re-minted (da-xamd): `main`'s `tail` is `tail@acts` inside the room
 * called `acts`, and its title is `tail` in both.
 */
export const titledPane = (page, title) => page.locator(`section.pane[data-title="${title}"]`);

/** What opens a titled pane's menu: its title, or — with several tabs — the tab in front. */
export const titledMenuButton = (page, title) =>
	titledPane(page, title)
		.locator('> .titlebar button.title, > .titlebar [role="tab"][aria-selected="true"]')
		.first();

export const boxOf = (locator) => locator.boundingBox();

/**
 * Where a tabbed holder's strip lies over the panes under it (da-cesj),
 * from the boxes on the SCREEN: every inner title the strip's box meets,
 * and every inner control its BADGES meet — the `+`, and since da-efr4i
 * the `×` after it. `titles` counts
 * the inner titles looked at, so an empty holder cannot pass for a clear
 * one.
 */
export const overStrip = (page, id) =>
	pane(page, id).evaluate((holder) => {
		const box = (el) => el.getBoundingClientRect();
		const meet = (a, b) => a.left < b.right && b.left < a.right && a.top < b.bottom && b.top < a.bottom;
		const strip = holder.querySelector(':scope > .titlebar');
		const badges = [...strip.querySelectorAll('button.add, button.close')].map((el) => [
			el.classList.contains('add') ? '+' : '×',
			box(el)
		]);
		const inner = [...holder.querySelectorAll(':scope > .content section.pane')];
		const overlaps = inner.flatMap((p) => [
			...[...p.querySelectorAll(':scope > .titlebar :is(.titlebox, .tabs)')]
				.filter((t) => meet(box(strip), box(t)))
				.map(() => `the strip lies over ${p.dataset.title}'s title`),
			...badges.flatMap(([mark, at]) =>
				[...p.querySelectorAll(':scope > .titlebar button')]
					.filter((b) => meet(at, box(b)))
					.map((b) => `the strip's ${mark} lies over ${p.dataset.title}'s ${b.className}`)
			)
		]);
		return { titles: inner.length, overlaps };
	});

export const activeTab = (page, id) =>
	pane(page, id).locator('[role="tab"][aria-selected="true"]').innerText();

/**
 * Wait until two animation frames in a row lay the page out the same.
 *
 * What a test waits for after an input whose effect is the layout: a drop,
 * a swipe, a press on a locked border. Playwright's input calls return once
 * the page has handled the event, and every handler here updates a store,
 * so by the second identical frame whatever the input did is on screen —
 * and whatever it did not do is not coming. Polled from node: a
 * `waitForFunction` predicate that is `async` returns a Promise, which is
 * truthy, so it never waited at all (da-1grm7).
 *
 * The frames' answer is the elements that MOVED between them, each named
 * with its box before and after, so a page that never settles times out
 * saying which element kept moving rather than `false` (da-lbkpg).
 */
export const settled = (page) =>
	until(
		() =>
			page.evaluate(async () => {
				const frame = () => new Promise((resolve) => requestAnimationFrame(resolve));
				const shape = () =>
					[...document.querySelectorAll('section, .cell, [role="separator"], [role="menu"], [role="tab"]')].map((el) => {
						const r = el.getBoundingClientRect();
						return [el, JSON.stringify([r.x, r.y, r.width, r.height, el.getAttribute('aria-selected')])];
					});
				const name = (el) =>
					el.localName +
					['data-pane', 'data-title', 'role', 'class']
						.filter((a) => el.hasAttribute(a))
						.map((a) => `[${a}=${el.getAttribute(a)}]`)
						.join('');
				await frame();
				const before = shape();
				await frame();
				const after = shape();
				if (after.length !== before.length) return [`${before.length} elements became ${after.length}`];
				return after.flatMap(([el, box], i) => (box === before[i][1] ? [] : [`${name(el)} ${before[i][1]} → ${box}`]));
			}),
		(moved) => moved.length === 0
	);

/**
 * What opens a pane's menu (da-9vtw): its title, or — with several tabs —
 * the tab in front. The `…` badge is gone; the `+` beside it adds a tab.
 *
 * ⚠ A pane may have NO such button in normal mode (da-z8klv): the NAME's
 * slice is wholly conditional, so a pane holding a widget whose kind
 * declares neither defaults nor options — `custom`, `aliases`, `notice` —
 * and no layout draws none. In layout mode every pane has one (da-vvcxc).
 */
export const menuButton = (page, id) =>
	pane(page, id).locator('.titlebar button.title, .titlebar [role="tab"][aria-selected="true"]').first();

/** A pane's `+`: the badge where the `…` was. */
export const addButton = (page, id) => pane(page, id).locator('.titlebar button.add').first();

/** The page in layout mode (da-vvcxc): the frame says so, as the root bar does. */
export const inLayoutMode = (page) => page.locator('.frame[data-layout-mode]').count().then((n) => n > 0);

/**
 * Into layout mode, by its one door: the open root tab's context menu (a
 * right click). A page already in it is left as it is.
 */
export async function enterLayoutMode(page) {
	if (await inLayoutMode(page)) return;
	await rootMenuButton(page).click({ button: 'right' });
	await page.locator('.frame[data-layout-mode]').waitFor();
}

/**
 * Out of layout mode by the open tab's menu — `Accept`, `Save layout` or
 * `Discard changes` — and wait for the page to say so.
 */
export async function leaveLayoutMode(page, how = 'Accept') {
	if (!(await inLayoutMode(page))) return;
	await rootMenuButton(page).click();
	await page.getByRole('menuitem', { name: how, exact: true }).click();
	await page.locator('.frame[data-layout-mode]').waitFor({ state: 'detached' });
}

/**
 * A pane's maximize badge (da-00am): before the tab names (da-bzhq). Its
 * own, not a child's. In layout mode it is the bar's HANDLE (da-vvcxc).
 */
export const maximizeButton = (page, id) => pane(page, id).locator('> .titlebar button.maximize');

/**
 * What opens the PAGE's menu (da-wjob): the active root tab's name. The
 * root bar is the pane title bar formatted as a top bar, so this is the
 * same `PaneMenu` with the same items, less the ones the root has no use
 * for — see `menu.ts`.
 */
export const rootMenuButton = (page) => page.locator('.root-bar [role="tab"][aria-selected="true"]');

/**
 * Open the menu `button` opens and choose `label` from it, in whichever
 * mode the page is (da-vvcxc). In layout mode a pane's normal-mode items
 * are one step in, under `Widget options`; the root tab's are not in its
 * layout-mode menu at all, so the mode is left (Accept) to reach them.
 */
export async function pickFrom(page, button, label) {
	await button.click();
	const item = page.getByRole('menuitem', { name: label, exact: true });
	if ((await item.count()) === 0 && (await inLayoutMode(page))) {
		const options = page.getByRole('menuitem', { name: 'Widget options', exact: true });
		if (await options.count()) await options.click();
		else {
			await page.keyboard.press('Escape');
			await leaveLayoutMode(page);
			await button.click();
		}
	}
	await item.click();
	await page.locator('[role="menu"]').waitFor({ state: 'detached' });
}

/** Open the root menu and choose an item by its name — `Save layout`, `split: a new pane to the left`. */
export const chooseRoot = (page, label) => pickFrom(page, rootMenuButton(page), label);

/** Open a pane's NAME menu and choose an item by its name — `Save layout`, `Reset widget`. */
export const choose = (page, id, label) => pickFrom(page, menuButton(page, id), label);

/** What opens a widget's own options: a pane's name, or the root tab's. */
const optionMenu = (page, id) => (id === null ? rootMenuButton(page) : menuButton(page, id));

/** The menu holding a widget's options open, in either mode (da-vvcxc): see `pickFrom`. */
async function openOptions(page, id) {
	if (id === null) await leaveLayoutMode(page);
	await optionMenu(page, id).click();
	if (id !== null && (await inLayoutMode(page))) await page.getByRole('menuitem', { name: 'Widget options', exact: true }).click();
}

/**
 * A widget's SWITCH in the menu its name opens (da-vhwq): pressed, waited
 * for, and the menu closed again. `id` is null for the root tab's menu.
 */
export const switchItem = (page, label, on) =>
	// By its NAME, which a glyph before the words is no part of (`aria-hidden`).
	page.getByRole('menuitemcheckbox', { name: label, exact: true, ...(on === undefined ? {} : { checked: on }) });

export async function toggleOption(page, id, label) {
	await openOptions(page, id);
	const was = (await switchItem(page, label).getAttribute('aria-checked')) === 'true';
	await switchItem(page, label).click();
	// The item stays, and says it went the other way: a switch does not
	// close the menu (PaneMenu.svelte), so this is the settle to wait for.
	await switchItem(page, label, !was).waitFor();
	await page.keyboard.press('Escape');
	await page.locator('[role="menu"]').waitFor({ state: 'detached' });
}

/** One of a widget's CHOICES, from the submenu its item opens; both menus close. */
export async function pickOption(page, id, label, value) {
	await openOptions(page, id);
	await page.locator('[role="menu"] > button').filter({ hasText: exactly(label) }).click();
	await page.locator('[role="menu"][aria-orientation="vertical"]').waitFor();
	await page
		.locator('[role="menu"][aria-orientation="vertical"] button')
		.filter({ hasText: new RegExp(`^\\s*${value}(\\s·\\sdefault)?\\s*$`) })
		.click();
	await page.waitForFunction(() => !document.querySelector('[role="menu"]'));
}

/** Whether a widget's switch reads ON, without changing it. */
export async function optionChecked(page, id, label) {
	await openOptions(page, id);
	const on = await switchItem(page, label).getAttribute('aria-checked');
	await page.keyboard.press('Escape');
	await page.locator('[role="menu"]').waitFor({ state: 'detached' });
	return on === 'true';
}

/** The values a widget's CHOICE offers, and which is in force. */
export async function choiceLines(page, id, label) {
	await openOptions(page, id);
	await page.locator('[role="menu"] > button').filter({ hasText: exactly(label) }).click();
	const list = page.locator('[role="menu"][aria-orientation="vertical"]');
	await list.waitFor();
	const lines = await list.locator('button').evaluateAll((bs) =>
		bs.map((b) => `${b.getAttribute('aria-checked') === 'true' ? '✓ ' : ''}${b.textContent.trim()}`)
	);
	await page.keyboard.press('Escape');
	await page.keyboard.press('Escape');
	await page.waitForFunction(() => !document.querySelector('[role="menu"]'));
	return lines;
}

/**
 * In layout mode, open a pane's name and choose an item — `Change Widget`,
 * `break out: this widget below`, `Close pane` (da-vvcxc).
 */
export async function manage(page, id, label) {
	await enterLayoutMode(page);
	await menuButton(page, id).click();
	await page.getByRole('menuitem', { name: label, exact: true }).click();
	await page.locator('[role="menu"]').waitFor({ state: 'detached' });
}

/** In layout mode, open the root tab's name and choose an item — `split: a new pane to the left`, `Close tab`. */
export async function manageRoot(page, label) {
	await enterLayoutMode(page);
	await chooseRoot(page, label);
}

/** In layout mode, add a tab to a pane: the `+`, then `name` in the chooser it opens. */
export async function addTab(page, id, name) {
	await enterLayoutMode(page);
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
 *
 * `every` gives the gesture its OWN CLOCK (da-wucn6): its events are stamped
 * that many ms apart, however long the round trips took. Without it each is
 * stamped as the browser receives it, so the gesture's speed is the runner's
 * — a test that asserts what a speed did passes `every`. `hold` is how many
 * ms longer the finger then rests where it stopped, before it lifts: a
 * stamp as well, so without `every` it is refused and not dropped.
 */
export async function swipe(page, x, y, dx, dy, steps = 6, every, hold = 0) {
	if (hold && !every) throw new Error('swipe: `hold` is a stamp, and needs `every`');
	const cdp = await page.context().newCDPSession(page);
	const t0 = Date.now() / 1000;
	const at = (type, touchPoints, i) =>
		cdp.send('Input.dispatchTouchEvent', {
			type,
			touchPoints,
			...(every && { timestamp: t0 + (i * every) / 1000 })
		});
	await at('touchStart', [{ x, y }], 0);
	for (let i = 1; i <= steps; i++)
		await at('touchMove', [{ x: x + (dx * i) / steps, y: y + (dy * i) / steps }], i);
	await at('touchEnd', [], steps + 1 + hold / every);
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
 * Wait until the content box `locator` is at rest: nothing of the swipe
 * still in flight.
 *
 * The barrier a swipe wants, `settled` being blind to it: a transform
 * moves what is drawn and nothing that is laid out, so the page's boxes
 * are identical from one frame to the next while the strip is sliding.
 * The strip moves only while the box is `moving` (da-24fgb), and both
 * endings put that down — the tab switched, or the swipe snapped back —
 * so this is where the tab may be read, and a swipe that should have
 * moved NOTHING has had its chance to.
 */
export const atRest = (locator) =>
	until(
		() => locator.evaluate((el) => el.classList.contains('moving')),
		(moving) => !moving
	);

/**
 * Move a pane's bar to `edge` — `top`, `bottom`, `left`, `right` — and,
 * given one, `align` along it, by dragging its handle (da-vvcxc): the
 * maximize badge in layout mode, dropped on that edge, at the third of it
 * the alignment names as the eye reads it (the left-hand bar runs bottom
 * to top). The handle is let go a few pixels inside the pane's edge.
 */
export async function moveBar(page, id, edge, align = 'center') {
	await enterLayoutMode(page);
	const box = await pane(page, id).boundingBox();
	const handle = await maximizeButton(page, id).boundingBox();
	const along = { start: 1 / 6, center: 1 / 2, end: 5 / 6 }[align];
	const inset = 3;
	const at = {
		top: { x: box.x + box.width * along, y: box.y + inset },
		bottom: { x: box.x + box.width * along, y: box.y + box.height - inset },
		left: { x: box.x + inset, y: box.y + box.height * (1 - along) },
		right: { x: box.x + box.width - inset, y: box.y + box.height * along }
	}[edge];
	await page.mouse.move(handle.x + handle.width / 2, handle.y + handle.height / 2);
	await page.mouse.down();
	await page.mouse.move(at.x, at.y, { steps: 8 });
	await page.mouse.up();
	await pane(page, id).and(page.locator(`.edge-${edge}`)).locator(`> .titlebar > .along.align-${align}`).waitFor();
}

/**
 * Put a pane's bar on `word` — an edge (`left`) or an alignment (`end`),
 * the other kept as it is — by its handle, and leave the page in the mode
 * it was in: what the `Bar ▸` submenu's icons did (da-vvcxc).
 */
export async function placeBar(page, id, word) {
	const was = await inLayoutMode(page);
	const edge = (await pane(page, id).getAttribute('class')).match(/\bedge-(\w+)/)[1];
	const align = (await pane(page, id).locator('> .titlebar > .along').getAttribute('class')).match(/\balign-(\w+)/)[1];
	const edges = ['top', 'bottom', 'left', 'right'];
	await moveBar(page, id, edges.includes(word) ? word : edge, edges.includes(word) ? align : word);
	if (!was) await leaveLayoutMode(page);
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
 * How many times `of` was run for the box it ended at. Not runs counted
 * after the drop: a pane keeps one run out and one waiting (CliPane's
 * `refresh`), so on a loaded box the run a drag's last tick asked for can
 * start after the drop, and by the clock it was the drop's (da-i1ok). The
 * drop's own run is the last box, and asking twice for it is a redraw of a
 * picture already drawn.
 */
export const finalAsks = (argvs, of) => {
	const mine = argvs.filter((a) => a[0] === of).map((a) => JSON.stringify(a));
	return mine.filter((a) => a === mine.at(-1)).length;
};

/**
 * Drag from the middle of `border` by (dx, dy) at pointer rate for
 * `ms`, and leave the button down. Time-bounded rather than counted:
 * the measurement is redraws PER SECOND of dragging, so the 16ms between
 * moves is the pointer's rate, not a wait for anything.
 */
export async function dragFor(page, border, dx, dy, ms) {
	// A border is a handle in layout mode alone (da-vvcxc).
	await enterLayoutMode(page);
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

/** Press the pointer on the middle of `border`, in layout mode (da-vvcxc); the point pressed. */
export async function grab(page, border) {
	await enterLayoutMode(page);
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

/**
 * `mine`: two panes side by side, so a nested layout has an INSIDE — to
 * edit, or to maximize out of. A saved layout of one pane is lifted into
 * the tab that holds it (shell.ts), and a lifted one-widget layout has no
 * inside: splitting the pane it draws in splits the tree AROUND it.
 *
 * The second is `widgetbar`, which is the fields widget's KIND: `fields`
 * is what it is CALLED (da-850j, registry.ts), and a kind nothing is
 * registered under declares neither defaults nor options.
 */
export const MINE = {
	version: 1,
	name: 'mine',
	root: {
		id: 'm',
		active: 0,
		bar,
		tabs: [
			{
				id: 'm-tab',
				content: {
					id: 'm-split',
					direction: 'row',
					ratios: [0.5, 0.5],
					children: [widgetPane('m-a', 'nowview'), widgetPane('m-b', 'widgetbar')]
				}
			}
		]
	}
};

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

/** A draft of one REPL pane for the `main` tab, written before the app loads. */
export const ONE_PANE = JSON.stringify({
	version: 1,
	name: 'main',
	root: {
		id: 'only',
		tabs: [{ id: 'only-tab', content: { id: 'only-widget', kind: 'repl', settings: {} } }],
		active: 0,
		bar: { edge: 'top', align: 'start' }
	}
});

/** An init script: `json` as the `main` tab's draft. */
export function draft(json) {
	localStorage.setItem('datum.layout.draft.v1.main', json);
}
