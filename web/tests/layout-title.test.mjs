// THE WIDGET TITLE IS TWO DOORS AND A `×` (da-efr4i).
//
// vividn, 2026-09-22: «in unlocked mode, add an x to the widget titles that
// let's you close it. Long pressing or right clicking on a widget title
// opens the change widget menu. I think that will feel natural to use.»
//
// So `Change Widget` keeps its home in the `▾` (da-z8klv) and gains a
// SECOND door — a hold on the name of the widget it would replace — and
// the row gains a `×` that closes what the title names. Both are the
// unlocked layout's: a lock already hides the `+` and the `▾`, and these
// two go with them.
//
// What this file holds the change to:
//
//   - the `×` is there unlocked, gone locked, and CLOSES — writing the
//     DRAFT and nothing else, which is the answer to a mis-tap: this
//     device's saved layouts are byte for byte what they were, `Restore
//     layout` is the way back, and the last pane closes to an EMPTY pane
//     and never to nothing, so the way back from that one is the press it
//     is already offering. The database is never touched at all; a widget
//     is a view of it.
//   - a right click on the title opens the chooser and eats the browser's
//     own menu; a right click on the pane's CONTENT does neither, because
//     that is where a reader selects and copies text.
//   - a long press by a FINGER does what the right click does, over real
//     milliseconds — the gesture is a timer ($lib/hold.ts), so a
//     synthesised pair would be a test of the test.
//   - a pane whose widget declares no options has NO title menu at all
//     (`custom`, `command` — da-z8klv), and the hold still works there:
//     the gesture is on the title's box, not on the menu's button.
//   - the press the title already had still opens what the pane IS, and
//     `Change Widget` is still in the `▾`. A door was added, not moved.

import test, { before, after, describe } from 'node:test';
import assert from 'node:assert/strict';
import { sharedApp, startApp, stopApp, withApp } from './support/app.mjs';
import {
	DESKTOP,
	addTab,
	along,
	chooser,
	layoutStorage,
	draft,
	manageButton,
	menuLines,
	pane,
	rootMenuButton,
	settled,
	titles,
	toggleRootLock,
	unlockRoot,
	widgetPane
} from './support/layout.mjs';

before(startApp);
after(stopApp);

/**
 * Two panes, and the second one deliberately has NO title menu: the
 * `command` widget declares neither defaults nor options, so its name
 * opens nothing (da-z8klv). The hold has to reach it anyway.
 */
const PAGE = JSON.stringify(along('row', widgetPane('repl', 'repl'), widgetPane('cmd', 'command')));

// ONE APP for the file (da-6b3dj, sharedApp in support/app.mjs), each test
// on `main` opened from the PAGE draft (`solo(fn, PAGE)`), locks put back
// between them. The finger's test alone opens its own: a touch context.
const shared = sharedApp(DESKTOP);
const onPage = (fn) => shared.solo(fn, PAGE);

const closeButton = (page, id) => pane(page, id).locator('> .titlebar button.close');

/** The page menu's items, opened and closed again. */
async function rootMenuLines(page) {
	await rootMenuButton(page).click();
	const lines = await menuLines(page);
	await page.keyboard.press('Escape');
	await page.locator('[role="menu"]').waitFor({ state: 'detached' });
	return lines;
}
/** A pane's title, as a finger finds it: the box the h2 and its menu button share. */
const titleBox = (page, id) => pane(page, id).locator('> .titlebar .titlebox');
const content = (page, id) => pane(page, id).locator('> .content');

/**
 * Every `contextmenu` the document sees, and whether the page had already
 * eaten it by the time it got there.
 *
 * A listener on `document` in the BUBBLE phase is the last thing on the
 * path, so `defaultPrevented` there is the whole page's answer — which is
 * the claim: eaten on a title, untouched on a pane's content, where the
 * browser's own menu is how text is copied.
 */
const watchMenus = (page) =>
	page.evaluate(() => {
		window.__menus = [];
		// Once a page: a shared app's next test watches again, and a second
		// listener would record every menu twice.
		if (window.__menusWatched) return;
		window.__menusWatched = true;
		document.addEventListener('contextmenu', (e) => window.__menus.push(e.defaultPrevented));
	});
const menusSeen = (page) => page.evaluate(() => window.__menus);

/**
 * A long press with a FINGER, over real milliseconds.
 *
 * `fieldpill.test.mjs`'s instrument and its reason: Playwright's
 * touchscreen only taps, and `page.mouse` on a touch-capable context is
 * still a mouse, so a coarse arm driven by the mouse would be the fine arm
 * under another name. 450 ms is `HOLD_MS`; 700 clears it without racing it.
 */
async function longPress(page, locator) {
	const box = await locator.boundingBox();
	const [x, y] = [box.x + box.width / 2, box.y + box.height / 2];
	const cdp = await page.context().newCDPSession(page);
	try {
		await cdp.send('Input.dispatchTouchEvent', { type: 'touchStart', touchPoints: [{ x, y }] });
		await page.waitForTimeout(700);
		await cdp.send('Input.dispatchTouchEvent', { type: 'touchEnd', touchPoints: [] });
	} finally {
		await cdp.detach();
	}
}

describe('the × on the widget title', () => {
	test('it closes what the title names, and writes only the draft doing it', async () => {
		await onPage(async ({ page }) => {
			await unlockRoot(page);
			assert.deepEqual(await titles(page), ['repl', 'command']);
			const before = await layoutStorage(page);

			// The act is named after the thing, as the menu's Close is.
			assert.equal(await closeButton(page, 'repl').getAttribute('aria-label'), 'close the repl pane');
			await closeButton(page, 'repl').click();
			await settled(page);
			assert.deepEqual(await titles(page), ['command'], 'the × did not close the pane');

			// A MIS-TAP IS NOT A LOSS. The × writes the DRAFT and only the
			// draft, as every other layout act does: whatever this device has
			// SAVED is byte for byte what it was, so `Restore layout` — which
			// the menu still offers — is the way back, and the database was
			// never touched at all, a widget being a view of it. (Nor is the
			// copy of the tree the next load draws first a save, da-bhudz.)
			const after = await layoutStorage(page);
			const saves = (all) => Object.fromEntries(Object.entries(all).filter(([k]) => !/\.(draft|shell)\./.test(k)));
			assert.deepEqual(saves(after), saves(before), 'the × wrote something other than the draft');
			assert.notDeepEqual(after, before, 'the × did not write the draft either');
			assert.ok((await rootMenuLines(page)).includes('Restore layout'));
		});
	});

	test('a lock takes it away, with the `+` and the `▾`', async () => {
		await onPage(async ({ page }) => {
			await unlockRoot(page);
			assert.equal(await page.locator('section.pane button.close').count(), 2);

			await toggleRootLock(page);
			await settled(page);
			assert.equal(await page.locator('section.pane button.close').count(), 0, 'a locked layout kept its `×`');
			assert.equal(await page.locator('section.pane button.add').count(), 0);

			await toggleRootLock(page);
			await settled(page);
			assert.equal(await page.locator('section.pane button.close').count(), 2, 'unlocking did not give it back');
		});
	});

	test('the last pane of a layout closes to an EMPTY pane, never to nothing', async () => {
		await onPage(async ({ page }) => {
			await unlockRoot(page);
			await closeButton(page, 'cmd').click();
			await settled(page);
			assert.deepEqual(await titles(page), ['repl']);

			// The × is the menu's `Close pane` and nothing else, so it ends
			// where that ends: a layout is never left with no pane in it, and
			// the pane it leaves is the question `Change Widget` answers.
			// Which is the whole of what a mis-tap costs here — one press.
			await closeButton(page, 'repl').click();
			await settled(page);
			assert.deepEqual(await titles(page), ['empty'], 'the × left the layout with nothing at all');
			await page.locator('.choose button').click();
			await chooser(page).waitFor();
			assert.match(await chooser(page).innerText(), /Change Widget/);
		});
	});
});

describe('the hold on the widget title', () => {
	test('a right click opens Change Widget and eats the browser’s own menu', async () => {
		await onPage(async ({ page }) => {
			await unlockRoot(page);
			await watchMenus(page);

			await titleBox(page, 'repl').click({ button: 'right' });
			await chooser(page).waitFor();
			// The chooser is what `Change Widget` opens — the same dialog the
			// `▾`'s item opens, wearing the same words.
			assert.match(await chooser(page).innerText(), /Change Widget/);
			// And the title's OWN menu did not open behind it.
			assert.equal(await page.locator('[role="menu"]').count(), 0, 'the title’s menu opened too');
			await page.keyboard.press('Escape');
			await chooser(page).waitFor({ state: 'detached' });
			assert.deepEqual(await menusSeen(page), [true], 'the browser’s menu was left to open on the title');

			// And the CONTENT is left alone: that is where a reader selects
			// and copies what a pane shows.
			await content(page, 'repl').click({ button: 'right', position: { x: 20, y: 20 } });
			await page.waitForTimeout(200);
			assert.equal(await chooser(page).count(), 0, 'a right click in the content opened the chooser');
			assert.deepEqual(
				await menusSeen(page),
				[true, false],
				'the browser’s menu was eaten in the pane’s content'
			);
		});
	});

	test('a pane with no title menu at all is still held', async () => {
		await onPage(async ({ page }) => {
			await unlockRoot(page);
			// The premise, and it is the point of the pane: `command`
			// declares neither defaults nor options, so its name opens
			// nothing (da-z8klv) and there is no button to hang this on.
			assert.equal(await pane(page, 'cmd').locator('> .titlebar button.title').count(), 0);

			await titleBox(page, 'cmd').click({ button: 'right' });
			await chooser(page).waitFor();
			assert.match(await chooser(page).innerText(), /Change Widget/);
		});
	});

	test('a long press by a finger does what the right click does', async () => {
		await withApp(
			async ({ page }) => {
				await unlockRoot(page);
				await longPress(page, titleBox(page, 'repl'));
				await chooser(page).waitFor();
				assert.match(await chooser(page).innerText(), /Change Widget/);
				// The press that held is not also the press that opens the
				// title's own menu ($lib/hold.ts swallows the click behind it).
				assert.equal(await page.locator('[role="menu"]').count(), 0, 'the hold also opened the title’s menu');
			},
			{
				viewport: { width: 1280, height: 900 },
				device: { hasTouch: true, isMobile: false },
				prepare: (context) => context.addInitScript(draft, PAGE)
			}
		);
	});

	test('a lock closes this door too', async () => {
		await onPage(async ({ page }) => {
			await unlockRoot(page);
			await toggleRootLock(page);
			await settled(page);
			await watchMenus(page);

			await titleBox(page, 'repl').click({ button: 'right' });
			await page.waitForTimeout(200);
			assert.equal(await chooser(page).count(), 0, 'a locked title opened the chooser');
			assert.deepEqual(await menusSeen(page), [false], 'a locked title still ate the browser’s menu');
		});
	});

	test('with a strip, it is the tab IN FRONT that is held, and the `×` closes that tab', async () => {
		await onPage(async ({ page }) => {
			await unlockRoot(page);
			await addTab(page, 'repl', 'aliases');
			await settled(page);
			const tabs = pane(page, 'repl').locator('> .titlebar [role="tab"]');
			assert.deepEqual(await tabs.allInnerTexts(), ['repl', 'aliases']);
			await watchMenus(page);

			// A tab BEHIND is a way to switch panes and nothing else: the
			// browser's own menu is left to it, as it is over the content.
			await tabs.first().click({ button: 'right' });
			await page.waitForTimeout(200);
			assert.equal(await chooser(page).count(), 0, 'a tab behind opened the chooser');
			assert.deepEqual(await menusSeen(page), [false]);

			// The tab in FRONT names the widget in front, so it is the title.
			await tabs.last().click({ button: 'right' });
			await chooser(page).waitFor();
			assert.match(await chooser(page).innerText(), /Change Widget/);
			assert.deepEqual(await menusSeen(page), [false, true]);
			await page.keyboard.press('Escape');
			await chooser(page).waitFor({ state: 'detached' });

			// And the × closes THE TAB, the pane staying: it is the menu's
			// `Close tab`, which is what the row's last badge always is.
			assert.equal(await closeButton(page, 'repl').getAttribute('aria-label'), 'close the aliases tab');
			await closeButton(page, 'repl').click();
			await settled(page);
			assert.deepEqual(await tabs.allInnerTexts(), [], 'the strip outlived its second tab');
			assert.deepEqual(await titles(page), ['repl', 'command'], 'the × closed the pane, not the tab');
		});
	});

	test('the door it was given is a SECOND one: the press and the `▾` are unchanged', async () => {
		await onPage(async ({ page }) => {
			await unlockRoot(page);

			// A press on the title still opens what the pane IS.
			await titleBox(page, 'repl').click();
			assert.deepEqual(await menuLines(page), ['Reset widget', 'newest first']);
			await page.keyboard.press('Escape');
			await page.locator('[role="menu"]').waitFor({ state: 'detached' });

			// And `Change Widget` is still the first item of the `▾`.
			await manageButton(page, 'repl').click();
			assert.equal((await menuLines(page))[0], 'Change Widget');
			await page.keyboard.press('Escape');
			await page.locator('[role="menu"]').waitFor({ state: 'detached' });
		});
	});
});
