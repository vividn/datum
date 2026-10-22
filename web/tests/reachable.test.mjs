// Every control the day frame offers has to be on the screen it is drawn on.
//
// On a phone the frame's rows add up to more than the viewport, so the column
// scrolls — and the widget bar, the last row, started below the bottom edge.
// The document itself does not scroll, so the reader's first gesture (flick
// the page) does nothing, and the way to log without typing was gone: the
// field chips and the `+` were in the DOM at y≈707 with the screen ending at
// 664 (da-hkpd).
//
// The trap this file exists to avoid: every other test in this suite runs at
// Playwright's 1280x720, where the bar fits and everything passes. And a test
// that asserts the bar EXISTS passes even while it is off-screen — it was in
// the DOM the whole time. So these run at a phone's viewport and assert
// REACHABILITY: the control's box lies inside the screen, with no scrolling
// first.
//
// The second trap, which the first version of this file walked into: it
// varied the axis the bug was found on (viewport height) and left the other
// one — how much is IN the bar — at a single field. Five ordinary fields wrap
// the bar to two rows on a phone, and the second row hung below the screen
// again (da-vyo2). So the cases here run against a POPULATED bar as well as
// an empty one.
//
// The bar is the fields widget now (da-c7u8): a button per TRACKED field — so the handful here is declared with `setup F`,
// which is what gives a field its button.
//
// And the count that wraps is not a constant: it is what fits across the
// screen, so it falls as the screen narrows — five fields at 390, four at
// 320, three at 280. Fixing a width and hunting for the count would just
// re-learn the width. These log one ordinary handful at every screen in the
// matrix, which wraps at all of them.

import test, { before, after, describe } from 'node:test';
import assert from 'node:assert/strict';
import { atEachScreen, focusTop, seedLines, sharedApp, startApp, stopApp, withApp } from './support/app.mjs';
import { fold, unfold } from './support/bar.mjs';
import { chooser, enterLayoutMode, inRoom } from './support/layout.mjs';
import { until } from './support/until.mjs';
import { FIELD_FORM, openField } from './support/field.mjs';

before(startApp);
after(stopApp);

/** A phone in portrait, once the browser's own chrome is subtracted. */
const PHONE = { width: 390, height: 640 };

/** The shortest screen worth claiming to support — an SE with chrome. */
const SHORT = { width: 390, height: 560 };

/** An SE's width, which is still common, and the narrowest phone there is. */
const NARROW = { width: 320, height: 664 };
const TINY = { width: 280, height: 664 };

// ONE APP per screen for most of this file (da-6b3dj, sharedApp in
// support/app.mjs): a phone, a 1280x900 desktop and Playwright's 1280x720,
// each test on `main` alone with an empty database. A test that leaves a
// widget's own state behind — a field pressed into its states, the custom
// pane's command — takes a room, whose widgets go with it, and one that
// opens the field widget shuts it again. What still opens an app of its own
// is alone at its options: the chooser's 320px, a coarse pointer, and a
// locked `main` from `prepare`.
const phone = sharedApp({ viewport: PHONE });
const desktop = sharedApp({ viewport: { width: 1280, height: 900 } });
const plain = sharedApp();

/** Both axes of the bug: how short the screen is and how narrow it is. */
const SCREENS = [PHONE, SHORT, NARROW, TINY];

/**
 * Enough ordinary fields to wrap the bar at every screen in SCREENS.
 *
 * Not an adversarial input — this is a person tracking five things
 * (da-vyo2).
 */
const FIELDS = ['coffee', 'mood', 'sleep', 'water', 'run'];

/** Track `fields` and wait until every one of them has a button. */
async function track(app, fields) {
	await seedLines(app, fields.map((field) => `setup ${field} --verb occur`));
	await app.page.locator('.bar button.widget').nth(fields.length - 1).waitFor();
}

/**
 * Where `selector` is, against the screen it is drawn on.
 *
 * `getBoundingClientRect` is viewport-relative, so this is the question the
 * reader is asking with their eyes: is the thing on the screen right now?
 */
function boxOf(page, selector) {
	return page.evaluate((s) => {
		const el = document.querySelector(s);
		if (!el) return null;
		const { top, bottom } = el.getBoundingClientRect();
		return { top, bottom, screen: document.documentElement.clientHeight };
	}, selector);
}

async function assertOnScreen(page, selector, what) {
	const box = await boxOf(page, selector);
	assert.ok(box, `${what} is not on the page at all`);
	assert.ok(
		box.top >= 0 && box.bottom <= box.screen,
		`${what} is off-screen: ${box.top.toFixed(0)}–${box.bottom.toFixed(0)} on a ${box.screen}px screen`
	);
}

describe('on a phone-sized screen', () => {
	test("a tracked field's button is on screen", async (t) => {
		await phone.solo(async (app) => {
			const { page, submit } = app;
			await submit('setup coffee --verb occur');
			await page.locator('.bar button.widget', { hasText: 'coffee' }).waitFor();
			await atEachScreen(t, page, [PHONE, SHORT], async () => {
				await assertOnScreen(page, '.bar button.widget', "the coffee field's chip");
			});
		});
	});

	// The two edges of a wrapped bar, and the one that da-hkpd's fix and its
	// verification both missed: the bar is pinned to the bottom, and wrapping
	// must push it neither DOWNWARD past that edge nor upward off the top.
	test(`a bar of ${FIELDS.length} fields keeps both its edges on screen`, async (t) => {
		await phone.solo(async (app) => {
			await track(app, FIELDS);
			await atEachScreen(t, app.page, SCREENS, async () => {
				await assertOnScreen(app.page, '.bar button.widget', "the first field's chip");
			});
		});
	});

	// The gear (da-pp1a) is the root bar's right end: the global settings
	// menu, holding «Backups» (da-9ow8m), «Sync» (da-s9r58), «Reset default layouts»
	// (da-jbcv) and the build stamp. The bar is one row that never wraps, so the thing that can
	// push the gear off is a crowded strip of tabs on a narrow screen —
	// and its menu, which opens from the right edge, has to land on the
	// screen as well.
	test('the gear and its menu are on screen', async (t) => {
		await phone.solo(async ({ page }) => {
			await atEachScreen(t, page, [...SCREENS, { width: 320, height: 480 }], async () => {
				await assertOnScreen(page, '.root-bar button.gear', 'the gear');
				const box = await page.locator('.root-bar button.gear').boundingBox();
				const width = await page.evaluate(() => document.documentElement.clientWidth);
				assert.ok(box.width >= 24 && box.height >= 24, `the gear is ${box.width}x${box.height}`);
				assert.ok(box.x >= 0 && box.x + box.width <= width + 1, `the gear is ${box.x}–${box.x + box.width} on ${width}px`);

				await page.locator('.root-bar button.gear').click();
				const items = page.locator('[role="menu"][aria-label="settings"] button');
				// The three acts (da-9ow8m, da-s9r58, da-jbcv) and the build
				// stamp, which is a fact and so disabled.
				assert.equal(await items.count(), 4);
				assert.equal(await items.evaluateAll((all) => all.filter((b) => !b.disabled).length), 3);
				const menu = await page.locator('[role="menu"][aria-label="settings"]').boundingBox();
				const screen = await page.evaluate(() => ({
					width: document.documentElement.clientWidth,
					height: document.documentElement.clientHeight
				}));
				assert.ok(menu.x >= 0 && menu.x + menu.width <= screen.width + 1, `the settings menu is ${menu.x}–${menu.x + menu.width}`);
				assert.ok(menu.y >= 0 && menu.y + menu.height <= screen.height + 1, 'the settings menu is taller than the screen');
				// Closed again: the next screen opens it for itself.
				await page.keyboard.press('Escape');
				await page.locator('[role="menu"][aria-label="settings"]').waitFor({ state: 'detached' });
			});
		});
	});

	// The day strip's own two controls (da-iu9i): the `+` on the divider,
	// which drags the day height and the date column, and the freeze-today
	// toggle at the far left of the same line. Both are drawn INSIDE the
	// strip, so a phone that leaves the strip a few dozen pixels is the
	// case that puts them off the screen — and both are fingertips.
	// The freeze-today toggle left the pane for the widget's options (da-2hd21).
	test("the strip's + handle is a fingertip on screen", async (t) => {
		await phone.solo(async ({ page }) => {
			await atEachScreen(t, page, [PHONE, SHORT], async () => {
				for (const [selector, what] of [['.strip button.day-grip', "the strip's + handle"]]) {
					await assertOnScreen(page, selector, what);
					const box = await page.locator(selector).boundingBox();
					const width = await page.evaluate(() => document.documentElement.clientWidth);
					assert.ok(box.width >= 24 && box.height >= 24, `${what} is ${box.width}x${box.height}`);
					assert.ok(
						box.x >= 0 && box.x + box.width <= width + 1,
						`${what} is ${box.x}–${box.x + box.width} on a ${width}px screen`
					);
				}
			});
		});
	});

	// The one chooser (da-uyx1) is a modal over the whole page, so the
	// screen it has to fit is the narrowest one: 320px, with every item a
	// fingertip's 24px and the dialog itself inside the viewport.

	test('the chooser fits, with 24px items', async (t) => {
		await withApp(async ({ page }) => {
			await page.locator('.root-bar button[aria-label="open a layout"]').click();
			await chooser(page).waitFor();
			await atEachScreen(t, page, [NARROW, TINY], async () => {
				const box = await chooser(page).boundingBox();
				const screen = await page.evaluate(() => ({
					width: document.documentElement.clientWidth,
					height: document.documentElement.clientHeight
				}));
				assert.ok(
					box.x >= 0 && box.x + box.width <= screen.width + 1,
					`the chooser is ${box.x}–${box.x + box.width} on a ${screen.width}px screen`
				);
				assert.ok(box.y >= 0 && box.y + box.height <= screen.height + 1, 'the chooser is taller than the screen');
				const items = chooser(page).locator('button');
				const count = await items.count();
				assert.ok(count > 1, `the chooser offers ${count} items`);
				for (let i = 0; i < count; i++) {
					const item = await items.nth(i).boundingBox();
					const name = await items.nth(i).innerText();
					assert.ok(item.height >= 24, `${name} is ${item.height}px tall`);
					assert.ok(
						item.x >= 0 && item.x + item.width <= screen.width + 1,
						`${name} is ${item.x}–${item.x + item.width} on a ${screen.width}px screen`
					);
				}
			});
		}, { viewport: NARROW });
	});

	// The output folds away and a DRAG is what folds it (da-ckw3). There was
	// a `^` button on the bar's top line until vividn, 2026-09-18: "The ^
	// button obscures the text. Let's just get rid of this button and instead
	// just put a little carrat above the input bar to suggest that it can be
	// pulled up." So there is no control to be reachable any more, and what
	// this holds instead is the pair that replaced it: the HINT is on the
	// screen at every phone width (a mark off the edge suggests nothing), and
	// the output the drag opens is on the screen too.
	test('the arrow is on screen, and the output it opens is too', async (t) => {
		await phone.solo(async ({ page }) => {
			await atEachScreen(t, page, SCREENS, async () => {
				await assertOnScreen(page, '.prompt-bar .caret', 'the arrow');
				const box = await page.locator('.prompt-bar .caret').boundingBox();
				const width = await page.evaluate(() => document.documentElement.clientWidth);
				assert.ok(
					box.x >= 0 && box.x + box.width <= width + 1,
					`the arrow is ${box.x}–${box.x + box.width} on a ${width}px screen`
				);

				// The bar GLIDES (da-ckw3, $lib/log.ts): the section is in the
				// DOM on the first frame and grows for ~220ms after it, so what
				// has to be on the screen is where it comes to REST — which is
				// what `unfold` waits for.
				await unfold(page);
				await assertOnScreen(page, '.prompt-bar .section.output', 'the output it shows');
				// Folded again, so the next screen starts where this one did.
				await fold(page);
			});
		});
	});

	// And the history itself: a phone reaches it by pulling the bar up
	// (da-ckw3), and what it must find there is the bar at the top and
	// `close history` on the screen — a control at the END of a history
	// thousands of entries long would be no control at all.
	test('the log a pull on the bar opens, and its way back, are on screen', async (t) => {
		await phone.solo(async (app) => {
			const { page } = app;
			await app.submit('occur coffee');
			await atEachScreen(t, page, SCREENS, async () => {
				await page.evaluate(() => {
					const el = document.querySelector('.prompt-bar');
					const r = el.getBoundingClientRect();
					const x = r.left + r.width / 2;
					const y = r.top + r.height / 2;
					const send = (type, at) =>
						el.dispatchEvent(
							new PointerEvent(type, {
								pointerId: 5,
								pointerType: 'touch',
								isPrimary: true,
								clientX: x,
								clientY: at,
								bubbles: true,
								cancelable: true
							})
						);
					send('pointerdown', y);
					for (let d = 20; d <= 200; d += 20) send('pointermove', y - d);
					send('pointerup', y - 200);
				});
				// The log is OPEN when the bar has been pulled past the notch.
				// The page itself never scrolls (da-ckw3, $lib/log.ts sizes three
				// boxes off one number), so `--log-height` is the only truth
				// there is — a `scrollTop` check here would pass on a page that
				// never moved at all.
				await page.waitForFunction(
					() =>
						parseFloat(
							getComputedStyle(document.querySelector('.shell')).getPropertyValue('--log-height')
						) > 0
				);
				await assertOnScreen(page, '.prompt-bar .section.input', 'the input section in the log');
				await assertOnScreen(page, '.log .top', '`close history`');
				const back = await page.locator('.log .top').boundingBox();
				assert.ok(back.height >= 24, '`close history` is ' + back.height + 'px tall');

				// Back to the dashboard, so the next screen pulls from rest.
				await page.locator('.log .top').click();
				await page.waitForFunction(
					() =>
						parseFloat(
							getComputedStyle(document.querySelector('.shell')).getPropertyValue('--log-height')
						) < 0.5
				);
			});
			assert.deepEqual(app.errors, []);
			// `close history` leaves the output open at the notch; folded
			// again, as a fresh phone has it, for the shared app's next test.
			await fold(page);
		});
	});

	// A new field opens the field widget's place (da-94q1, da-fu29p) — a tab
	// on a phone — so what its form has to clear is the screen's own box. Its
	// pictograms are measured in tests/track.test.mjs.
	test("a new field's form is on screen", async () => {
		await phone.solo(async ({ page }) => {
			await openField(page);
			await page.locator(FIELD_FORM).waitFor();
			await assertOnScreen(page, FIELD_FORM, 'the field form');
			// Left again, as a place is on a phone: back.
			await page.goBack();
			await page.locator(FIELD_FORM).waitFor({ state: 'detached' });
		});
	});

	// The fields widget has NO floating surface left: the hold panel was the
	// only one and da-e78x6 removed it, taking this file's "a field's state
	// bubbles are on screen" with it. A PLAIN press fills the bar with the
	// field's states IN PLACE (da-g635), inside the same scroller under the
	// same height rules, so the list itself is what has to be on screen —
	// which is the test below, and now the only one there is to write.
	test("the states a press fills the bar with are on screen", async () => {
		await phone.room('pressed', async (app) => {
			const { page, submit } = app;
			// mood is the switch here, so the other four are what wrap the bar.
			await submit('setup mood --verb switch --states happy,sad');
			await track(app, FIELDS.filter((f) => f !== 'mood'));
			const chip = page.locator('.bar button.widget', { hasText: 'mood' });
			await chip.waitFor();
			await chip.click();
			await assertOnScreen(page, '.bar [data-back]', 'the way back to the fields');
			await assertOnScreen(page, '.bar [data-state="happy"]', "mood's happy");
			await assertOnScreen(page, '.bar [data-add-state]', 'the + that adds a state');
		});
	});

	// The stats widget (da-n7r0) as a root tab (da-0a5o): reached by its
	// URL, its tab on the root bar, and what it shows on the screen it
	// opens on — the range picker and the argv it is running. There is no
	// field picker any more (da-ptuwr): `stats --month` is every field (da-80do8).
	test('/stats and its range picker are on screen', async (t) => {
		await phone.solo(async (app) => {
			const { page } = app;
			await track(app, FIELDS);
			await app.goto('/stats');
			await page.locator('.root-bar [role="tab"][aria-selected="true"]', { hasText: 'stats' }).waitFor();
			await page.waitForFunction(() => document.querySelector('.stats .argv')?.textContent === 'stats --month');
			await atEachScreen(t, page, SCREENS, async () => {
				await assertOnScreen(page, '.root-bar [role="tab"][aria-selected="true"]', 'the stats tab');
				await assertOnScreen(page, '.stats .argv', 'the argv it is running');
				await assertOnScreen(page, '.stats .range button', 'the range picker');
				// And the bottom bar's prompt, whatever tab is in front.
				await assertOnScreen(page, '.prompt-bar input', "the bottom bar's prompt");
			});
		});
	});

	test("a new field's form is on screen with a wrapped bar", async () => {
		await phone.solo(async (app) => {
			const { page } = app;
			await track(app, FIELDS);
			await openField(page);
			await page.locator(FIELD_FORM).waitFor();
			await assertOnScreen(page, FIELD_FORM, 'the field form');
			// Left again, as a place is on a phone: back.
			await page.goBack();
			await page.locator(FIELD_FORM).waitFor({ state: 'detached' });
		});
	});
});

// The custom pane's controls (da-wb2n) are buttons in the tab order, not
// click targets: from the REPL's `…` (the pane before them, now the REPL has
// no prompt of its own — da-rfs9), Tab reaches ✎, the keyboard types and
// applies a command, and when that command writes, Tab reaches its ▶.
describe('by keyboard', () => {
	/** Tab forward from where focus is until `selector` has it. */
	async function tabTo(page, selector) {
		for (let i = 0; i < 40; i++) {
			if (await page.evaluate((s) => document.activeElement?.matches(s), selector)) return;
			await page.keyboard.press('Tab');
		}
		assert.fail(`Tab never reached ${selector}`);
	}

	test("the custom pane's ✎ and ▶ are reachable", async () => {
		await plain.room('keys', async ({ page, clock }, room) => {
			await page.locator(`section[data-pane="${inRoom(room, 'repl')}"] .titlebar button.title`).focus();
			await tabTo(page, 'button.edit');
			await page.keyboard.press('Enter');
			await page.keyboard.type('occur tea');
			await page.keyboard.press('Enter');
			await page.locator('button.play').waitFor();
			// Copilot on #167: both are icon-only, so each needs a name and
			// must not submit a form it might one day sit in.
			for (const [selector, name] of [['button.play', 'run it'], ['button.edit', 'edit the command']]) {
				const el = page.locator(selector);
				assert.equal(await el.getAttribute('aria-label'), name);
				assert.equal(await el.getAttribute('type'), 'button');
			}
			// A minute on, or the second `occur tea` lands on the first one's
			// instant under the held clock (da-ydao) and there is still one.
			await clock.forward(60_000);
			await tabTo(page, 'button.play');
			await page.keyboard.press('Enter');
			// From node, one read every 100ms: `entries` is a whole `db
			// backup` through the CLI. Asked as an async predicate on every
			// animation frame — the suite's only one — it failed once under
			// load with "Resulting promise was garbage collected" (da-31464).
			await until(
				() => page.evaluate(async () => (await window.datum.entries(500)).filter((e) => e.field === 'tea').length),
				(n) => n === 2
			);
		});
	});
});

// The layout engine's controls (da-jdop): every pane's `…` menu, every
// item in it, and every draggable border between panes must be reachable
// by keyboard — a Tab walk from the top of the page lands on each of
// them, the menu opens on Enter with its first item focused, the arrows
// walk the items and every one of them is on screen, and an arrow on a
// focused border moves it.
describe('the layout engine, by keyboard', () => {
	/** Tab from the top until nothing new is reached; the elements landed on. */
	async function tabWalk(page) {
		await focusTop(page);
		const seen = new Set();
		for (let i = 0; i < 80; i++) {
			await page.keyboard.press('Tab');
			const id = await page.evaluate(() => {
				const el = document.activeElement;
				if (!el || el === document.body) return null;
				if (!el.dataset.walk) el.dataset.walk = `walk-${Math.random().toString(36).slice(2, 8)}`;
				return el.dataset.walk;
			});
			if (id === null || seen.has(id)) break;
			seen.add(id);
		}
		return seen;
	}

	test('a Tab walk reaches every menu button, every border and the nowview steps, in layout mode', async () => {
		await desktop.solo(async ({ page }) => {
			// Layout mode (da-vvcxc) draws every control there is: the borders,
			// each pane's `+`, `×` and handle, and each root tab's `×`.
			await enterLayoutMode(page);
			const reached = await tabWalk(page);
			// The root bar's `+` and its active tab — which is the page's own
			// menu button (da-wjob) — and the logo,
			// which is the database menu (da-y992), are walked too, and each
			// pane's title and `+` (da-9vtw).
			const controls = await page.evaluate(() =>
				[
					...document.querySelectorAll(
						'button.title, button.add, button.maximize, [role="separator"], .nowview-steps button, .root-bar [role="tab"][aria-selected="true"], .root-bar button.tab-close:not(:disabled), .root-bar button.gear, .prompt-bar .brand'
					)
				]
					.filter((el) => el.getClientRects().length > 0)
					.map((el) => ({
						walk: el.dataset.walk ?? null,
						what: el.matches('button.title')
							? `menu of ${el.closest('section')?.dataset.title}`
							: el.matches('.root-bar button.gear')
								? 'the gear'
								: el.matches('.root-bar button.tab-close')
									? 'the × of a root tab'
									: el.matches('.root-bar [role="tab"]')
										? 'menu of the root bar'
										: el.matches('button.add')
											? `the + of ${el.closest('section')?.dataset.title ?? 'the root bar'}`
											: el.matches('button.maximize')
												? `the handle of ${el.closest('section')?.dataset.title}`
													: el.matches('.nowview-steps button')
													? el.getAttribute('aria-label')
													: el.matches('.brand')
														? 'the logo’s database menu'
														: 'border'
					}))
			);
			assert.equal(controls.filter((c) => c.what.endsWith('window')).length, 2, 'no nowview steps');
			assert.equal(controls.filter((c) => c.what === 'menu of the root bar').length, 1, 'the root bar has no menu button');
			assert.equal(controls.filter((c) => c.what === 'the gear').length, 1, 'the root bar has no gear (da-pp1a)');
			// The one root tab wears its ×, disabled: the page's last tab does not close.
			assert.equal(await page.locator('.root-bar button.tab-close:disabled').count(), 1, 'the root tab wears no × in layout mode');
			assert.equal(await page.locator('button.faint').count(), 0, 'the margin handles are back (da-wjob)');
			assert.ok(controls.some((c) => c.what === 'border'), 'the default layout has no draggable border');
			for (const control of controls) {
				assert.ok(control.walk && reached.has(control.walk), `${control.what} is not in the Tab order`);
			}
		});
	});

	test('every menu item is on screen and reachable by the arrows, in either mode', async () => {
		await desktop.solo(async ({ page }) => {
			// A pane's title, and the root bar's active tab (da-wjob) and gear:
			// in normal mode what each thing IS, in layout mode what is done
			// to it (da-vvcxc).
			const openers = page.locator('button.title:visible, .root-bar [role="tab"][aria-selected="true"], .root-bar button.gear');
			/** Every label the arrows reached, across all the menus. */
			const seen = new Set();
			for (const layoutMode of [false, true]) {
				if (layoutMode) await enterLayoutMode(page);
				const menus = await openers.count();
				for (let m = 0; m < menus; m++) {
					await openers.nth(m).focus();
					await page.keyboard.press('Enter');
					const items = page.locator('[role="menu"] button');
					// The walk is over what can be FOCUSED: a menu whose every item
					// is disabled opens, holds focus on itself, and closes on the
					// Escape below.
					const enabled = await items.evaluateAll((all) => all.filter((b) => !b.disabled).length);
					const visited = new Set();
					for (let i = 0; i < enabled; i++) {
						// By name rather than by words: the four arrows of Split and
						// of Break out (da-9vtw) draw the same four glyphs.
						const label = await page.evaluate(() => {
							const el = document.activeElement;
							const { top, bottom, left, right } = el.getBoundingClientRect();
							const ok = top >= 0 && left >= 0 && bottom <= innerHeight && right <= innerWidth;
							return {
								text: el.getAttribute('aria-label') ?? el.textContent.trim(),
								role: el.getAttribute('role'),
								ok,
								disabled: el.disabled
							};
						});
						assert.ok(label.role?.startsWith('menuitem'), `focus left the menu at ${label.text}`);
						assert.ok(label.ok, `${label.text} is off-screen`);
						visited.add(label.text);
						await page.keyboard.press('ArrowDown');
					}
					assert.equal(visited.size, enabled, `menu ${m}: the arrows visited ${visited.size} of ${enabled} items`);
					for (const label of visited) seen.add(label);
					await page.keyboard.press('Escape');
					await page.locator('[role="menu"]').waitFor({ state: 'detached' });
					assert.ok(
						await openers.nth(m).evaluate((el) => el === document.activeElement),
						'Escape did not hand focus back to the button'
					);
				}
			}
			// Layout mode's acts were walked too.
			for (const act of ['Change Widget', 'Accept', 'Discard changes'])
				assert.ok(seen.has(act), `${act} was never walked: ${[...seen].join(' | ')}`);
			// `Reset layout` (da-luwe) is an act like the other four: on the
			// screen and on the arrows' path, in the menu of `main` — the
			// built-in root tab this page opens with.
			assert.ok(seen.has('Reset layout'), `Reset layout was never walked: ${[...seen].join(' | ')}`);
		});
	});

	// The floors a pointer needs, after the margins came down (da-jgrn) and
	// the title row with them (da-om9v): a border is at least 8px across;
	// what opens a pane's menu — its title (da-9vtw) — and the badges
	// beside it, the `+`, the maximize (da-00am) and the `×` that closes
	// what the title names (da-efr4i), are the row's height
	// (16px) where the pointer is fine and a fingertip's 24px where it is
	// coarse; the root bar's are 24px under either.
	// Two screens share one app; the coarse pointer cannot — `hasTouch` is a
	// property of the browser context, not of the viewport, so it needs one
	// of its own.
	for (const [name, screens, withIt] of [
		['by pointer', [{ width: 1280, height: 900 }, { width: 390, height: 844 }], (fn) => desktop.solo(fn)],
		[
			'under a finger',
			[{ width: 390, height: 844 }],
			(fn) => withApp(fn, { viewport: { width: 390, height: 844 }, device: { hasTouch: true, isMobile: true } })
		]
	]) {
		test(`every border is ≥ 8px and every … badge is on its floor, ${name}`, async (t) => {
			await withIt(async ({ page }) => {
				// Layout mode draws them all: the borders, the `+`, the `×`.
				await enterLayoutMode(page);
				await atEachScreen(t, page, screens, async () => {
					const sizes = await page.evaluate(() => {
						const box = (el) => {
							const b = el.getBoundingClientRect();
							return [b.width, b.height];
						};
						const shown = (s) => [...document.querySelectorAll(s)].filter((el) => el.getClientRects().length > 0);
						return {
							coarse: matchMedia('(pointer: coarse)').matches,
							borders: shown('[role="separator"]').map((el) => Math.min(...box(el))),
							panes: shown(
								'section button.title, section button.add, section button.maximize, section button.close'
							).map(box),
							root: shown('.root-bar button.add, .root-bar button.tab-close, .root-bar button.gear, .root-bar [role="tab"]').map(box)
						};
					});
					assert.equal(sizes.coarse, name.endsWith('finger'));
					assert.ok(sizes.borders.length > 0 && sizes.panes.length > 0 && sizes.root.length > 0);
					for (const across of sizes.borders) assert.ok(across >= 8, `a border is ${across}px across`);
					const floor = sizes.coarse ? 24 : 16;
					for (const [w, h] of sizes.panes) assert.ok(w >= floor && h >= floor, `a pane's menu button is ${w}x${h}`);
					for (const [w, h] of sizes.root) assert.ok(w >= 24 && h >= 24, `a root bar control is ${w}x${h}`);
				});
			});
		});
	}

	// Normal mode (da-vvcxc), what a lock was (da-2a5y), has FEWER targets,
	// not unreachable ones: no `+` and no border is in the DOM at all, a menu
	// only where a widget has options (da-gvdc), and what is left — the tabs
	// — is still walked to. The way into layout mode is the open root tab,
	// which the walk above reaches.
	test('normal mode has no `+` and no border, a menu only for options, and its tabs are still in the Tab order', async () => {
		await withApp(
			async ({ page }) => {
				const reached = await tabWalk(page);
				const controls = await page.evaluate(() => ({
					hidden: document.querySelectorAll('section.root button.add, section.root [role="separator"]').length,
					// A menu is left only where a widget has options (da-gvdc): the repl's.
					titles: [...document.querySelectorAll('section.root button.title')].map((b) => b.closest('section.pane')?.dataset.title),
					tabs: [...document.querySelectorAll('section.root [role="tab"]')].map((el) => el.dataset.walk ?? null)
				}));
				assert.equal(controls.hidden, 0, 'normal mode still offers a `+` or a border');
				assert.deepEqual(controls.titles, ['repl'], 'normal mode offers a menu beyond a widget’s options');
				assert.equal(controls.tabs.length, 2);
				// The active tab is the strip's one stop; the arrows walk the rest.
				assert.ok(controls.tabs.some((walk) => walk && reached.has(walk)), 'the tab strip is not in the Tab order');
			},
			{
				viewport: { width: 1280, height: 900 },
				prepare: (context) => context.addInitScript((json) => localStorage.setItem('datum.layout.draft.v1.main', json), TWO)
			}
		);
	});

	test('an arrow on a focused border moves it by 2%', async () => {
		await desktop.solo(async ({ page }) => {
			await enterLayoutMode(page);
			const border = page.locator('.split.row > [role="separator"]');
			await border.focus();
			const was = Number(await border.getAttribute('aria-valuenow'));
			await page.keyboard.press('ArrowRight');
			await page.waitForFunction(
				(w) => Number(document.querySelector('.split.row > [role="separator"]').getAttribute('aria-valuenow')) === w + 2,
				was
			);
			await page.keyboard.press('ArrowLeft');
			await page.waitForFunction(
				(w) => Number(document.querySelector('.split.row > [role="separator"]').getAttribute('aria-valuenow')) === w,
				was
			);
		});
	});
});

/** A `main` of a root holder over [the REPL | a tabbed tail/nowview pane]. */
const bar = { edge: 'top', align: 'start' };
const TWO = JSON.stringify({
	version: 1,
	name: 'main',
	root: {
		id: 'root',
		active: 0,
		bar,
		tabs: [
			{
				id: 'root-tab',
				content: {
					id: 'row',
					direction: 'row',
					ratios: [0.5, 0.5],
					children: [
						{ id: 'repl', active: 0, bar, tabs: [{ id: 'repl-tab', content: { id: 'repl-widget', kind: 'repl', settings: {} } }] },
						{
							id: 'tabbed',
							active: 0,
							bar,
							tabs: [
								{ id: 'tabbed-tail', content: { id: 'tabbed-tail-widget', kind: 'tail', settings: {} } },
								{ id: 'tabbed-nowview', content: { id: 'tabbed-nowview-widget', kind: 'nowview', settings: {} } }
							]
						}
					]
				}
			}
		]
	}
});

// The input widget's own page (da-0dhc, back as da-3zon). The widget is
// the bottom bar's command line drawn in a pane, and vividn asked for it
// because the bar is hard to see on a desktop — so the two claims are
// that its box is ON THE SCREEN it is drawn on, at every phone size, and
// that the keyboard can get into it without a pointer. A prompt below the
// fold, or one only a click can reach, is not a command line.
describe('the input widget at /input', () => {
	test('its box and its receipt are on screen', async (t) => {
		await phone.solo(
			async (app) => {
				await app.goto('/input');
				await app.page.locator('.input-widget input').waitFor();
				await atEachScreen(t, app.page, SCREENS, async () => {
					await assertOnScreen(app.page, '.input-widget', "the input widget's box");
					await assertOnScreen(app.page, '.input-widget input', "the input widget's prompt");
					// And the bar it stands beside is still there and still usable.
					await assertOnScreen(app.page, '.prompt-bar input', "the bottom bar's prompt");
				});
			}
		);
	});

	test('a Tab walk from the top of the page reaches its input, as it does the bar’s', async () => {
		await plain.solo(async (app) => {
			await app.goto('/input');
			await app.page.locator('.input-widget input').waitFor();
			await focusTop(app.page);
			const reached = new Set();
			for (let i = 0; i < 40; i++) {
				await app.page.keyboard.press('Tab');
				const where = await app.page.evaluate(() =>
					document.activeElement?.closest('.input-widget')
						? 'widget'
						: document.activeElement?.closest('.prompt-bar')
							? 'bar'
							: null
				);
				if (where) reached.add(where);
				if (reached.size === 2) break;
			}
			assert.deepEqual([...reached].sort(), ['bar', 'widget']);
		});
	});
});
