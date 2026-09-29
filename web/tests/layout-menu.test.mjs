// The layout engine's pane menu (da-jdop, da-nk4k, da-13bc, da-9vtw): what
// opens it — the pane's TITLE, or the tab in front — its items, the
// `Bar ▸` submenu, the keyboard and the pointer. Driven through the built
// app in a real browser. Read through the frame's classes, the buttons'
// words and `aria-checked`, never "the menu looks different".

import test, { before, after, describe } from 'node:test';
import assert from 'node:assert/strict';
import { sharedApp, startApp, stopApp, withApp } from './support/app.mjs';
import {
	DESKTOP,
	ONE_PANE,
	PHONE,
	activeTab,
	addButton,
	addTab,
	barMenu,
	boxOf,
	choose,
	chooseBar,
	chooseRoot,
	chooser,
	exactly,
	manage,
	manageButton,
	manageRoot,
	maximizeButton,
	menuButton,
	menuLines,
	offers,
	openBar,
	openChooser,
	overStrip,
	pane,
	rootManageButton,
	rootMenuButton,
	settled,
	titledPane,
	titles,
	toggleRootLock
} from './support/layout.mjs';
import { until } from './support/until.mjs';

before(startApp);
after(stopApp);

// ONE APP per screen for most of this file (da-b7sfp, sharedApp in
// support/app.mjs): a context is 2.5 s of boot, and nothing here reads a
// ROW, so what a test leaves behind is layout alone, which `solo` puts back
// to `main` before the next one — its lock too, and a `ONE_PANE` draft is
// `solo`'s to open (da-6b3dj). What still opens an app of its own leaves
// the root WITHOUT `main` (Save layout as, Change Widget at the root) or
// starts on a fresh phone, neither of which `solo` can undo.
const shared = sharedApp(DESKTOP);
const phone = sharedApp(PHONE);

describe('the menu the title opens', () => {
	/**
	 * The default layout's tail pane, in two slices (da-bzhq): what its
	 * NAME opens, and what the `▾` beside the `+` opens.
	 *
	 * `Reset widget` and the widget's own options are in the NAME slice
	 * since da-hkjp gave the tail kind a declared `type`: both are the
	 * registry's arithmetic over what a kind declares, so a kind that
	 * declares something has them. The kind with neither is `custom`, which
	 * the tests below that need one now use — and which since da-z8klv has
	 * NO name menu at all, nothing being left in that slice. The `▾` slice
	 * is untouched by da-hkjp — it is about the PANE, not the widget in it
	 * — and `Rename pane` is second in it since da-s7sdm, which is the
	 * same distinction from the other side: renaming is done TO the pane.
	 * `Change Widget` leads it since da-z8klv: WHICH widget sits in the
	 * pane is done to the pane too (vividn: "change widget belongs in the
	 * triangle menu").
	 *
	 * The option is the ITEM `type` rather than an `Options…` that opens a
	 * modal (da-vhwq): a choice is a submenu of its values, opened from the
	 * row that names the setting.
	 */
	const TAIL_MENU = ['Reset widget', 'type'];
	const TAIL_MANAGE = ['Change Widget', 'Rename pane', 'Split ← ↓ ↑ →', 'Break out ← ↓ ↑ →', 'Bar', '× Close pane'];

	test('the title is the button, the `▾` beside the `+` holds the acts on the pane', async () => {
		await shared.solo(async ({ page }) => {
			const title = menuButton(page, 'tail');
			assert.match(await title.getAttribute('class'), /\btitle\b/);
			assert.equal(await title.getAttribute('aria-label'), 'menu of the tail pane');
			assert.equal(await pane(page, 'tail').locator('.titlebar button.more').count(), 0, 'the … badge is still there');

			// The `▾` (da-bzhq) is named, and its menu is the other slice.
			const badge = manageButton(page, 'tail');
			assert.equal(await badge.getAttribute('aria-label'), 'manage the tail pane');
			await title.click();
			assert.deepEqual(await menuLines(page), TAIL_MENU);
			await page.keyboard.press('Escape');
			await page.locator('[role="menu"]').waitFor({ state: 'detached' });

			await badge.click();
			assert.deepEqual(await menuLines(page), TAIL_MANAGE);
			assert.equal(await page.locator('[role="menu"]').getAttribute('aria-label'), 'manage the tail pane');
			// Change Widget fills this tab from the one modal (da-uyx1): the
			// pane becomes what was picked, and nothing is added beside it.
			await page.getByRole('menuitem', { name: 'Change Widget', exact: true }).click();
			await chooser(page).waitFor();
			await chooser(page).locator('button').filter({ hasText: exactly('aliases') }).click();
			await chooser(page).waitFor({ state: 'detached' });
			assert.deepEqual(await titles(page), ['dayview', 'repl', 'nowview', 'aliases', 'custom', 'fields']);
		});
	});

	// vividn (da-ygc9): "let's add an icon on Close pane, and move close pane
	// to be the last entry in the menu." Last means last — after the acts
	// and after anything a root adds of its own — in BOTH menus, since one
	// builder makes them (`menu.ts`). The × is decoration, so the item's
	// name is still its label alone and the keyboard still finds it there.
	test('Close is the last item of either ▾, and wears an ×', async () => {
		await shared.solo(async ({ page }) => {
			await manageButton(page, 'tail').click();
			const lines = await menuLines(page);
			assert.equal(lines.at(-1), '× Close pane');
			const close = page.getByRole('menuitem', { name: 'Close pane', exact: true });
			assert.equal(await close.locator('.glyph').innerText(), '×');
			assert.equal(await close.locator('.glyph').getAttribute('aria-hidden'), 'true');
			await page.keyboard.press('Escape');
			await page.locator('[role="menu"]').waitFor({ state: 'detached' });

			// The NAME's menu ends on the layout's acts — `export backup…`
			// was its last item until it became the gear's «Backups»
			// (da-9ow8m) — and Close is the last of the `▾`, which is where
			// the acts on the page are (da-bzhq).
			await rootMenuButton(page).click();
			assert.equal((await menuLines(page)).at(-1), 'Save layout as');
			await page.keyboard.press('Escape');
			await page.locator('[role="menu"]').waitFor({ state: 'detached' });
			await rootManageButton(page).click();
			assert.equal((await menuLines(page)).at(-1), '× Close tab');
			await page.keyboard.press('Escape');
		});
	});

	// da-ywp1: a pane's bar is a LINE on the pane's outline, so its title
	// straddles that line — and on a pane at the page's outer edge the
	// outer half hangs into the page's margin (`.frame`'s 0.75rem padding,
	// which is where the page's air moved in da-ckw3 so that `main` can
	// reach zero under a fully-pulled log). That half was under one of the four `…` edge handles
	// (da-qk2k, z-index 3), which took the pointer and left the title
	// unclickable there; the handles went with the root menu (da-wjob),
	// and this holds the margin clear for the half-row the bars hang into.
	test("a title hanging into the page's margin still takes the pointer", async () => {
		await shared.solo(async ({ page }) => {
			await chooseBar(page, 'tail', 'right');
			await settled(page);
			const [title, inner] = await Promise.all([
				boxOf(menuButton(page, 'tail')),
				page
					.locator('main > .frame')
					.evaluate((m) => m.getBoundingClientRect().right - parseFloat(getComputedStyle(m).paddingRight))
			]);
			const x = title.x + title.width - 1;
			const y = title.y + title.height / 2;
			assert.ok(x > inner, `the title's outer edge (${x}) does not reach the page's margin (past ${inner})`);
			const over = await page.evaluate(
				([x, y]) => document.elementFromPoint(x, y)?.closest('button')?.className ?? 'nothing',
				[x, y]
			);
			assert.match(over, /\btitle\b/, `what lies over the title's outer half is ${over}`);
			await page.mouse.click(x, y);
			assert.deepEqual(await menuLines(page), TAIL_MENU);
			await page.keyboard.press('Escape');
		});
	});

	test('each Split arrow puts the new pane on its own side', async () => {
		await shared.solo(async ({ page }) => {
			await manage(page, 'tail', 'split: a new pane below');
			assert.deepEqual(await titles(page), ['dayview', 'repl', 'nowview', 'tail', 'empty', 'custom', 'fields']);
			const empty = page.locator('section.pane[data-title="empty"]');
			const [tail, made] = await Promise.all([boxOf(pane(page, 'tail')), boxOf(empty)]);
			assert.equal(Math.round(made.x), Math.round(tail.x));
			assert.ok(made.y > tail.y + tail.height, 'the new pane is not under the tail');
			// One button, not a grid of them (da-uyx1) — and it opens the
			// chooser: the layouts it could hold first (`main` is open above
			// this pane, so it is not offered — da-0a5o), then the kinds.
			assert.deepEqual(await empty.locator('.body button').allInnerTexts(), ['Change Widget']);
			await openChooser(page);
			assert.deepEqual(await offers(page), [
				'dayview',
				'repl',
				'nowview',
				'tail',
				'custom',
				'command',
				'stats',
				'fields',
				'aliases',
				'sync',
				'help',
				'backups',
				'input'
			]);
			await chooser(page).locator('button', { hasText: 'nowview' }).click();
			// The new pane is titled by its kind and draws: a second nowview
			// under the tail, at its own box's size.
			await page.locator('section.pane[data-title="nowview"]').nth(1).locator('svg.svg').waitFor();
			assert.deepEqual(await titles(page), ['dayview', 'repl', 'nowview', 'tail', 'nowview', 'custom', 'fields']);
		});
	});

	test('← puts it on the other side of the same pane', async () => {
		await shared.solo(async ({ page }) => {
			await manage(page, 'tail', 'split: a new pane to the left');
			const [tail, made] = await Promise.all([
				boxOf(pane(page, 'tail')),
				boxOf(page.locator('section.pane[data-title="empty"]'))
			]);
			assert.ok(made.x + made.width <= tail.x + 1, 'the new pane is not left of the tail');
			assert.equal(Math.round(made.y), Math.round(tail.y));
		});
	});

	// The arrows are the SCREEN's, not the bar's: the menu is drawn on
	// `document.body` and reads the screen's way whatever the bar does.
	test('a rotated bar does not turn the arrows', async () => {
		await shared.solo(async ({ page }) => {
			await chooseBar(page, 'tail', 'left');
			await manage(page, 'tail', 'split: a new pane to the right');
			const [tail, made] = await Promise.all([
				boxOf(pane(page, 'tail')),
				boxOf(page.locator('section.pane[data-title="empty"]'))
			]);
			assert.ok(made.x >= tail.x + tail.width - 1, 'the new pane is not right of the tail');
			assert.equal(Math.round(made.y), Math.round(tail.y));
		});
	});

	test('Break out takes the widget out of its grouping, and again out of the next', async () => {
		await shared.solo(async ({ page }) => {
			// The tail sits in the side column, beside nowview; ← takes it out
			// of that column and lays it down the page beside the whole thing.
			await manage(page, 'tail', 'break out: this widget to the left');
			const [tail, nowview] = await Promise.all([boxOf(pane(page, 'tail')), boxOf(pane(page, 'nowview'))]);
			assert.ok(tail.x + tail.width <= nowview.x + 1, 'the tail is not left of what it left');
			assert.ok(tail.height > nowview.height, 'the tail did not take the column’s whole height');
			// Again: out of the row it landed in, to the left of the page.
			await manage(page, 'tail', 'break out: this widget to the left');
			const [again, strip] = await Promise.all([boxOf(pane(page, 'tail')), boxOf(pane(page, 'strip'))]);
			assert.ok(again.x < strip.x, 'the tail is not left of the day strip');
			assert.ok(again.height > tail.height, `the tail is ${again.height}px tall, was ${tail.height}`);
		});
	});

	// da-n632 (vividn, 2026-09-17): a tab breaking out of a stack lands beside
	// the STACK, in its own box — as Split would put a new pane there —
	// not beside the whole column it sits in.
	test('Break out of a tab stack lands the tab beside the stack, inside its row of the column', async () => {
		await phone.solo(async ({ page }) => {
			await manage(page, 'side', 'break out: this widget to the right');
			const tail = titledPane(page, 'tail');
			await tail.waitFor();
			const [stack, moved, strip, fields] = await Promise.all(
				[pane(page, 'side'), tail, pane(page, 'strip'), pane(page, 'bar')].map(boxOf)
			);
			assert.equal(await pane(page, 'side').getAttribute('data-title'), 'nowview', 'the stack did not keep its other tabs');
			assert.ok(moved.x >= stack.x + stack.width - 1, 'the tail is not right of the stack');
			assert.ok(Math.abs(moved.y - stack.y) < 2 && Math.abs(moved.height - stack.height) < 2, 'the tail is not in the stack’s row');
			assert.ok(moved.y > strip.y && moved.y < fields.y, 'the tail left the column');
			// The strip above and the fields below still span the full width, and the pair fills it.
			assert.ok(Math.abs(strip.width - fields.width) < 2, 'the strip and the fields no longer match');
			assert.ok(
				Math.abs(stack.x - strip.x) < 2 && Math.abs(moved.x + moved.width - (strip.x + strip.width)) < 2,
				`the pair does not fill the column’s width: ${JSON.stringify({ stack, moved, strip })}`
			);
		});
	});

	// da-6ji5 (vividn, 2026-09-17): «Split should split the current tab so
	// that it then has two widgets» — named from them until renamed, and
	// saved as a layout only when asked.
	test('Split in a tab stack splits the tab in front: named from its widgets, renamed, saved only when asked', async () => {
		await phone.solo(async (app) => {
			const { page } = app;
			const strip = () => pane(page, 'side').locator('> .titlebar [role="tab"]');
			const labels = () => strip().allInnerTexts();
			const names = () => page.evaluate(async () => (await window.datum.layouts.listLayouts()).map((l) => l.name).sort());
			const stored = () => page.evaluate(() => Object.keys(localStorage).filter((k) => k.startsWith('datum.layout.')).sort());
			const [namesBefore, storedBefore] = [await names(), await stored()];
			const tailId = await page.evaluate(() => JSON.stringify(window.datum.layout.current()).match(/"id":"([^"]+)","kind":"tail"/)[1]);

			await manage(page, 'side', 'split: a new pane to the right');
			await until(labels, (l) => l[0] === 'tail | empty');
			assert.deepEqual(await labels(), ['tail | empty', 'nowview', 'custom'], 'the other tabs left the strip');
			assert.equal(await strip().first().getAttribute('title'), 'tail | empty');
			const [tail, empty] = await Promise.all([titledPane(page, 'tail'), titledPane(page, 'empty')].map(boxOf));
			assert.ok(empty.x >= tail.x + tail.width - 1 && Math.abs(empty.y - tail.y) < 2, 'the empty half is not right of the tail');
			const kept = await page.evaluate((id) => JSON.stringify(window.datum.layout.current()).includes(`"id":"${id}","kind":"tail"`), tailId);
			assert.ok(kept, 'the tail widget lost its id');
			// Splitting minted no layout: nothing new saved, no new draft name.
			assert.deepEqual(await names(), namesBefore);
			// The split is an edit to `main`, the enclosing layout: its draft, its dot.
			assert.deepEqual((await stored()).filter((k) => !storedBefore.includes(k)), ['datum.layout.draft.v1.main']);
			assert.equal(await rootMenuButton(page).locator('.dot').count(), 1, 'the edit is not the enclosing layout’s');

			// The empty half filled — from its own button, an empty pane
			// having no name menu — and the name follows.
			await openChooser(page);
			await chooser(page).locator('button').filter({ hasText: exactly('aliases') }).click();
			await until(labels, (l) => l[0] === 'tail | aliases');

			// Renamed: the dialog starts at the name, and the name survives a reload and a further split.
			await manage(page, 'side', 'Rename tab');
			const field = page.locator('dialog.ask input');
			assert.equal(await field.inputValue(), 'tail | aliases');
			await field.fill('pair');
			await page.locator('dialog.ask').getByRole('button', { name: 'rename' }).click();
			await until(labels, (l) => l[0] === 'pair');
			await app.reload();
			await until(labels, (l) => l[0] === 'pair');
			await manage(page, 'side', 'split: a new pane below');
			await until(() => titledPane(page, 'empty').count(), (n) => n === 1);
			assert.deepEqual(await labels(), ['pair', 'nowview', 'custom']);
			// Emptied, the tab wears the name it is given again.
			await manage(page, 'side', 'Rename tab');
			await page.locator('dialog.ask input').fill('');
			await page.locator('dialog.ask').getByRole('button', { name: 'rename' }).click();
			await until(labels, (l) => l[0] === '(tail | aliases) / empty');
			assert.deepEqual(await names(), namesBefore, 'renaming saved a layout');

			// Save as layout, asked for: listed, and the tab wears it.
			await choose(page, 'side', 'Save as layout');
			await page.locator('dialog.ask input').fill('mine');
			await page.locator('dialog.ask').getByRole('button', { name: 'save' }).click();
			await until(labels, (l) => l[0] === 'mine');
			assert.deepEqual(await names(), [...namesBefore, 'mine'].sort());
		});
	});

	// da-s7sdm (vividn, 2026-09-21): «Yes, let's add a rename option. I
	// suppose it should go in the (lockable) layout menu». A PANE is
	// renameable, not only a tab of a strip: with one tab the tab IS the
	// pane, so the name on the frame is that tab's `title` — which
	// `nameOf` already preferred over the kind's name. The act is in the
	// `▾`, with the rest of what is DONE to a pane, so a lock hides it.
	test('Rename pane names a lone pane everywhere; emptied, the key goes with the name', async () => {
		await shared.solo(async (app) => {
			const { page } = app;
			const tabOf = () =>
				page.evaluate(() => {
					const walk = (p) => p.tabs.flatMap((t) => (t.id === 'tail-tab' ? [t] : (t.content.children ?? []).flatMap(walk)));
					return walk(window.datum.layout.current().root)[0];
				});
			const dots = () => rootMenuButton(page).locator('.dot').count();
			assert.equal(await dots(), 0, 'the layout started modified');
			assert.equal((await tabOf()).title, undefined, 'the tab is named before anyone named it');

			// The field starts at the name it wears now — the kind's.
			await manageButton(page, 'tail').click();
			await page.getByRole('menuitem', { name: 'Rename pane', exact: true }).click();
			const field = page.locator('dialog.ask input');
			assert.equal(await field.inputValue(), 'tail');
			await field.fill('sleep tail');
			await page.locator('dialog.ask').getByRole('button', { name: 'rename' }).click();

			// Worn everywhere `labelOf` is read: the frame's title, the
			// button that opens its menu, and the `▾`'s own name.
			await titledPane(page, 'sleep tail').waitFor();
			assert.deepEqual(await titles(page), ['dayview', 'repl', 'nowview', 'sleep tail', 'custom', 'fields']);
			assert.equal(await pane(page, 'tail').locator('h2').innerText(), 'sleep tail');
			assert.equal(await menuButton(page, 'tail').getAttribute('aria-label'), 'menu of the sleep tail pane');
			assert.equal(await manageButton(page, 'tail').getAttribute('aria-label'), 'manage the sleep tail pane');
			// It EDITS the tree, so it dirties the layout like any other edit.
			assert.equal(await dots(), 1, 'a rename did not dirty the layout');
			assert.equal((await tabOf()).title, 'sleep tail');

			// The draft is the device's, so the name comes back with it.
			await app.reload();
			await titledPane(page, 'sleep tail').waitFor();

			// Emptied, the title goes and the KEY goes with it (`setTitle`,
			// the pattern `setLocked` uses): a pane never named and one
			// named and cleared are the same pane.
			await manageButton(page, 'tail').click();
			await page.getByRole('menuitem', { name: 'Rename pane', exact: true }).click();
			await page.locator('dialog.ask input').fill('');
			await page.locator('dialog.ask').getByRole('button', { name: 'rename' }).click();
			await titledPane(page, 'tail').waitFor();
			assert.equal(Object.hasOwn(await tabOf(), 'title'), false, 'the emptied title left its key behind');

			// A locked layout cannot be renamed: the `▾` holding the act is
			// not drawn, which is the lock this bead asked for, for free.
			await toggleRootLock(page);
			await until(() => manageButton(page, 'tail').count(), (n) => n === 0);
		});
	});

	// da-cesj: the strip of a tab stack is a row of its own, and the panes
	// a Split puts in its front tab hang their titles half a row above
	// their outlines — into room under the strip, not under it. The
	// phone's `mobile` layout used to ship such a stack (`[fields,
	// nowview]`); row 3 is a split of the two now (da-hkjp), so the stack
	// is BUILT here — a nowview added as a second tab of the fields
	// widget, which is the shape that layout had.
	for (const [word, label] of [
		['right', 'split: a new pane to the right'],
		['below', 'split: a new pane below']
	]) {
		test(`Split ${word} in a tab stack: the strip lies over no inner title, and its + over no inner control`, async () => {
			await withApp(
				async ({ page }) => {
					// A fresh phone's `mobile` opens locked (da-gshh).
					await toggleRootLock(page);
					// `mobile` stopped shipping a tab stack in da-hkjp — its
					// row 3 is a split of fields and a nowview now — so the
					// stack this is about is BUILT: a nowview added as a
					// second tab of the fields widget, the shape that layout
					// had. `fields` names the holder of the split; `bar` is
					// the widget pane inside it.
					await addTab(page, 'bar', 'nowview');
					await manage(page, 'bar', label);
					await titledPane(page, 'empty').waitFor();
					assert.deepEqual(await overStrip(page, 'bar'), { titles: 2, overlaps: [] });
				},
				{ viewport: { width: 390, height: 844 }, device: { isMobile: true, hasTouch: true }, home: null }
			);
		});
	}

	test('the + adds a tab holding what the chooser came back with; dismissed, it adds nothing', async () => {
		await shared.solo(async ({ page }) => {
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
		});
	});

	test('the tab in front opens the menu; an inactive tab just switches', async () => {
		await shared.solo(async ({ page }) => {
			await addTab(page, 'tail', 'aliases');
			const tabs = pane(page, 'tail').locator('[role="tab"]');
			// `aliases` declares neither defaults nor options and holds no
			// layout, so its slice is empty and the tab in front carries no
			// menu at all — Change Widget having moved to the `▾` (da-z8klv).
			assert.equal(await tabs.nth(1).getAttribute('aria-haspopup'), null);
			await tabs.nth(0).click();
			assert.equal(await page.locator('[role="menu"]').count(), 0, 'switching opened the menu');
			assert.equal(await activeTab(page, 'tail'), 'tail');
			// The tab in front carries the menu, and its name is still its words.
			assert.equal(await tabs.nth(0).getAttribute('aria-haspopup'), 'menu');
			assert.equal(await tabs.nth(0).getAttribute('aria-label'), null);
			// Now the tail is the tab in front: the same click opens the menu,
			// and it is the TAIL's — the tab in front is what the name slice
			// is about, so the tail's declared `type` (da-hkjp) puts its two
			// widget items in it while `aliases`, which declares nothing,
			// would not.
			await tabs.nth(0).click();
			await page.locator('[role="menu"]').waitFor();
			assert.deepEqual(await menuLines(page), TAIL_MENU);
			await page.keyboard.press('Escape');
			await page.locator('[role="menu"]').waitFor({ state: 'detached' });
			// With tabs it is the TAB that closes, and Break out gives the
			// tab in front a pane of its own — both under the `▾` (da-bzhq).
			await manageButton(page, 'tail').click();
			assert.deepEqual(await menuLines(page), ['Change Widget', 'Rename tab', 'Split ← ↓ ↑ →', 'Break out ← ↓ ↑ →', 'Bar', '× Close tab']);
			await page.getByRole('menuitem', { name: 'break out: this widget below', exact: true }).click();
			await page.locator('[role="menu"]').waitFor({ state: 'detached' });
			// The tab that left is a pane of its own, under the pane it left;
			// the other stayed behind, which is that pane's whole strip now.
			assert.equal(await pane(page, 'tail').locator('[role="tab"]').count(), 0);
			assert.deepEqual(await titles(page), ['dayview', 'repl', 'nowview', 'aliases', 'tail', 'custom', 'fields']);
			const [stayed, gone] = await Promise.all([
				boxOf(page.locator('section.pane[data-title="aliases"]')),
				boxOf(page.locator('section.pane[data-title="tail"]'))
			]);
			assert.ok(gone.y > stayed.y, 'the tab that broke out is not below the pane it left');
		});
	});

	// The last pane cannot be closed — `tree.test.mjs` holds `closeTab` to
	// it — and the root frame, which used to grey the item, draws no bar
	// of its own now (da-2a5y): its tabs are the shell's top bar.
	test('the root frame draws no bar: a lone root widget has no title, no menu and no inset', async () => {
		await shared.solo(async ({ page }) => {
			assert.deepEqual(await titles(page), ['repl']);
			const root = page.locator('section.root');
			assert.equal(await root.locator('.titlebar').count(), 0);
			assert.equal(await root.locator('button.title, button.add').count(), 0);
			assert.equal(await root.locator('h2').count(), 0);
			assert.equal(await root.locator('> .content').evaluate((el) => getComputedStyle(el).paddingTop), '0px');
		}, ONE_PANE);
	});

	test('Close tab is disabled on the last root tab', async () => {
		await shared.solo(async ({ page }) => {
			// The root's tabs are the root bar's (da-0a5o), and the last
			// of them cannot be closed from the `▾` that holds Close.
			await rootManageButton(page).click();
			// Named by the label alone: the × is decoration, `aria-hidden`.
			const close = page.getByRole('menuitem', { name: 'Close tab', exact: true });
			assert.equal(await close.isDisabled(), true);
			// Disabled, it keeps both its glyph and its place (da-ygc9).
			assert.equal((await menuLines(page)).at(-1), '× Close tab');
			await page.keyboard.press('Escape');
		}, ONE_PANE);
	});

	test('closing panes unnests their split', async () => {
		await shared.solo(async ({ page }) => {
			await manage(page, 'custom', 'Close pane');
			assert.deepEqual(await titles(page), ['dayview', 'repl', 'nowview', 'tail', 'fields']);
			await manage(page, 'nowview', 'Close pane');
			assert.deepEqual(await titles(page), ['dayview', 'repl', 'tail', 'fields']);
			// The side column is gone: the pane that held it holds the tail
			// itself now, as the row's second child.
			assert.equal(await page.locator('section.pane[data-pane="side"]').getAttribute('data-title'), 'tail');
			assert.equal(await page.locator('section.holder[data-pane="side"]').count(), 0);
		});
	});
});

// The root bar is the pane title bar formatted as a top bar (da-wjob).
// vividn: it "is actually just a specially formatted version of the same
// thing — make sure it is really completely aligned with the other widget
// picker, just formatted differently". So the test that matters is not a
// second list of the root's items written out by hand: it is the pane's
// own list, with the root's differences NAMED — anything else that
// diverges fails here.
describe("the root bar's menu", () => {
	/** The root's differences from a pane's, and only these. */
	const NO_BREAK = 'Break out ← ↓ ↑ →'; // a tab leaves its STRIP, and the root has none (da-a642)
	const NO_BAR = 'Bar'; //                the root draws no bar of its own — this one is the shell's
	const ROOT_ONLY = []; //                 `export backup…` was, until it became the gear's «Backups» (da-9ow8m)
	const NO_LOCK = 'Lock layout'; //       a button beside the gear at the root (da-bzhq)
	const CLOSE = '× Close tab'; //        last in the `▾` of both (da-ygc9)

	/** Open a menu, read its lines, close it. */
	async function linesOf(page, open) {
		await open();
		const lines = await menuLines(page);
		await page.keyboard.press('Escape');
		await page.locator('[role="menu"]').waitFor({ state: 'detached' });
		return lines;
	}

	test('the active layout name is the button, and Save layout is the first item', async () => {
		await shared.solo(async ({ page }) => {
			const name = rootMenuButton(page);
			assert.equal(await name.innerText(), 'main');
			assert.equal(await name.getAttribute('aria-haspopup'), 'menu');
			assert.equal(await name.getAttribute('title'), 'layout menu');
			assert.equal(await page.locator('.root-bar button.more').count(), 0, 'the … badge is still there');
			await name.click();
			assert.equal(await page.locator('[role="menu"]').getAttribute('aria-label'), 'menu of the main layout');
			// Change Widget is the `▾`'s first item now (da-z8klv), so the
			// name's slice opens on the acts.
			assert.equal((await menuLines(page))[0], 'Save layout');
			await page.keyboard.press('Escape');
		});
	});

	test('an inactive root tab just switches; the one it switches to opens the menu', async () => {
		await shared.solo(async ({ page }) => {
			const tabs = page.locator('.root-bar [role="tab"]');
			await page.locator('.root-bar button[aria-label="open a layout"]').click();
			await chooser(page).locator('button').filter({ hasText: exactly('stats') }).click();
			await chooser(page).waitFor({ state: 'detached' });
			await page.locator('.root-bar [role="tab"]').nth(1).waitFor();
			assert.deepEqual(await tabs.allInnerTexts(), ['main', 'stats']);
			// The one behind carries no menu: a click on it switches.
			assert.equal(await tabs.nth(0).getAttribute('aria-haspopup'), null);
			await tabs.nth(0).click();
			assert.equal(await page.locator('[role="menu"]').count(), 0, 'switching opened the menu');
			await page.locator('.root-bar [role="tab"][aria-selected="true"]', { hasText: 'main' }).waitFor();
			await tabs.nth(0).click();
			await page.locator('[role="menu"]').waitFor();
			await page.keyboard.press('Escape');
		});
	});

	test('the root menus are the pane menus, less Break out, Bar and the lock', async () => {
		await withApp(async ({ page }) => {
			// A pane whose menu has everything a pane menu can have: several
			// tabs (so it says `Close tab`, as the root's always does) and an
			// active tab holding a LAYOUT (so the four acts are on it too,
			// da-crp5). `main` is only choosable once the root tab is `mine`.
			page.once('dialog', (dialog) => dialog.accept('mine'));
			await chooseRoot(page, 'Save layout as');
			await page.locator('.root-bar [role="tab"]', { hasText: 'mine' }).waitFor();
			await addTab(page, 'tail', 'main');
			// The chooser closes before the layout is inlined; the tab wears
			// its name once it is.
			await pane(page, 'tail').locator('[role="tab"]', { hasText: 'main' }).waitFor();

			// The NAME's slice: what the tab shows and what it is saved as.
			const paneLines = await linesOf(page, () => menuButton(page, 'tail').click());
			assert.deepEqual(paneLines, [
				'Save layout',
				'Restore layout',
				// This tab holds `main`, which ships with datum (da-luwe).
				'Reset layout',
				'Save layout as',
				NO_LOCK
			]);
			// And the `▾`'s: what is done to the pane and its tabs (da-bzhq),
			// Close last and wearing its × (da-ygc9).
			const paneManage = await linesOf(page, () => manageButton(page, 'tail').click());
			assert.deepEqual(paneManage, ['Change Widget', 'Split ← ↓ ↑ →', NO_BREAK, NO_BAR, CLOSE]);

			// The same arithmetic on both slices.
			//
			// `Reset layout` is dropped from the comparison rather than
			// missing from one side: it is a fact about the LAYOUT — this
			// pane holds the built-in `main`, the root tab holds the `mine`
			// the reader just saved — and not about which menu is open, so
			// it is the one item the two menus may differ on (da-luwe).
			const rootLines = await linesOf(page, () => rootMenuButton(page).click());
			const shared = paneLines.filter((l) => l !== NO_LOCK && l !== 'Reset layout');
			assert.deepEqual(rootLines, [...shared, ...ROOT_ONLY]);

			const rootManage = await linesOf(page, () => rootManageButton(page).click());
			assert.deepEqual(rootManage, paneManage.filter((l) => l !== NO_BREAK && l !== NO_BAR));
		}, DESKTOP);
	});

	// The tab BECOMES the widget (da-8o07): a root tab is a copy of the
	// layout it names, and a widget chosen for it replaces the copy rather
	// than being put inside it — so the tab reads the widget's own title,
	// not `main`. `layout-nested.test.mjs` holds the rest of that rule.
	test('Change Widget replaces the root tab with what the chooser came back with', async () => {
		await withApp(async (app) => {
			const { page } = app;
			await manageRoot(page, 'Change Widget');
			await chooser(page).waitFor();
			await chooser(page).locator('button').filter({ hasText: exactly('stats') }).click();
			await chooser(page).waitFor({ state: 'detached' });
			await page.locator('section.pane[data-title="stats"]').waitFor();
			assert.deepEqual(await titles(page), ['stats']);
			const names = () =>
				page.locator('.root-bar [role="tab"]').evaluateAll((all) => all.map((b) => b.textContent.replace('•', '').trim()));
			assert.deepEqual(await names(), ['stats']);

			// At the root the widget is its built-in layout (da-y2f6), so the
			// tab has a name — and a named root tab is in this device's open
			// list, which is what brings it back.
			await app.reload();
			await page.locator('section.pane[data-title="stats"]').waitFor();
			assert.deepEqual(await names(), ['stats']);
			assert.deepEqual(await titles(page), ['stats']);
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
		await shared.solo(async ({ page }) => {
			await manageButton(page, 'tail').click();
			const items = page.locator('[role="menu"] button');
			assert.deepEqual(await menuLines(page), ['Change Widget', 'Rename pane', 'Split ← ↓ ↑ →', 'Break out ← ↓ ↑ →', 'Bar', '× Close pane']);
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
		});
	});

	test('right moves the frame\'s bar there and turns the alignment icons; the align stays', async () => {
		await shared.solo(async ({ page }) => {
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
		});
	});

	// The left-hand bar is turned round (it reads up), so its start is at
	// the BOTTOM — and its icon says so, where the title really is.
	//
	// The maximize is the first thing along the bar since da-bzhq, so on an
	// upside-down bar it is BELOW the title; what says the row starts at
	// the bottom is that order, plus the title reaching into the bottom
	// half of the pane.
	test('on a left bar the start icon and the title are both at the bottom', async () => {
		await shared.solo(async ({ page }) => {
			await chooseBar(page, 'tail', 'left');
			const [frame, h2, max] = await Promise.all([
				boxOf(pane(page, 'tail')),
				boxOf(pane(page, 'tail').locator('h2')),
				boxOf(maximizeButton(page, 'tail'))
			]);
			assert.ok(max.y > h2.y, `the maximize (y=${max.y}) is not below the title (y=${h2.y})`);
			assert.ok(h2.y + h2.height > frame.y + frame.height / 2, 'align start on a left bar is not at the bottom');
			await openBar(page, 'tail');
			const start = (await row(page))[5];
			assert.equal(start.title, 'align to the start (bottom)');
			assert.ok(start.bar.y >= 8, `the start mark is at y=${start.bar.y}, not at the bottom`);
		});
	});

	test('by keyboard: in with ArrowRight, across, Enter chooses; back out with ArrowLeft, Escape, Tab', async () => {
		await shared.solo(async ({ page }) => {
			// `Bar` is under the `▾` beside the `+` (da-bzhq).
			const more = manageButton(page, 'tail');
			const focused = () =>
				page.evaluate(() => document.activeElement.getAttribute('aria-label') ?? document.activeElement.textContent.trim());
			/** From the `▾`: open, ArrowDown until `Bar` is focused. */
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
			assert.ok(await more.evaluate((el) => el === document.activeElement), 'choosing did not hand focus to the ▾');

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
			assert.ok(await more.evaluate((el) => el === document.activeElement), 'Escape twice did not reach the ▾');

			await toBar();
			await page.keyboard.press('ArrowRight');
			await page.keyboard.press('Tab');
			assert.equal(await page.locator('[role="menu"]').count(), 0, 'Tab left a menu open');
		});
	});
});

// "bar submenu should open immediately on mouse over" and "let's put the
// two split buttons on the same line" (da-13bc) — four of them now (da-9vtw).
describe('the pane menu by pointer', () => {
	const barItem = (page) => page.getByRole('menuitem', { name: /^Bar/ });

	test('Bar ▸ opens on the pointer with no click, stays open on the way to it, and closes on another item', async () => {
		await shared.solo(async ({ page }) => {
			await manageButton(page, 'tail').click();
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
		});
	});

	// A finger landing on `Bar` enters it too — Chromium fires a touch
	// `pointerenter` at the touch's start — and that must not open the
	// submenu; the tap does (Copilot on #176). A real touch, through CDP.
	test('a touch entering Bar ▸ does not open it; the tap does', async () => {
		await phone.solo(async ({ page }) => {
			await manageButton(page, 'repl').tap();
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
		});
	});

	// Copilot on #176: the row's own arrows, to both of its ends.
	test('ArrowRight and ArrowLeft walk the Split row and stop at its ends', async () => {
		await shared.solo(async ({ page }) => {
			const focused = () =>
				page.evaluate(() => document.activeElement.getAttribute('aria-label') ?? document.activeElement.textContent.trim());
			// The `▾` beside the `+` is where Split is now (da-bzhq). The
			// menu opens on its first item, which is `Change Widget`
			// (da-z8klv), then `Rename pane` (da-s7sdm); the Split row is
			// the next one down.
			await manageButton(page, 'tail').focus();
			await page.keyboard.press('Enter');
			assert.equal(await focused(), 'Change Widget');
			await page.keyboard.press('ArrowDown');
			assert.equal(await focused(), 'Rename pane');
			await page.keyboard.press('ArrowDown');
			assert.equal(await focused(), 'split: a new pane to the left');
			// Vim order, and the walk follows the eye: ← ↓ ↑ →.
			const walk = [];
			for (const key of ['ArrowLeft', 'ArrowRight', 'ArrowRight', 'ArrowRight', 'ArrowRight', 'ArrowLeft']) {
				await page.keyboard.press(key);
				walk.push(await focused());
			}
			assert.deepEqual(walk, [
				'split: a new pane to the left',
				'split: a new pane below',
				'split: a new pane above',
				'split: a new pane to the right',
				'split: a new pane to the right',
				'split: a new pane above'
			]);
			// And ArrowDown from the row's last button goes on to the next
			// item — the four arrows of Break out, then Bar, then Close
			// pane, which is last (da-ygc9).
			await page.keyboard.press('ArrowRight');
			await page.keyboard.press('ArrowRight');
			await page.keyboard.press('ArrowDown');
			assert.equal(await focused(), 'break out: this widget to the left');
			for (let i = 0; i < 4; i++) await page.keyboard.press('ArrowDown');
			assert.equal(await focused(), 'Bar');
			await page.keyboard.press('ArrowDown');
			assert.equal(await focused(), '× Close pane');
		});
	});

	// «split and breakout arrows should be in vim key order left down up
	// right» (vividn, da-bzhq): h j k l, and the DOM order is that order, so
	// the button the eye calls the last is the one ArrowRight ends on.
	test('Split is one line of four arrows in vim order, and ↓ splits below', async () => {
		await shared.solo(async ({ page }) => {
			await manageButton(page, 'tail').click();
			const buttons = page.getByRole('group', { name: 'Split' }).getByRole('menuitem');
			assert.deepEqual(await buttons.evaluateAll((all) => all.map((b) => `${b.textContent.trim()} ${b.getAttribute('aria-label')}`)), [
				'← split: a new pane to the left',
				'↓ split: a new pane below',
				'↑ split: a new pane above',
				'→ split: a new pane to the right'
			]);
			const boxes = await Promise.all([0, 1, 2, 3].map((i) => boxOf(buttons.nth(i))));
			for (const b of boxes) {
				assert.equal(b.y, boxes[0].y, 'the four are not on one line');
				assert.ok(b.width >= 24 && b.height >= 24, `a split button is ${b.width}x${b.height}`);
			}
			assert.ok(boxes[0].x < boxes[1].x && boxes[1].x < boxes[2].x && boxes[2].x < boxes[3].x, 'the row is not drawn in its DOM order');
			await buttons.nth(1).click();
			await page.locator('[role="menu"]').waitFor({ state: 'detached' });
			assert.deepEqual(await titles(page), ['dayview', 'repl', 'nowview', 'tail', 'empty', 'custom', 'fields']);
			const [tail, empty] = await Promise.all([boxOf(pane(page, 'tail')), boxOf(page.locator('section.pane[data-title="empty"]'))]);
			assert.equal(Math.round(empty.x), Math.round(tail.x));
			assert.equal(Math.round(empty.width), Math.round(tail.width));
			assert.ok(empty.y > tail.y + tail.height, 'the new pane is not under the tail');
		});
	});
});
