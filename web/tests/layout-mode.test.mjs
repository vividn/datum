// LAYOUT MODE (da-vvcxc), which replaced the padlock, the per-device lock
// and every `▾`. vividn, 2026-10-14: «long pressing (and release without
// dragging to reorder the tabs) or right clicking on a primary tab name
// enters unlocked/layout mode. This should be very visually distinctive.»
//
// What this file holds the change to: a page opens in NORMAL mode, where
// nothing moves the tree and no `▾` or padlock is drawn; each way in — a
// right click, Shift+F10, a mouse's long press, a finger's — enters a mode
// the page shows; a hold that MOVES still reorders; the layout-mode menus
// hold exactly what vividn listed; Esc Esc discards and Esc Enter accepts;
// moving to another tab with a change in hand asks; Save writes a layout
// entry and Accept does not, though Accept survives a reload; and the
// maximize is the bar's handle.

import test, { before, after, describe } from 'node:test';
import assert from 'node:assert/strict';
import { startApp, stopApp, withApp } from './support/app.mjs';
import {
	DESKTOP,
	PHONE,
	addButton,
	enterLayoutMode,
	leaveLayoutMode,
	menuButton,
	moveBar,
	pane,
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
/** The layout-mode dialog that asks how to leave. */
const asking = (page) => page.locator('dialog.confirm[open]');
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
		test(`${width} wide: a first visit's four tabs and their × are all reached in the strip in layout mode`, async (t) => {
			await withApp(async ({ page }) => {
				const rest = await page.locator('.root-bar .tabs').evaluate((el) => [el.scrollWidth, el.clientWidth]);
				t.diagnostic(`${width} wide, out of layout mode: the strip is ${rest[0]} of ${rest[1]}px`);
				await enterLayoutMode(page);
				const at = await page.locator('.root-bar .tabs').evaluate((el) => {
					const strip = el.getBoundingClientRect();
					const out = [...el.querySelectorAll('[role="tab"], .tab-close')]
						.filter((b) => {
							b.scrollIntoView({ inline: 'nearest', block: 'nearest', behavior: 'instant' });
							const r = b.getBoundingClientRect();
							return r.left < strip.left - 0.5 || r.right > strip.right + 0.5;
						})
						.map((b) => b.getAttribute('aria-label') ?? b.textContent.trim());
					return {
						names: [...el.querySelectorAll('[role="tab"]')].map((t) => t.textContent.replace('•', '').trim()),
						closes: el.querySelectorAll('.tab-close').length,
						out,
						scroll: el.scrollWidth,
						client: el.clientWidth
					};
				});
				assert.deepEqual(at.names, ['welcome', 'setup', 'collect', 'dayview']);
				assert.equal(at.closes, 4);
				assert.deepEqual(at.out, [], `out of reach of the strip's ${at.client}px`);
				t.diagnostic(`${width} wide: the strip is ${at.scroll} of ${at.client}px`);
			}, { ...PHONE, viewport: { width, height: 640 }, home: null });
		});
	}
});

describe('the ways in', () => {
	test('a right click on the open tab enters it, visibly: accent panes, handles in the gaps, a × on every tab; Accept leaves it', async () => {
		await withApp(async ({ page, errors }) => {
			await rootMenuButton(page).click({ button: 'right' });
			await mode(page).waitFor();
			assert.equal(await page.locator('[role="menu"]').count(), 0, 'the right click also opened the menu');
			assert.equal(await page.locator('.root-bar[data-layout-mode]').count(), 1);
			const panes = await page.locator('section.pane').count();
			assert.equal(await page.locator('section.pane.editing').count(), panes, 'a pane not drawn in the mode');
			assert.ok((await page.locator('[role="separator"]').count()) > 0, 'no handle between the panes');
			assert.equal(await page.locator('.root-bar button.tab-close').count(), 2);
			assert.ok(await addButton(page, 'tail').isVisible());
			await leaveLayoutMode(page, 'Accept');
			assert.equal(await page.locator('[role="separator"]').count(), 0);
			assert.deepEqual(errors, []);
		}, { ...DESKTOP, ...TWO });
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
			await page.waitForTimeout(200);
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
				await page.waitForTimeout(300);
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
			const normal = await menuNames(page);
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

	test('the open root tab: Change Widget, Split, Accept, Save layout, Discard changes, Close tab', async () => {
		await withApp(async ({ page }) => {
			await enterLayoutMode(page);
			await rootMenuButton(page).click();
			assert.deepEqual(await menuNames(page), ['Change Widget', 'Split', 'Accept', 'Save layout', 'Discard changes', 'Close tab']);
			await closeMenu(page);
		}, DESKTOP);
	});
});

describe('the ways out', () => {
	test('Esc Esc discards; Esc Enter accepts, writes no layout entry, and a reload keeps it', async () => {
		await withApp(async (app) => {
			const { page, errors } = app;
			const before = await titles(page);
			await enterLayoutMode(page);
			await pane(page, 'custom').locator('.titlebar button.close').click();
			await until(() => titles(page), (t) => !t.includes('custom'));
			await page.keyboard.press('Escape');
			await asking(page).waitFor();
			// Discard changes throws the change away, so it is the dialog's one
			// red button (da-ad2lt), and Accept and Save layout are the accent.
			const red = await asking(page).evaluate((d) => {
				const error = getComputedStyle(document.documentElement).getPropertyValue('--error').trim();
				const probe = document.createElement('span');
				probe.style.color = error;
				document.body.append(probe);
				const want = getComputedStyle(probe).color;
				probe.remove();
				return [...d.querySelectorAll('button')].filter((b) => getComputedStyle(b).color === want).map((b) => b.textContent.trim());
			});
			assert.deepEqual(red, ['Discard changes'], 'the red buttons of the leave dialog');
			await page.keyboard.press('Escape');
			await mode(page).waitFor({ state: 'detached' });
			await until(() => titles(page), (t) => JSON.stringify(t) === JSON.stringify(before));

			await enterLayoutMode(page);
			await pane(page, 'custom').locator('.titlebar button.close').click();
			await until(() => titles(page), (t) => !t.includes('custom'));
			await page.keyboard.press('Escape');
			await asking(page).waitFor();
			await page.keyboard.press('Enter');
			await mode(page).waitFor({ state: 'detached' });
			assert.ok(!(await titles(page)).includes('custom'));
			assert.deepEqual(await userLayouts(page), [], 'Accept wrote a layout entry');
			await app.reload();
			assert.ok(!(await titles(page)).includes('custom'), 'a reload lost what Accept kept');
			assert.equal(await isOn(page), false);
			assert.deepEqual(errors, []);
		}, DESKTOP);
	});

	test('Escape with nothing changed just leaves', async () => {
		await withApp(async ({ page }) => {
			await enterLayoutMode(page);
			await page.keyboard.press('Escape');
			await mode(page).waitFor({ state: 'detached' });
			assert.equal(await asking(page).count(), 0);
		}, DESKTOP);
	});

	test('an Escape a dialog takes is the dialog’s: the chooser closes, and the mode does not ask', async () => {
		await withApp(async ({ page }) => {
			await enterLayoutMode(page);
			await pane(page, 'custom').locator('.titlebar button.close').click();
			await addButton(page, 'tail').click();
			await page.locator('dialog.chooser').waitFor();
			await page.keyboard.press('Escape');
			await page.locator('dialog.chooser').waitFor({ state: 'detached' });
			await page.waitForTimeout(200);
			assert.equal(await asking(page).count(), 0, 'the chooser’s Escape also asked how to leave');
			assert.equal(await isOn(page), true);
		}, DESKTOP);
	});

	test('Save layout writes the layout entry and leaves', async () => {
		await withApp(async ({ page }) => {
			await enterLayoutMode(page);
			await pane(page, 'custom').locator('.titlebar button.close').click();
			await rootMenuButton(page).click();
			await page.getByRole('menuitem', { name: 'Save layout', exact: true }).click();
			await mode(page).waitFor({ state: 'detached' });
			await until(() => userLayouts(page), (names) => names.includes('main'));
		}, DESKTOP);
	});

	test('moving to another tab with a change in hand asks; Discard changes puts it back and the tab moves', async () => {
		await withApp(async ({ page }) => {
			const before = await titles(page);
			await enterLayoutMode(page);
			await pane(page, 'custom').locator('.titlebar button.close').click();
			await until(() => titles(page), (t) => !t.includes('custom'));
			await page.locator('.root-bar [role="tab"][aria-selected="false"]').click();
			await asking(page).waitFor();
			await asking(page).getByRole('button', { name: 'Discard changes' }).click();
			await mode(page).waitFor({ state: 'detached' });
			await until(() => rootMenuButton(page).textContent(), (t) => t.includes('cli'));
			await page.locator('.root-bar [role="tab"][aria-selected="false"]').click();
			await until(() => titles(page), (t) => JSON.stringify(t) === JSON.stringify(before));
		}, { ...DESKTOP, ...TWO });
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
