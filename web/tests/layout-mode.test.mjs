// LAYOUT MODE (da-vvcxc), which replaced the padlock, the per-device lock
// and every `▾`. vividn, 2026-10-14: «long pressing (and release without
// dragging to reorder the tabs) or right clicking on a primary tab name
// enters unlocked/layout mode. This should be very visually distinctive.»
//
// What this file holds the change to: a page opens in NORMAL mode, where
// nothing moves the tree and no `▾` or padlock is drawn; each way in — a
// right click, Shift+F10, a mouse's long press, a finger's — enters a mode
// the page shows; a hold that MOVES still reorders; the layout-mode menus
// hold exactly what vividn listed; and the maximize is the bar's handle.
//
// And MODIFY MODE's ways out (da-00c29), vividn, 2026-11-10: «don't show the
// [x] buttons on every tab / - instead show a check button on its right and a
// revert (circle arrow) button on it's left, which applies the changes or
// resets respectively. No buttons on other tabs, which should be dimmed. / -
// when in modify mode, disable all normal functionality of widgets», and «yes
// to esc and enter», «a "modify mode" pill along with (:arrow: revert) and
// (:check: accept) buttons down in the input bar, which should otherwise be
// disabled». So: no × on a tab, ↺ and ✓ on the modified one, the others dim
// and still; ✓ and Enter apply (the dot, no layout entry, a reload keeps it),
// ↺ and Escape reset; the bar's pill does the same with the rest of the bar
// inert; and a widget does nothing.

import test, { before, after, describe } from 'node:test';
import assert from 'node:assert/strict';
import { startApp, stopApp, withApp } from './support/app.mjs';
import {
	DESKTOP,
	PHONE,
	addButton,
	addTab,
	applyButton,
	enterLayoutMode,
	leaveLayoutMode,
	menuButton,
	moveBar,
	pane,
	resetButton,
	rested,
	rootMenuButton,
	titles
} from './support/layout.mjs';
import { pageWait } from './support/press.mjs';
import { until } from './support/until.mjs';
import { HOLD_MS } from '../src/lib/hold.ts';

before(startApp);
after(stopApp);

const mode = (page) => page.locator('.frame[data-layout-mode]');
const isOn = async (page) => (await mode(page).count()) > 0;
const userLayouts = (page) =>
	page.evaluate(async () => (await window.datum.layouts.listLayouts()).filter((l) => l.source === 'user').map((l) => l.name));
/** The open menu's items by name: a row by its label, an item by its words less the glyph. */
const menuNames = (page) =>
	page.locator('[role="menu"]').first().evaluate((menu) =>
		[...menu.children].map((el) => el.getAttribute('aria-label') ?? el.textContent.replace('×', '').trim())
	);
const closeMenu = async (page) => {
	await page.keyboard.press('Escape');
	await page.locator('[role="menu"]').waitFor({ state: 'detached' });
};
/** A dialog asking anything: modify mode asks nothing on its way out. */
const asking = (page) => page.locator('dialog.confirm[open]');
/** What a press on the drawn middle of `locator` lands on, inert or not: a mouse, not an actionable click. */
const pressMiddle = async (page, locator) => {
	const box = await locator.boundingBox();
	await page.mouse.click(box.x + box.width / 2, box.y + box.height / 2);
};
const TWO = { home: { names: ['main', 'cli'], active: 0 } };

/** A finger's long press through CDP, `move`d by (dx, 0) before the lift when given. */
async function fingerHold(page, locator, dx = 0, back = false) {
	const box = await locator.boundingBox();
	const [x, y] = [box.x + box.width / 2, box.y + box.height / 2];
	const cdp = await page.context().newCDPSession(page);
	try {
		await cdp.send('Input.dispatchTouchEvent', { type: 'touchStart', touchPoints: [{ x, y }] });
		// Held until the hold has fired, which picks a finger's tab up.
		await page.waitForFunction(() => !!document.querySelector('.root-bar [data-dragging]'));
		for (let i = 1; dx && i <= 8; i++)
			await cdp.send('Input.dispatchTouchEvent', { type: 'touchMove', touchPoints: [{ x: x + (dx * i) / 8, y }] });
		for (let i = 7; back && i >= 0; i--)
			await cdp.send('Input.dispatchTouchEvent', { type: 'touchMove', touchPoints: [{ x: x + (dx * i) / 8, y }] });
		await cdp.send('Input.dispatchTouchEvent', { type: 'touchEnd', touchPoints: [] });
	} finally {
		await cdp.detach();
	}
}

describe('normal mode', () => {
	test('a page opens in it: no border to drag, no + or × on a pane, and no ▾ or padlock anywhere', async () => {
		await withApp(async ({ page, errors }) => {
			assert.equal(await isOn(page), false);
			assert.equal(await page.locator('[role="separator"]').count(), 0, 'a border is a handle outside layout mode');
			assert.equal(await page.locator('section.pane button.add, section.pane button.close').count(), 0);
			assert.equal(await page.locator('button.manage, .root-bar button.lock').count(), 0);
			assert.deepEqual(
				await page.locator('button').evaluateAll((all) => all.filter((b) => b.textContent.trim() === '▾').length),
				0,
				'a ▾ is drawn'
			);
			// A pane's name opens what it IS, as before.
			await menuButton(page, 'tail').click();
			assert.ok(!(await menuNames(page)).includes('Change Widget'));
			await closeMenu(page);
			assert.deepEqual(errors, []);
		}, DESKTOP);
	});

	test('a fresh device opens in it too: no page «starts locked» or unlocked any more', async () => {
		await withApp(async ({ page }) => {
			assert.equal(await isOn(page), false);
			assert.equal(await page.locator('[role="separator"]').count(), 0);
		}, { ...DESKTOP, home: null });
	});
});

// vremya on #764 (da-hibn3): the (?) took the bar's last 13px and put
// `collect`'s × half under the `+`. The strip scrolls once a first visit's
// tabs, each with its ×, pass a phone's width, as its four do in layout mode
// (da-uzi5x, vividn's dayview tab): at 390 wide, and 360, a common Android
// width, every × is reachable and none stands under the `+`.
describe('on a phone', () => {
	for (const width of [390, 360]) {
		test(`${width} wide: a first visit's four tabs and the modified one's ↺ and ✓ are all reached in the strip in modify mode`, async (t) => {
			await withApp(async ({ page }) => {
				const rest = await page.locator('.root-bar .tabs').evaluate((el) => [el.scrollWidth, el.clientWidth]);
				t.diagnostic(`${width} wide, out of layout mode: the strip is ${rest[0]} of ${rest[1]}px`);
				await enterLayoutMode(page);
				const at = await page.locator('.root-bar .tabs').evaluate((el) => {
					const strip = el.getBoundingClientRect();
					const out = [...el.querySelectorAll('[role="tab"], .way-out')]
						.filter((b) => {
							b.scrollIntoView({ inline: 'nearest', block: 'nearest', behavior: 'instant' });
							const r = b.getBoundingClientRect();
							return r.left < strip.left - 0.5 || r.right > strip.right + 0.5;
						})
						.map((b) => b.getAttribute('aria-label') ?? b.textContent.trim());
					return {
						names: [...el.querySelectorAll('[role="tab"]')].map((t) => t.textContent.replace('•', '').trim()),
						ways: [...el.querySelectorAll('.way-out')].map((b) => b.textContent.trim()),
						order: [...el.children].map((b) => (b.matches('.way-out') ? b.textContent.trim() : b.getAttribute('aria-selected'))),
						out,
						scroll: el.scrollWidth,
						client: el.clientWidth
					};
				});
				assert.deepEqual(at.names, ['welcome', 'setup', 'collection', 'dayview']);
				assert.deepEqual(at.ways, ['↺', '✓'], 'the ways out on the tab');
				assert.deepEqual(at.order.slice(0, 3), ['↺', 'true', '✓'], 'not ↺ before and ✓ after the open tab');
				assert.deepEqual(at.out, [], `out of reach of the strip's ${at.client}px`);
				t.diagnostic(`${width} wide: the strip is ${at.scroll} of ${at.client}px`);
			}, { ...PHONE, viewport: { width, height: 640 }, home: null });
		});
	}
});

describe('the ways in', () => {
	test('a right click on the open tab enters it, visibly: accent panes, handles in the gaps, no × on a tab, ↺ and ✓ on the open one, the others dim; ✓ leaves it', async () => {
		await withApp(async ({ page, errors }) => {
			await rootMenuButton(page).click({ button: 'right' });
			await mode(page).waitFor();
			assert.equal(await page.locator('[role="menu"]').count(), 0, 'the right click also opened the menu');
			assert.equal(await page.locator('.root-bar[data-layout-mode]').count(), 1);
			const panes = await page.locator('section.pane').count();
			assert.equal(await page.locator('section.pane.editing').count(), panes, 'a pane not drawn in the mode');
			assert.ok((await page.locator('[role="separator"]').count()) > 0, 'no handle between the panes');
			assert.equal(await page.locator('.root-bar .tabs button').evaluateAll((all) => all.filter((b) => b.textContent.includes('×')).length), 0, 'a tab wears a ×');
			assert.equal(await page.locator('.root-bar .tabs .way-out').count(), 2);
			const other = page.locator('.root-bar [role="tab"][aria-selected="false"]');
			assert.ok(Number(await other.evaluate((el) => getComputedStyle(el).opacity)) < 1, 'the other tab is not dimmed');
			assert.ok(Number(await rootMenuButton(page).evaluate((el) => getComputedStyle(el).opacity)) === 1, 'the open tab is dimmed');
			assert.ok(await addButton(page, 'tail').isVisible());
			await leaveLayoutMode(page, 'apply');
			assert.equal(await page.locator('[role="separator"]').count(), 0);
			assert.deepEqual(errors, []);
		}, { ...DESKTOP, ...TWO });
	});

	test('the open tab menu’s Modify enters it, on a phone too', async () => {
		for (const device of [DESKTOP, PHONE])
			await withApp(async ({ page }) => {
				await rootMenuButton(page).click();
				await page.getByRole('menuitem', { name: 'Modify', exact: true }).click();
				await mode(page).waitFor();
				assert.equal(await page.locator('.root-bar .tabs .way-out').count(), 2);
			}, device);
	});

	test('Shift+F10 on the focused open tab enters it', async () => {
		await withApp(async ({ page }) => {
			await rootMenuButton(page).focus();
			await page.keyboard.press('Shift+F10');
			await mode(page).waitFor();
		}, DESKTOP);
	});

	test('a mouse held on the open tab and let go where it was enters it, and opens no menu', async () => {
		await withApp(async ({ page }) => {
			const box = await rootMenuButton(page).boundingBox();
			await page.mouse.move(box.x + box.width / 2, box.y + box.height / 2);
			await page.mouse.down();
			// A mouse's hold draws nothing until the lift: so held on the
			// page's clock, which a timer set after `HOLD_MS`'s fires behind it.
			await pageWait(page, HOLD_MS);
			await page.mouse.up();
			await mode(page).waitFor();
			assert.equal(await page.locator('[role="menu"]').count(), 0, 'the hold also opened the menu');
		}, DESKTOP);
	});

	test('a right click on a tab NOT in front enters nothing, and leaves the browser its menu', async () => {
		await withApp(async ({ page }) => {
			await page.evaluate(() => {
				window.__menus = [];
				document.addEventListener('contextmenu', (e) => window.__menus.push(e.defaultPrevented));
			});
			await page.locator('.root-bar [role="tab"][aria-selected="false"]').click({ button: 'right' });
			// The contextmenu is handled as it is dispatched (reorder.ts's `menu`),
			// so an entry it set off is drawn by the next frame: wait out whatever moves.
			await rested(page);
			assert.equal(await isOn(page), false);
			assert.deepEqual(await page.evaluate(() => window.__menus), [false], 'the browser’s menu was eaten on a tab behind');
		}, { ...DESKTOP, ...TWO });
	});

	test('a finger held and let go enters it; held and MOVED it reorders instead', async () => {
		await withApp(
			async ({ page }) => {
				const order = () =>
					page.locator('.root-bar [role="tab"]').evaluateAll((all) => all.map((b) => b.textContent.replace('•', '').trim()));
				const first = await order();
				const other = page.locator('.root-bar [role="tab"][aria-selected="false"]');
				const gap = (await other.boundingBox()).x - (await rootMenuButton(page).boundingBox()).x;
				await fingerHold(page, rootMenuButton(page), gap + 20);
				await until(order, (now) => now[0] !== first[0]);
				assert.equal(await isOn(page), false, 'a hold that moved entered layout mode');
				await fingerHold(page, rootMenuButton(page));
				await mode(page).waitFor();
			},
			{ viewport: { width: 1280, height: 900 }, device: { hasTouch: true }, ...TWO }
		);
	});

	test('a finger held, dragged away and brought back to where it started moves nothing and enters nothing (da-y8ccu)', async () => {
		await withApp(
			async ({ page }) => {
				const order = () =>
					page.locator('.root-bar [role="tab"]').evaluateAll((all) => all.map((b) => b.textContent.replace('•', '').trim()));
				const first = await order();
				await fingerHold(page, rootMenuButton(page), 120, true);
				// The lift decides at once (reorder.ts's `up`), so an entry or a
				// move it set off is drawn by the next frame: wait out whatever moves.
				await rested(page);
				assert.equal(await isOn(page), false, 'a hold that wandered and came home entered layout mode');
				assert.deepEqual(await order(), first, 'the tabs moved');
				await fingerHold(page, rootMenuButton(page));
				await mode(page).waitFor();
			},
			{ viewport: { width: 390, height: 844 }, device: { hasTouch: true }, ...TWO }
		);
	});
});

describe('the menus in layout mode', () => {
	test('a pane: Change Widget, Rename pane, Split, Break out, Widget options › what normal mode opens, Close pane', async () => {
		await withApp(async ({ page }) => {
			await menuButton(page, 'tail').click();
			// Less its Modify (da-k5hu5): in the mode, that is where the page is.
			const [modify, ...normal] = await menuNames(page);
			assert.equal(modify, 'Modify');
			await closeMenu(page);
			await enterLayoutMode(page);
			await menuButton(page, 'tail').click();
			const names = await menuNames(page);
			assert.deepEqual(names, [
				'Change Widget',
				'Rename pane',
				'Split',
				'Break out',
				...(normal.length ? ['Widget options'] : []),
				'Close pane'
			]);
			assert.equal(await page.getByRole('menuitem', { name: 'Bar', exact: true }).count(), 0);
			if (normal.length) {
				await page.getByRole('menuitem', { name: 'Widget options', exact: true }).click();
				assert.deepEqual(await menuNames(page), normal, 'Widget options is not what the name opens in normal mode');
				await page.keyboard.press('ArrowLeft');
				assert.deepEqual(await menuNames(page), names, 'ArrowLeft did not go back');
			}
			await closeMenu(page);
		}, DESKTOP);
	});

	test('the open root tab: Change Widget, Split, Taller than the screen, Close tab: its ways out are its ↺ and ✓', async () => {
		await withApp(async ({ page }) => {
			await enterLayoutMode(page);
			await rootMenuButton(page).click();
			assert.deepEqual(await menuNames(page), ['Change Widget', 'Split', 'Taller than the screen', 'Close tab']);
			await closeMenu(page);
		}, DESKTOP);
	});
});

describe('the ways out', () => {
	/** Into the mode with custom closed. */
	const closeCustom = async (page) => {
		await enterLayoutMode(page);
		await pane(page, 'custom').locator('.titlebar button.close').click();
		await until(() => titles(page), (t) => !t.includes('custom'));
	};
	const dotted = (page) => rootMenuButton(page).locator('.dot').count().then((n) => n > 0);

	for (const [how, out] of [
		['the tab’s ✓', (page) => applyButton(page).click()],
		['the bar’s ✓', (page) => page.locator('.prompt-bar .modify button.apply').click()],
		['Enter', (page) => page.keyboard.press('Enter')]
	])
		test(`${how} applies: out of the mode, the tab modified (the dot), no layout entry written, and a reload keeps it`, async () => {
			await withApp(async (app) => {
				const { page, errors } = app;
				await closeCustom(page);
				await out(page);
				await mode(page).waitFor({ state: 'detached' });
				assert.equal(await asking(page).count(), 0, 'leaving asked');
				assert.ok(!(await titles(page)).includes('custom'));
				assert.ok(await dotted(page), 'the tab is not marked modified');
				assert.deepEqual(await userLayouts(page), [], 'applying wrote a layout entry');
				await app.reload();
				assert.ok(!(await titles(page)).includes('custom'), 'a reload lost what ✓ kept');
				assert.equal(await isOn(page), false);
				assert.deepEqual(errors, []);
			}, DESKTOP);
		});

	for (const [how, out] of [
		['the tab’s ↺', (page) => resetButton(page).click()],
		['the bar’s ↺', (page) => page.locator('.prompt-bar .modify button:not(.apply)').click()],
		['Escape', (page) => page.keyboard.press('Escape')]
	])
		test(`${how} resets: out of the mode, the layout as it was, and nothing asked`, async () => {
			await withApp(async ({ page, errors }) => {
				const before = await titles(page);
				await closeCustom(page);
				await out(page);
				await mode(page).waitFor({ state: 'detached' });
				assert.equal(await asking(page).count(), 0, 'leaving asked');
				await until(() => titles(page), (t) => JSON.stringify(t) === JSON.stringify(before));
				assert.equal(await dotted(page), false, 'the tab is still marked modified');
				assert.deepEqual(errors, []);
			}, DESKTOP);
		});

	test('the ✓ has the focus as the mode is entered, and Enter on a control is that control’s', async () => {
		await withApp(async ({ page }) => {
			await enterLayoutMode(page);
			await until(() => applyButton(page).evaluate((b) => b === document.activeElement), Boolean);
			await rootMenuButton(page).focus();
			await page.keyboard.press('Enter');
			await page.locator('[role="menu"]').waitFor();
			await rested(page);
			assert.equal(await isOn(page), true, 'Enter on the tab’s name left the mode');
			await closeMenu(page);
			await rested(page);
			assert.equal(await isOn(page), true, 'the menu’s Escape left the mode');
		}, DESKTOP);
	});

	test('an Escape a dialog takes is the dialog’s: the chooser closes, and the mode stays', async () => {
		await withApp(async ({ page }) => {
			await closeCustom(page);
			await addButton(page, 'tail').click();
			await page.locator('dialog.chooser').waitFor();
			await page.keyboard.press('Escape');
			await page.locator('dialog.chooser').waitFor({ state: 'detached' });
			// The mode takes a key a task after it (mode.ts's `onKey`): wait out whatever it moves.
			await rested(page);
			assert.equal(await isOn(page), true, 'the chooser’s Escape also left the mode');
			assert.ok(!(await titles(page)).includes('custom'), 'the chooser’s Escape also reset');
		}, DESKTOP);
	});

	test('after ✓ the tab menu’s Save writes the layout entry and the dot goes', async () => {
		await withApp(async ({ page }) => {
			await closeCustom(page);
			await leaveLayoutMode(page, 'apply');
			await rootMenuButton(page).click();
			await page.getByRole('menuitem', { name: 'Save', exact: true }).click();
			await until(() => userLayouts(page), (names) => names.includes('main'));
			await until(() => dotted(page), (d) => !d);
		}, DESKTOP);
	});

	test('the other tabs are still: a press on one moves nothing, and they bear no button', async () => {
		await withApp(async ({ page }) => {
			await closeCustom(page);
			const other = page.locator('.root-bar [role="tab"][aria-selected="false"]');
			assert.equal(await other.getAttribute('aria-disabled'), 'true');
			// aria-disabled, which Playwright would wait out: forced, as a finger is not.
			await other.click({ force: true });
			await rested(page);
			assert.equal(await isOn(page), true);
			assert.equal(await asking(page).count(), 0);
			assert.ok((await rootMenuButton(page).textContent()).includes('main'), 'the press moved to another tab');
			assert.equal(await page.locator('.root-bar .tabs .way-out').count(), 2, 'a way out on another tab');
		}, { ...DESKTOP, ...TWO });
	});
});

describe('the bar in modify mode', () => {
	for (const [name, device] of [
		['desktop', DESKTOP],
		['phone', PHONE]
	])
		test(`${name}: a «modify mode» pill between ↺ and ✓, and the rest of the bar inert and dimmed`, async () => {
			await withApp(async ({ page }) => {
				await enterLayoutMode(page);
				const pill = page.locator('.prompt-bar .modify');
				assert.deepEqual(
					await pill.evaluate((el) => [...el.children].map((c) => c.textContent.trim())),
					['↺', 'modify mode', '✓']
				);
				const input = page.locator('.prompt-bar input');
				assert.equal(await input.evaluate((el) => el.closest('[inert]') !== null), true, 'the box is not inert');
				assert.equal(await pill.evaluate((el) => el.closest('[inert]') === null), true, 'the pill is inert');
				assert.ok(Number(await page.locator('.prompt-bar .section.input').evaluate((el) => getComputedStyle(el).opacity)) < 1, 'the input row is not dimmed');
				await input.focus();
				assert.equal(await input.evaluate((el) => el === document.activeElement), false, 'the box took the focus');
				await leaveLayoutMode(page, 'apply');
				assert.equal(await pill.count(), 0);
				assert.equal(await input.evaluate((el) => el.closest('[inert]') === null), true, 'the box is still inert');
			}, device);
		});
});

describe('widgets in modify mode', () => {
	test('they are inert and dimmed, and a press on collect’s field logs nothing; out of the mode the same press logs', async () => {
		await withApp(async (app) => {
			const { page } = app;
			assert.equal((await app.run(['setup', 'coffee', '--verb', 'occur'])).code, 0);
			await app.redraw();
			const field = page.locator('section.pane[data-title="collect"] .bar [data-field="coffee"]');
			await field.waitFor();
			await enterLayoutMode(page);
			assert.equal(await field.evaluate((el) => el.closest('[inert]') !== null), true, 'collect is not inert');
			assert.ok(
				Number(await field.evaluate((el) => getComputedStyle(el.closest('.tab-body')).opacity)) < 1,
				'collect is not dimmed'
			);
			// The pane's own name is no widget's: it stays bright, and opens its menu.
			assert.equal(await menuButton(page, 'tail').evaluate((el) => el.closest('[inert]')), null);
			await pressMiddle(page, field);
			await rested(page);
			assert.equal((await app.allEntries()).filter((e) => e.field === 'coffee').length, 0, 'a press in modify mode logged');
			await leaveLayoutMode(page, 'reset');
			await pressMiddle(page, field);
			await until(async () => (await app.allEntries()).filter((e) => e.field === 'coffee').length, (n) => n === 1);
		}, DESKTOP);
	});
});

// A WIDGET'S MODIFY MODE (da-k5hu5), vividn, 2026-11-10: «individual widgets
// should have a modify mode too, which you can access by tapping and holding
// on the widget title, or an option "Modify" in the widget menu», and «remove
// the (x) button to remove tabs on multi tab panes, instead we can do that
// through the widget menu».
describe('a widget’s modify mode', () => {
	const scoped = (page) => page.locator('section.pane.editing');
	const ways = (page, id) => pane(page, id).locator('> .titlebar .way-out');
	/** A long press on `locator`: a finger's through CDP, or the mouse's. */
	async function hold(page, locator, finger) {
		const box = await locator.boundingBox();
		const [x, y] = [box.x + box.width / 2, box.y + box.height / 2];
		if (!finger) {
			await page.mouse.move(x, y);
			await page.mouse.down();
			await page.waitForTimeout(HOLD_MS + 150);
			await page.mouse.up();
			return;
		}
		const cdp = await page.context().newCDPSession(page);
		try {
			await cdp.send('Input.dispatchTouchEvent', { type: 'touchStart', touchPoints: [{ x, y }] });
			await page.waitForTimeout(HOLD_MS + 150);
			await cdp.send('Input.dispatchTouchEvent', { type: 'touchEnd', touchPoints: [] });
		} finally {
			await cdp.detach();
		}
	}
	/** What the page shows at the `bar` pane's scope: asserted on every way in. */
	async function atBar(page, finger = false) {
		await mode(page).waitFor();
		assert.deepEqual(await scoped(page).evaluateAll((all) => all.map((p) => p.dataset.pane)), ['bar'], 'not the one pane worked');
		assert.deepEqual(
			await pane(page, 'bar').locator('> .titlebar .along').evaluate((el) =>
				[...el.children].filter((c) => c.matches('.way-out, .titlebox, .tabs')).map((c) => (c.matches('.way-out') ? c.textContent.trim() : 'name'))
			),
			['↺', 'name', '✓'],
			'not ↺ before the name and ✓ after it'
		);
		assert.equal(await page.locator('.root-bar .way-out').count(), 0, 'the root tab wears a way out');
		assert.equal(await page.locator('.root-bar .tabs').evaluate((el) => el.inert && Number(getComputedStyle(el).opacity) < 1), true, 'the root tabs are live');
		const others = page.locator('section.pane:not(.editing) > .titlebar');
		assert.ok((await others.count()) > 0);
		assert.deepEqual(
			[...new Set(await others.evaluateAll((all) => all.map((t) => t.inert && Number(getComputedStyle(t).opacity) < 1)))],
			[true],
			'another pane’s bar is live or bright'
		);
		const body = (sel) => page.locator(`${sel} > .content > .tab-body`).first();
		assert.equal(await body('section.pane.editing').evaluate((el) => el.inert && getComputedStyle(el).opacity === '1'), true, 'its widget is live, or dimmed');
		assert.equal(await body('section.pane:not(.editing)').evaluate((el) => el.inert && Number(getComputedStyle(el).opacity) < 1), true, 'another widget is live, or bright');
		assert.equal(await page.locator('[role="separator"]').count(), 0, 'a border outside it is a handle');
		// The ✓ has the focus, for Enter; a finger's lift gives it to the title it lifted from.
		if (!finger) await until(() => ways(page, 'bar').last().evaluate((b) => b === document.activeElement), Boolean);
		assert.equal(await page.locator('.prompt-bar .modify').count(), 1, 'no «modify mode» pill');
	}

	for (const [name, device, finger] of [
		['desktop', DESKTOP, false],
		['phone', PHONE, true]
	]) {
		test(`${name}: a long press on a widget’s title enters it at that widget: ↺ and ✓ about its name, the rest dimmed and inert`, async () => {
			await withApp(async ({ page, errors }) => {
				await hold(page, menuButton(page, 'bar'), finger);
				await atBar(page, finger);
				assert.equal(await page.locator('[role="menu"], dialog.chooser[open]').count(), 0, 'the hold opened something');
				assert.deepEqual(errors, []);
			}, device);
		});

		test(`${name}: Modify, first in a widget’s menu, enters it at that widget`, async () => {
			await withApp(async ({ page }) => {
				await menuButton(page, 'bar').click();
				assert.equal((await menuNames(page))[0], 'Modify');
				await page.getByRole('menuitem', { name: 'Modify', exact: true }).click();
				await atBar(page);
			}, device);
		});
	}

	const dotted = (page) => rootMenuButton(page).locator('.dot').count().then((n) => n > 0);
	const into = async (page) => {
		await menuButton(page, 'bar').click({ button: 'right' });
		await mode(page).waitFor();
		await moveBar(page, 'bar', 'bottom');
	};
	for (const [how, out, kept] of [
		['its ✓', (page) => ways(page, 'bar').last().click(), true],
		['the bar’s ✓', (page) => page.locator('.prompt-bar .modify button.apply').click(), true],
		['Enter', (page) => page.keyboard.press('Enter'), true],
		['its ↺', (page) => ways(page, 'bar').first().click(), false],
		['the bar’s ↺', (page) => page.locator('.prompt-bar .modify button:not(.apply)').click(), false],
		['Escape', (page) => page.keyboard.press('Escape'), false]
	])
		test(`${how} ${kept ? 'applies' : 'resets'} what was done to it`, async () => {
			await withApp(async ({ page, errors }) => {
				await into(page);
				if (how === 'Enter') await ways(page, 'bar').last().focus();
				await out(page);
				await mode(page).waitFor({ state: 'detached' });
				assert.equal(await page.locator('section.pane.editing').count(), 0);
				assert.equal(await pane(page, 'bar').evaluate((el) => el.classList.contains('edge-bottom')), kept, 'the bar is not where it should be');
				assert.equal(await dotted(page), kept, 'the dot says otherwise');
				assert.deepEqual(errors, []);
			}, DESKTOP);
		});

	test('a widget scope whose pane closes widens to the tab’s', async () => {
		await withApp(async ({ page }) => {
			await menuButton(page, 'bar').click({ button: 'right' });
			await mode(page).waitFor();
			await pane(page, 'bar').locator('> .titlebar button.close').click();
			await until(() => titles(page), (t) => !t.includes('widgetbar'));
			assert.equal(await isOn(page), true, 'closing its pane left the mode');
			assert.equal(await page.locator('.root-bar .tabs .way-out').count(), 2, 'not at the tab’s scope');
			assert.equal(await scoped(page).count(), await page.locator('section.pane').count());
		}, DESKTOP);
	});

	for (const [name, device, id, setup] of [
		['desktop', DESKTOP, 'tail', (page) => addTab(page, 'tail', 'nowview')],
		['phone', PHONE, 'side', enterLayoutMode]
	])
		test(`${name}: a pane of several tabs has no ×, and its menu’s Close tab closes the one in front`, async () => {
			await withApp(async ({ page }) => {
				await setup(page);
				const tabs = pane(page, id).locator('> .titlebar [role="tab"]');
				const many = await tabs.count();
				assert.ok(many > 1);
				assert.equal(await pane(page, id).locator('> .titlebar button.close').count(), 0, 'a × on a pane of several tabs');
				await menuButton(page, id).click();
				await page.getByRole('menuitem', { name: 'Close tab', exact: true }).click();
				await until(() => tabs.count(), (n) => n === many - 1 || (many === 2 && n === 0));
			}, device);
		});
});

describe('the bar handle', () => {
	test('in layout mode the maximize is a handle: dragged to the bottom right, the bar goes there', async () => {
		await withApp(async ({ page }) => {
			await moveBar(page, 'tail', 'bottom', 'end');
			assert.equal(await pane(page, 'tail').locator('> .titlebar > .along').getAttribute('class').then((c) => c.includes('align-end')), true);
			await page.locator(`section[data-pane="tail"] .maximize`).press('ArrowLeft');
			await pane(page, 'tail').and(page.locator('.edge-left')).waitFor();
		}, DESKTOP);
	});
});
