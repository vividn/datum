// The layout engine's `…` menu (da-jdop, da-nk4k, da-13bc): its items, the
// `Bar ▸` submenu, the keyboard and the pointer — driven through the built
// app in a real browser. Read through the frame's classes, the buttons'
// words and `aria-checked`, never "the menu looks different".

import test, { before, after, describe } from 'node:test';
import assert from 'node:assert/strict';
import { startApp, stopApp, withApp } from './support/app.mjs';
import {
	DESKTOP,
	PHONE,
	ONE_PANE,
	activeTab,
	barMenu,
	boxOf,
	chooseBar,
	choose,
	draft,
	exactly,
	menuLines,
	openBar,
	pane,
	settled,
	titles
} from './support/layout.mjs';

before(startApp);
after(stopApp);

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
				'aliases',
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
			// The app's own listener was added first, so it has run by the
			// time ours has; then the page settles on whatever it did.
			await page.waitForFunction(() => window.__entered.length > 0);
			await settled(page);
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
