// The layout engine's pane menu (da-jdop, da-nk4k, da-13bc, da-9vtw): what
// opens it — the pane's TITLE, or the tab in front — its items, the
// `Bar ▸` submenu, the keyboard and the pointer. Driven through the built
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
	addButton,
	addTab,
	barMenu,
	boxOf,
	chooseBar,
	choose,
	chooser,
	draft,
	exactly,
	menuButton,
	menuLines,
	offers,
	openBar,
	openChooser,
	pane,
	settled,
	titles
} from './support/layout.mjs';

before(startApp);
after(stopApp);

describe('the menu the title opens', () => {
	/** The menu of the default layout's tail pane: one tab, in the side column. */
	const TAIL_MENU = ['Choose…', 'Split ← → ↑ ↓', 'Break out ← → ↑ ↓', 'Close pane', 'Bar'];

	test('the title is the button, and Choose… is the first item', async () => {
		await withApp(async ({ page }) => {
			const title = menuButton(page, 'tail');
			assert.match(await title.getAttribute('class'), /\btitle\b/);
			assert.equal(await title.getAttribute('aria-label'), 'menu of the tail pane');
			assert.equal(await pane(page, 'tail').locator('.titlebar button.more').count(), 0, 'the … badge is still there');
			await title.click();
			assert.deepEqual(await menuLines(page), TAIL_MENU);
			// Choose… fills this tab from the one modal (da-uyx1): the pane
			// becomes what was picked, and nothing is added beside it.
			await page.getByRole('menuitem', { name: 'Choose…', exact: true }).click();
			await chooser(page).waitFor();
			await chooser(page).locator('button').filter({ hasText: exactly('aliases') }).click();
			await chooser(page).waitFor({ state: 'detached' });
			assert.deepEqual(await titles(page), ['dayview', 'repl', 'nowview', 'aliases', 'command', 'fields']);
		}, DESKTOP);
	});

	test('each Split arrow puts the new pane on its own side', async () => {
		await withApp(async ({ page }) => {
			await choose(page, 'tail', 'split: a new pane below');
			assert.deepEqual(await titles(page), ['dayview', 'repl', 'nowview', 'tail', 'empty', 'command', 'fields']);
			const empty = page.locator('section.pane[data-title="empty"]');
			const [tail, made] = await Promise.all([boxOf(pane(page, 'tail')), boxOf(empty)]);
			assert.equal(Math.round(made.x), Math.round(tail.x));
			assert.ok(made.y > tail.y + tail.height, 'the new pane is not under the tail');
			// One button, not a grid of them (da-uyx1) — and it opens the
			// chooser: the layouts it could hold first (`day` is open above
			// this pane, so it is not offered — da-0a5o), then the kinds.
			assert.deepEqual(await empty.locator('.body button').allInnerTexts(), ['Choose Widget/Layout']);
			await openChooser(page);
			assert.deepEqual(await offers(page), [
				'dayview',
				'repl',
				'nowview',
				'tail',
				'command',
				'stats',
				'fields',
				'aliases'
			]);
			await chooser(page).locator('button', { hasText: 'nowview' }).click();
			// The new pane is titled by its kind and draws: a second nowview
			// under the tail, at its own box's size.
			await page.locator('section.pane[data-title="nowview"]').nth(1).locator('svg.svg').waitFor();
			assert.deepEqual(await titles(page), ['dayview', 'repl', 'nowview', 'tail', 'nowview', 'command', 'fields']);
		}, DESKTOP);
	});

	test('← puts it on the other side of the same pane', async () => {
		await withApp(async ({ page }) => {
			await choose(page, 'tail', 'split: a new pane to the left');
			const [tail, made] = await Promise.all([
				boxOf(pane(page, 'tail')),
				boxOf(page.locator('section.pane[data-title="empty"]'))
			]);
			assert.ok(made.x + made.width <= tail.x + 1, 'the new pane is not left of the tail');
			assert.equal(Math.round(made.y), Math.round(tail.y));
		}, DESKTOP);
	});

	// The arrows are the SCREEN's, not the bar's: the menu is drawn on
	// `document.body` and reads the screen's way whatever the bar does.
	test('a rotated bar does not turn the arrows', async () => {
		await withApp(async ({ page }) => {
			await chooseBar(page, 'tail', 'left');
			await choose(page, 'tail', 'split: a new pane to the right');
			const [tail, made] = await Promise.all([
				boxOf(pane(page, 'tail')),
				boxOf(page.locator('section.pane[data-title="empty"]'))
			]);
			assert.ok(made.x >= tail.x + tail.width - 1, 'the new pane is not right of the tail');
			assert.equal(Math.round(made.y), Math.round(tail.y));
		}, DESKTOP);
	});

	test('Break out takes the widget out of its grouping, and again out of the next', async () => {
		await withApp(async ({ page }) => {
			// The tail sits in the side column, beside nowview; ← takes it out
			// of that column and lays it down the page beside the whole thing.
			await choose(page, 'tail', 'break out: this widget to the left');
			const [tail, nowview] = await Promise.all([boxOf(pane(page, 'tail')), boxOf(pane(page, 'nowview'))]);
			assert.ok(tail.x + tail.width <= nowview.x + 1, 'the tail is not left of what it left');
			assert.ok(tail.height > nowview.height, 'the tail did not take the column’s whole height');
			// Again: out of the row it landed in, to the left of the page.
			await choose(page, 'tail', 'break out: this widget to the left');
			const [again, strip] = await Promise.all([boxOf(pane(page, 'tail')), boxOf(pane(page, 'strip'))]);
			assert.ok(again.x < strip.x, 'the tail is not left of the day strip');
			assert.ok(again.height > tail.height, `the tail is ${again.height}px tall, was ${tail.height}`);
		}, DESKTOP);
	});

	test('the + adds a tab holding what the chooser came back with; dismissed, it adds nothing', async () => {
		await withApp(async ({ page }) => {
			await addButton(page, 'tail').click();
			await chooser(page).waitFor();
			await page.keyboard.press('Escape');
			await chooser(page).waitFor({ state: 'detached' });
			assert.equal(await pane(page, 'tail').locator('[role="tab"]').count(), 0, 'a dismissed chooser added a tab');

			await addTab(page, 'tail', 'aliases');
			const tabs = pane(page, 'tail').locator('[role="tab"]');
			assert.deepEqual(await tabs.allInnerTexts(), ['tail', 'aliases']);
			assert.equal(await activeTab(page, 'tail'), 'aliases');
			assert.equal(await pane(page, 'tail').locator('h2').count(), 0, 'a tabbed pane has no h2');
			assert.equal(await pane(page, 'tail').getAttribute('data-title'), 'aliases', 'the tab is not what was chosen');
		}, DESKTOP);
	});

	test('the tab in front opens the menu; an inactive tab just switches', async () => {
		await withApp(async ({ page }) => {
			await addTab(page, 'tail', 'aliases');
			const tabs = pane(page, 'tail').locator('[role="tab"]');
			// The tab in front carries the menu, and its name is still its words.
			assert.equal(await tabs.nth(1).getAttribute('aria-haspopup'), 'menu');
			assert.equal(await tabs.nth(1).getAttribute('aria-label'), null);
			await tabs.nth(0).click();
			assert.equal(await page.locator('[role="menu"]').count(), 0, 'switching opened the menu');
			assert.equal(await activeTab(page, 'tail'), 'tail');
			// Now the tail is the tab in front: the same click opens the menu.
			await tabs.nth(0).click();
			await page.locator('[role="menu"]').waitFor();
			// With tabs it is the TAB that closes, and Break out gives the
			// tab in front a pane of its own.
			assert.deepEqual(await menuLines(page), ['Choose…', 'Split ← → ↑ ↓', 'Break out ← → ↑ ↓', 'Close tab', 'Bar']);
			await page.getByRole('menuitem', { name: 'break out: this widget below', exact: true }).click();
			await page.locator('[role="menu"]').waitFor({ state: 'detached' });
			// The tab that left is a pane of its own, under the pane it left;
			// the other stayed behind, which is that pane's whole strip now.
			assert.equal(await pane(page, 'tail').locator('[role="tab"]').count(), 0);
			assert.deepEqual(await titles(page), ['dayview', 'repl', 'nowview', 'aliases', 'tail', 'command', 'fields']);
			const [stayed, gone] = await Promise.all([
				boxOf(page.locator('section.pane[data-title="aliases"]')),
				boxOf(page.locator('section.pane[data-title="tail"]'))
			]);
			assert.ok(gone.y > stayed.y, 'the tab that broke out is not below the pane it left');
		}, DESKTOP);
	});

	// The last pane cannot be closed — `tree.test.mjs` holds `closeTab` to
	// it — and the root frame, which used to grey the item, draws no bar
	// of its own now (da-2a5y): its tabs are the shell's top bar.
	test('the root frame draws no bar: a lone root widget has no title, no menu and no inset', async () => {
		await withApp(
			async ({ page }) => {
				assert.deepEqual(await titles(page), ['repl']);
				const root = page.locator('section.root');
				assert.equal(await root.locator('.titlebar').count(), 0);
				assert.equal(await root.locator('button.title, button.add').count(), 0);
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
			await menuButton(page, 'tail').click();
			const items = page.locator('[role="menu"] button');
			assert.deepEqual(await menuLines(page), ['Choose…', 'Split ← → ↑ ↓', 'Break out ← → ↑ ↓', 'Close pane', 'Bar']);
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
			const more = menuButton(page, 'tail');
			const focused = () =>
				page.evaluate(() => document.activeElement.getAttribute('aria-label') ?? document.activeElement.textContent.trim());
			/** From the title: open, ArrowDown until `Bar` is focused. */
			async function toBar() {
				await page.keyboard.press('Enter');
				for (let i = 0; i < 16 && (await focused()) !== 'Bar'; i++) await page.keyboard.press('ArrowDown');
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
			assert.ok(await more.evaluate((el) => el === document.activeElement), 'choosing did not hand focus to the title');

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
			assert.ok(await more.evaluate((el) => el === document.activeElement), 'Escape twice did not reach the title');

			await toBar();
			await page.keyboard.press('ArrowRight');
			await page.keyboard.press('Tab');
			assert.equal(await page.locator('[role="menu"]').count(), 0, 'Tab left a menu open');
		}, DESKTOP);
	});
});

// "bar submenu should open immediately on mouse over" and "let's put the
// two split buttons on the same line" (da-13bc) — four of them now (da-9vtw).
describe('the pane menu by pointer', () => {
	const barItem = (page) => page.getByRole('menuitem', { name: /^Bar/ });

	test('Bar ▸ opens on the pointer with no click, stays open on the way to it, and closes on another item', async () => {
		await withApp(async ({ page }) => {
			await menuButton(page, 'tail').click();
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
			await menuButton(page, 'repl').tap();
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
			await menuButton(page, 'tail').focus();
			await page.keyboard.press('Enter');
			// Choose… first, then the Split row.
			assert.equal(await focused(), 'Choose…');
			await page.keyboard.press('ArrowDown');
			assert.equal(await focused(), 'split: a new pane to the left');
			const walk = [];
			for (const key of ['ArrowLeft', 'ArrowRight', 'ArrowRight', 'ArrowRight', 'ArrowRight', 'ArrowLeft']) {
				await page.keyboard.press(key);
				walk.push(await focused());
			}
			assert.deepEqual(walk, [
				'split: a new pane to the left',
				'split: a new pane to the right',
				'split: a new pane above',
				'split: a new pane below',
				'split: a new pane below',
				'split: a new pane above'
			]);
			// And ArrowDown from the row's last button goes on to the next
			// item — the four arrows of Break out, then Close pane.
			await page.keyboard.press('ArrowRight');
			await page.keyboard.press('ArrowRight');
			await page.keyboard.press('ArrowDown');
			assert.equal(await focused(), 'break out: this widget to the left');
			for (let i = 0; i < 4; i++) await page.keyboard.press('ArrowDown');
			assert.equal(await focused(), 'Close pane');
		}, DESKTOP);
	});

	test('Split is one line of four arrows, and ↓ splits below', async () => {
		await withApp(async ({ page }) => {
			await menuButton(page, 'tail').click();
			const buttons = page.getByRole('group', { name: 'Split' }).getByRole('menuitem');
			assert.deepEqual(await buttons.evaluateAll((all) => all.map((b) => `${b.textContent.trim()} ${b.getAttribute('aria-label')}`)), [
				'← split: a new pane to the left',
				'→ split: a new pane to the right',
				'↑ split: a new pane above',
				'↓ split: a new pane below'
			]);
			const boxes = await Promise.all([0, 1, 2, 3].map((i) => boxOf(buttons.nth(i))));
			for (const b of boxes) {
				assert.equal(b.y, boxes[0].y, 'the four are not on one line');
				assert.ok(b.width >= 24 && b.height >= 24, `a split button is ${b.width}x${b.height}`);
			}
			await buttons.nth(3).click();
			await page.locator('[role="menu"]').waitFor({ state: 'detached' });
			assert.deepEqual(await titles(page), ['dayview', 'repl', 'nowview', 'tail', 'empty', 'command', 'fields']);
			const [tail, empty] = await Promise.all([boxOf(pane(page, 'tail')), boxOf(page.locator('section.pane[data-title="empty"]'))]);
			assert.equal(Math.round(empty.x), Math.round(tail.x));
			assert.equal(Math.round(empty.width), Math.round(tail.width));
			assert.ok(empty.y > tail.y + tail.height, 'the new pane is not under the tail');
		}, DESKTOP);
	});
});
