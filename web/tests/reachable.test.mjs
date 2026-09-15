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
// The bar is the fields widget now (da-c7u8): Track + first, then a button
// per TRACKED field — so the handful here is declared with `spec set`,
// which is what gives a field its button.
//
// And the count that wraps is not a constant: it is what fits across the
// screen, so it falls as the screen narrows — five fields at 390, four at
// 320, three at 280. Fixing a width and hunting for the count would just
// re-learn the width. These log one ordinary handful at every screen in the
// matrix, which wraps at all of them.

import test, { before, after, describe } from 'node:test';
import assert from 'node:assert/strict';
import { startApp, stopApp, withApp } from './support/app.mjs';
import { chooser } from './support/layout.mjs';

before(startApp);
after(stopApp);

/** A phone in portrait, once the browser's own chrome is subtracted. */
const PHONE = { width: 390, height: 640 };

/** The shortest screen worth claiming to support — an SE with chrome. */
const SHORT = { width: 390, height: 560 };

/** An SE's width, which is still common, and the narrowest phone there is. */
const NARROW = { width: 320, height: 664 };
const TINY = { width: 280, height: 664 };

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
async function track(page, submit, fields) {
	for (const field of fields) await submit(`spec set ${field} --default-verb occur`);
	await page.locator('.bar button.widget').nth(fields.length - 1).waitFor();
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
	for (const viewport of [PHONE, SHORT]) {
		const size = `${viewport.width}x${viewport.height}`;

		test(`the fields widget's Track + is on screen at ${size}`, async () => {
			await withApp(async ({ page }) => {
				await assertOnScreen(page, '.bar button.track', 'Track +');
			}, { viewport });
		});

		test(`a tracked field's button is on screen at ${size}`, async () => {
			await withApp(async ({ page, submit }) => {
				await submit('spec set coffee --default-verb occur');
				await page.locator('.bar button.widget', { hasText: 'coffee' }).waitFor();
				await assertOnScreen(page, '.bar button.widget', "the coffee field's chip");
			}, { viewport });
		});
	}

	for (const viewport of SCREENS) {
		const size = `${viewport.width}x${viewport.height}`;

		// The one that da-hkpd's fix and its verification both missed: the
		// bar was pinned to the bottom, and then wrapped DOWNWARD past it.
		test(`Track + is still on screen with ${FIELDS.length} fields at ${size}`, async () => {
			await withApp(async ({ page, submit }) => {
				await track(page, submit, FIELDS);
				await assertOnScreen(page, '.bar button.track', 'Track +');
			}, { viewport });
		});

		// The other edge of a wrapped bar: growing it upward must not push
		// its first row off the top instead.
		test(`the first chip is on screen with ${FIELDS.length} fields at ${size}`, async () => {
			await withApp(async ({ page, submit }) => {
				await track(page, submit, FIELDS);
				await assertOnScreen(page, '.bar button.widget', "the first field's chip");
			}, { viewport });
		});
	}

	// The gear (da-pp1a) is the root bar's right end: the global settings
	// menu, and since da-luwe the only thing in it is the build stamp —
	// the reset it also held is a layout's own act now. The bar is one row
	// that never wraps, so the thing that can push the gear off is a
	// crowded strip of tabs on a narrow screen — and its menu, which opens
	// from the right edge, has to land on the screen as well.
	for (const viewport of [...SCREENS, { width: 320, height: 480 }]) {
		const size = `${viewport.width}x${viewport.height}`;

		test(`the gear and its menu are on screen at ${size}`, async () => {
			await withApp(async ({ page }) => {
				await assertOnScreen(page, '.root-bar button.gear', 'the gear');
				const box = await page.locator('.root-bar button.gear').boundingBox();
				const width = await page.evaluate(() => document.documentElement.clientWidth);
				assert.ok(box.width >= 24 && box.height >= 24, `the gear is ${box.width}x${box.height}`);
				assert.ok(box.x >= 0 && box.x + box.width <= width + 1, `the gear is ${box.x}–${box.x + box.width} on ${width}px`);

				await page.locator('.root-bar button.gear').click();
				const items = page.locator('[role="menu"][aria-label="settings"] button');
				// The build stamp alone — a fact, so disabled, and so the one
				// menu here with nothing in it to press (da-luwe).
				assert.equal(await items.count(), 1);
				assert.equal(await items.evaluateAll((all) => all.filter((b) => !b.disabled).length), 0);
				const menu = await page.locator('[role="menu"][aria-label="settings"]').boundingBox();
				const screen = await page.evaluate(() => ({
					width: document.documentElement.clientWidth,
					height: document.documentElement.clientHeight
				}));
				assert.ok(menu.x >= 0 && menu.x + menu.width <= screen.width + 1, `the settings menu is ${menu.x}–${menu.x + menu.width}`);
				assert.ok(menu.y >= 0 && menu.y + menu.height <= screen.height + 1, 'the settings menu is taller than the screen');
			}, { viewport });
		});
	}

	// The one chooser (da-uyx1) is a modal over the whole page, so the
	// screen it has to fit is the narrowest one: 320px, with every item a
	// fingertip's 24px and the dialog itself inside the viewport.
	for (const viewport of [NARROW, TINY]) {
		const size = `${viewport.width}x${viewport.height}`;

		test(`the chooser fits, with 24px items, at ${size}`, async () => {
			await withApp(async ({ page }) => {
				await page.locator('.root-bar button[aria-label="open a layout"]').click();
				await chooser(page).waitFor();
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
			}, { viewport });
		});
	}

	// The REPL's own control (da-bhvy) is on the bottom bar's top line, and
	// what it opens hangs upward from there. A lid a thumb cannot find is
	// no lid: it is asserted here at every screen, box and all.
	for (const viewport of SCREENS) {
		const size = `${viewport.width}x${viewport.height}`;

		test(`the REPL control is on screen, and opens there, at ${size}`, async () => {
			await withApp(async ({ page }) => {
				await assertOnScreen(page, '.prompt-bar .caret', 'the REPL control');
				const box = await page.locator('.prompt-bar .caret').boundingBox();
				const width = await page.evaluate(() => document.documentElement.clientWidth);
				assert.ok(box.height >= 24, `the REPL control is ${box.height}px tall`);
				assert.ok(
					box.x >= 0 && box.x + box.width <= width + 1,
					`the REPL control is ${box.x}–${box.x + box.width} on a ${width}px screen`
				);

				await page.locator('.prompt-bar .caret').click();
				await assertOnScreen(page, '.prompt-bar .expansion', 'the REPL it opens');
			}, { viewport });
		});
	}

	// A button whose panel opens off the bottom edge is no more reachable
	// than a button that is off it: the bar is the frame's bottom row, so
	// its panels hang upward.
	test('the form Track + opens is on screen', async () => {
		await withApp(async ({ page }) => {
			await page.locator('.bar button.track').click();
			await assertOnScreen(page, '.panel.add', "the add-a-field form");
		}, { viewport: PHONE });
	});

	// A PLAIN press fills the bar with the field's states in place (da-g635)
	// — nothing floats, so there is no panel to be off the screen. The panel
	// a LONG press opens is still one, bubbles and states together, and it is
	// the one this asserts.
	test("a field's state bubbles are on screen", async () => {
		await withApp(async ({ page, submit }) => {
			await submit('spec set mood --default-verb switch --state happy');
			const chip = page.locator('.bar button.widget', { hasText: 'mood' });
			await chip.waitFor();
			await chip.click({ button: 'right' });
			await assertOnScreen(page, '.bar .panel', "mood's state panel");
		}, { viewport: PHONE });
	});

	test("the states a press fills the bar with are on screen", async () => {
		await withApp(async ({ page, submit }) => {
			// mood is the switch here, so the other four are what wrap the bar.
			await submit('spec set mood --default-verb switch --state happy --state sad');
			await track(page, submit, FIELDS.filter((f) => f !== 'mood'));
			const chip = page.locator('.bar button.widget', { hasText: 'mood' });
			await chip.waitFor();
			await chip.click();
			await assertOnScreen(page, '.bar [data-back]', 'the way back to the fields');
			await assertOnScreen(page, '.bar [data-state="happy"]', "mood's happy");
			await assertOnScreen(page, '.bar [data-add-state]', 'the + that adds a state');
		}, { viewport: PHONE });
	});

	// The stats widget (da-n7r0) as a root tab (da-0a5o): reached by its
	// URL, its tab on the root bar, and its pickers on the screen it opens on.
	for (const viewport of SCREENS) {
		const size = `${viewport.width}x${viewport.height}`;

		test(`/stats and its pickers are on screen at ${size}`, async () => {
			await withApp(async (app) => {
				const { page, submit } = app;
				await track(page, submit, FIELDS);
				await app.goto('/stats');
				await page.locator('.root-bar [role="tab"][aria-selected="true"]', { hasText: 'stats' }).waitFor();
				await assertOnScreen(page, '.root-bar [role="tab"][aria-selected="true"]', 'the stats tab');
				await page.locator('.stats button.field').nth(FIELDS.length - 1).waitFor();
				await assertOnScreen(page, '.stats button.field', "the first field's chip");
				await assertOnScreen(page, '.stats .range button', 'the range picker');
				// And the bottom bar's prompt, whatever tab is in front.
				await assertOnScreen(page, '.prompt-bar input', "the bottom bar's prompt");
				await assertOnScreen(page, '.prompt-bar .hint-button', 'the beta hint');
			}, { viewport });
		});
	}

	test('the form Track + opens is on screen with a wrapped bar', async () => {
		await withApp(async ({ page, submit }) => {
			await track(page, submit, FIELDS);
			await page.locator('.bar button.track').click();
			await assertOnScreen(page, '.panel.add', 'the add-a-field form');
		}, { viewport: PHONE });
	});
});

// The command pane's controls (da-wb2n) are buttons in the tab order, not
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

	test("the command pane's ✎ and ▶ are reachable", async () => {
		await withApp(async ({ page }) => {
			await page.locator('section[data-pane="repl"] .titlebar button.title').focus();
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
			await tabTo(page, 'button.play');
			await page.keyboard.press('Enter');
			await page.waitForFunction(
				async () => (await window.datum.entries(500)).filter((e) => e.field === 'tea').length === 2
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
		await page.evaluate(() => document.body.focus());
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

	test('a Tab walk reaches every menu button, every border and the nowview steps', async () => {
		await withApp(async ({ page }) => {
			const reached = await tabWalk(page);
			// The root bar's `+` and its active tab — which is the page's own
			// menu button (da-wjob) — and the beta hint (da-0a5o) are walked
			// too, and each pane's title and `+` (da-9vtw).
			const controls = await page.evaluate(() =>
				[
					...document.querySelectorAll(
						'button.title, button.add, [role="separator"], .nowview-steps button, .root-bar [role="tab"][aria-selected="true"], .root-bar button.gear, .prompt-bar .hint-button'
					)
				]
					.filter((el) => el.getClientRects().length > 0)
					.map((el) => ({
						walk: el.dataset.walk ?? null,
						what: el.matches('button.title')
							? `menu of ${el.closest('section')?.dataset.title}`
							: el.matches('.root-bar button.gear')
								? 'the gear'
								: el.matches('.root-bar [role="tab"]')
									? 'menu of the root bar'
									: el.matches('button.add')
										? `the + of ${el.closest('section')?.dataset.title ?? 'the root bar'}`
										: el.matches('.nowview-steps button')
											? el.getAttribute('aria-label')
											: el.matches('.hint-button')
												? 'the beta hint'
												: 'border'
					}))
			);
			assert.equal(controls.filter((c) => c.what.endsWith('window')).length, 2, 'no nowview steps');
			assert.equal(controls.filter((c) => c.what === 'menu of the root bar').length, 1, 'the root bar has no menu button');
			assert.equal(controls.filter((c) => c.what === 'the gear').length, 1, 'the root bar has no gear (da-pp1a)');
			assert.equal(await page.locator('button.faint').count(), 0, 'the margin handles are back (da-wjob)');
			assert.ok(controls.some((c) => c.what === 'border'), 'the default layout has no draggable border');
			for (const control of controls) {
				assert.ok(control.walk && reached.has(control.walk), `${control.what} is not in the Tab order`);
			}
		}, { viewport: { width: 1280, height: 900 } });
	});

	test('every menu item is on screen and reachable by the arrows', async () => {
		await withApp(async ({ page }) => {
			// A pane's title, and the root bar's active tab (da-wjob).
			const openers = page.locator('button.title:visible, .root-bar [role="tab"][aria-selected="true"], .root-bar button.gear');
			const menus = await openers.count();
			/** Every label the arrows reached, across all the menus. */
			const seen = new Set();
			for (let m = 0; m < menus; m++) {
				await openers.nth(m).focus();
				await page.keyboard.press('Enter');
				const items = page.locator('[role="menu"] button');
				// The walk is over what can be FOCUSED: a menu whose every item
				// is disabled — the gear's, since da-luwe — opens, holds focus
				// on itself, and closes on the Escape below.
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
			// `Reset layout` (da-luwe) is an act like the other four: on the
			// screen and on the arrows' path, in the menu of `day` — the
			// built-in root tab this page opens with.
			assert.ok(seen.has('Reset layout'), `Reset layout was never walked: ${[...seen].join(' | ')}`);
		}, { viewport: { width: 1280, height: 900 } });
	});

	// The floors a pointer needs, after the margins came down (da-jgrn) and
	// the title row with them (da-om9v): a border is at least 8px across;
	// what opens a pane's menu — its title (da-9vtw) — and the `+` beside
	// it are the row's height (16px) where the pointer is fine and a
	// fingertip's 24px where it is coarse; the root bar's are 24px under
	// either.
	for (const [name, options] of [
		['1280x900', { viewport: { width: 1280, height: 900 } }],
		['390x844', { viewport: { width: 390, height: 844 } }],
		['390x844 under a finger', { viewport: { width: 390, height: 844 }, device: { hasTouch: true, isMobile: true } }]
	]) {
		test(`every border is ≥ 8px and every … badge is on its floor at ${name}`, async () => {
			await withApp(async ({ page }) => {
				const sizes = await page.evaluate(() => {
					const box = (el) => {
						const b = el.getBoundingClientRect();
						return [b.width, b.height];
					};
					const shown = (s) => [...document.querySelectorAll(s)].filter((el) => el.getClientRects().length > 0);
					return {
						coarse: matchMedia('(pointer: coarse)').matches,
						borders: shown('[role="separator"]').map((el) => Math.min(...box(el))),
						panes: shown('section button.title, section button.add').map(box),
						root: shown('.root-bar button.add, .root-bar button.gear, .root-bar [role="tab"]').map(box)
					};
				});
				assert.equal(sizes.coarse, name.endsWith('finger'));
				assert.ok(sizes.borders.length > 0 && sizes.panes.length > 0 && sizes.root.length > 0);
				for (const across of sizes.borders) assert.ok(across >= 8, `a border is ${across}px across`);
				const floor = sizes.coarse ? 24 : 16;
				for (const [w, h] of sizes.panes) assert.ok(w >= floor && h >= floor, `a pane's menu button is ${w}x${h}`);
				for (const [w, h] of sizes.root) assert.ok(w >= 24 && h >= 24, `a root bar control is ${w}x${h}`);
			}, options);
		});
	}

	// The Bar submenu (da-nk4k) is icons, not words: each a fingertip's 24px
	// each way and on screen, under a top bar and under a left-hand one —
	// whose rotation the row, portalled to the body, does not inherit.
	for (const [name, viewport] of [
		['1280x900', { width: 1280, height: 900 }],
		['390x844', { width: 390, height: 844 }]
	]) {
		test(`every Bar submenu icon is ≥ 24px and on screen, on a left-hand bar too, at ${name}`, async () => {
			await withApp(async ({ page }) => {
				const icons = page.locator('[role="menu"][aria-label="Bar"] button');
				for (const edge of ['top', 'left']) {
					// A pane's own menu, not the root bar's `+` or `…` (da-0a5o), which have no Bar.
					await page.locator('section.pane button.title').first().click();
					await page.locator('[role="menu"] button', { hasText: /^\s*Bar\s*$/ }).click();
					const seen = await icons.evaluateAll((all) =>
						all.map((el) => {
							const { top, bottom, left, right, width, height } = el.getBoundingClientRect();
							return {
								label: el.getAttribute('aria-label'),
								width,
								height,
								on: top >= 0 && left >= 0 && bottom <= innerHeight && right <= innerWidth,
								mode: getComputedStyle(el).writingMode
							};
						})
					);
					assert.equal(seen.length, 7);
					for (const icon of seen) {
						assert.ok(icon.width >= 24 && icon.height >= 24, `${edge} bar: ${icon.label} is ${icon.width}x${icon.height}`);
						assert.ok(icon.on, `${edge} bar: ${icon.label} is off-screen`);
						assert.equal(icon.mode, 'horizontal-tb', `${edge} bar: ${icon.label}`);
					}
					if (edge === 'top') await icons.and(page.locator('[aria-label="left"]')).click();
					else await page.keyboard.press('Escape');
				}
			}, { viewport });
		});
	}

	// Locked (da-2a5y), a layout has FEWER targets, not unreachable ones:
	// no menu, no `+` and no border is in the DOM at all, and what is left
	// — the tabs — is still walked to. The lock is this device's, on the `day`
	// tab (da-0a5o); the root bar above the layout keeps its own menu, which
	// is where `Unlock layout` is (da-crp5).
	test('a locked layout has no menu and no border, and its tabs are still in the Tab order', async () => {
		await withApp(
			async ({ page }) => {
				const reached = await tabWalk(page);
				const controls = await page.evaluate(() => ({
					hidden: document.querySelectorAll(
						'section.root button.title, section.root button.add, section.root [role="separator"]'
					).length,
					tabs: [...document.querySelectorAll('section.root [role="tab"]')].map((el) => el.dataset.walk ?? null)
				}));
				assert.equal(controls.hidden, 0, 'a locked layout still offers a menu, a `+` or a border');
				assert.equal(controls.tabs.length, 2);
				// The active tab is the strip's one stop; the arrows walk the rest.
				assert.ok(controls.tabs.some((walk) => walk && reached.has(walk)), 'the tab strip is not in the Tab order');
			},
			{
				viewport: { width: 1280, height: 900 },
				prepare: (context) =>
					context.addInitScript((json) => {
						localStorage.setItem('datum.layout.draft.v1.day', json);
						localStorage.setItem('datum.layout.locked.v1.day', '1');
					}, LOCKED)
			}
		);
	});

	test('an arrow on a focused border moves it by 2%', async () => {
		await withApp(async ({ page }) => {
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
		}, { viewport: { width: 1280, height: 900 } });
	});
});

/** The `day` layout this device locks: a root holder over [the REPL | a tabbed tail/nowview pane]. */
const bar = { edge: 'top', align: 'start' };
const LOCKED = JSON.stringify({
	version: 1,
	name: 'day',
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
