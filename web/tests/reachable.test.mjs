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
// And the count that wraps is not a constant: it is what fits across the
// screen, so it falls as the screen narrows — five fields at 390, four at
// 320, three at 280. Fixing a width and hunting for the count would just
// re-learn the width. These log one ordinary handful at every screen in the
// matrix, which wraps at all of them.

import test, { before, after, describe } from 'node:test';
import assert from 'node:assert/strict';
import { startApp, stopApp, withApp } from './support/app.mjs';

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

/** Log `fields` and wait until every one of them has a chip. */
async function track(page, submit, fields) {
	for (const field of fields) await submit(`occur ${field}`);
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

		test(`the widget bar's + is on screen at ${size}`, async () => {
			await withApp(async ({ page }) => {
				await assertOnScreen(page, '.bar button.add-button', 'the add-a-field (+) button');
			}, { viewport });
		});

		test(`a logged field's chip is on screen at ${size}`, async () => {
			await withApp(async ({ page, submit }) => {
				await submit('occur coffee');
				await page.locator('.bar button.widget', { hasText: 'coffee' }).waitFor();
				await assertOnScreen(page, '.bar button.widget', "the coffee field's chip");
			}, { viewport });
		});
	}

	for (const viewport of SCREENS) {
		const size = `${viewport.width}x${viewport.height}`;

		// The one that da-hkpd's fix and its verification both missed: the
		// bar was pinned to the bottom, and then wrapped DOWNWARD past it.
		test(`the + is still on screen with ${FIELDS.length} fields at ${size}`, async () => {
			await withApp(async ({ page, submit }) => {
				await track(page, submit, FIELDS);
				await assertOnScreen(page, '.bar button.add-button', 'the add-a-field (+) button');
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

	// A button whose panel opens off the bottom edge is no more reachable
	// than a button that is off it: the bar is the frame's bottom row, so
	// its panels hang upward.
	test('the form the + opens is on screen', async () => {
		await withApp(async ({ page }) => {
			await page.locator('.bar button.add-button').click();
			await assertOnScreen(page, '.panel.add', "the add-a-field form");
		}, { viewport: PHONE });
	});

	test("a field's state menu is on screen", async () => {
		await withApp(async ({ page, submit }) => {
			await submit('switch mood happy');
			const chip = page.locator('.bar button.widget', { hasText: 'mood' });
			await chip.waitFor();
			await chip.click();
			await assertOnScreen(page, '.bar .panel', "mood's state menu");
		}, { viewport: PHONE });
	});

	// The stats widget's own page (da-n7r0): reached by the header's link,
	// and its pickers on the screen it opens on.
	for (const viewport of SCREENS) {
		const size = `${viewport.width}x${viewport.height}`;

		test(`/stats and its pickers are on screen at ${size}`, async () => {
			await withApp(async ({ page, submit }) => {
				await track(page, submit, FIELDS);
				await assertOnScreen(page, 'header nav a[href="/stats"]', 'the stats link');
				await page.locator('header nav a[href="/stats"]').click();
				await page.locator('.stats button.field').nth(FIELDS.length - 1).waitFor();
				await assertOnScreen(page, '.stats button.field', "the first field's chip");
				await assertOnScreen(page, '.stats .range button', 'the range picker');
			}, { viewport });
		});
	}

	test('the form the + opens is on screen with a wrapped bar', async () => {
		await withApp(async ({ page, submit }) => {
			await track(page, submit, FIELDS);
			await page.locator('.bar button.add-button').click();
			await assertOnScreen(page, '.panel.add', 'the add-a-field form');
		}, { viewport: PHONE });
	});
});

// The command pane's controls (da-wb2n) are buttons in the tab order, not
// click targets: from the prompt, Tab reaches ✎, the keyboard types and
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
			await page.locator('.repl input').focus();
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

	test('a Tab walk reaches every … button, every border and the nowview steps', async () => {
		await withApp(async ({ page }) => {
			const reached = await tabWalk(page);
			const controls = await page.evaluate(() =>
				[...document.querySelectorAll('button.more, [role="separator"], .nowview-steps button')].map((el) => ({
					walk: el.dataset.walk ?? null,
					what: el.matches('button.more')
						? `menu of ${el.closest('section').dataset.title}`
						: el.matches('.nowview-steps button')
							? el.getAttribute('aria-label')
							: 'border'
				}))
			);
			assert.equal(controls.filter((c) => c.what.endsWith('window')).length, 2, 'no nowview steps');
			assert.ok(controls.some((c) => c.what === 'border'), 'the default layout has no draggable border');
			for (const control of controls) {
				assert.ok(control.walk && reached.has(control.walk), `${control.what} is not in the Tab order`);
			}
		}, { viewport: { width: 1280, height: 900 } });
	});

	test('every menu item is on screen and reachable by the arrows', async () => {
		await withApp(async ({ page }) => {
			const menus = await page.locator('button.more').count();
			for (let m = 0; m < menus; m++) {
				await page.locator('button.more').nth(m).focus();
				await page.keyboard.press('Enter');
				const items = page.locator('[role="menu"] button');
				const count = await items.count();
				const visited = new Set();
				for (let i = 0; i < count; i++) {
					const label = await page.evaluate(() => {
						const el = document.activeElement;
						const { top, bottom, left, right } = el.getBoundingClientRect();
						const ok = top >= 0 && left >= 0 && bottom <= innerHeight && right <= innerWidth;
						return { text: el.textContent.trim(), role: el.getAttribute('role'), ok, disabled: el.disabled };
					});
					assert.ok(label.role?.startsWith('menuitem'), `focus left the menu at ${label.text}`);
					assert.ok(label.ok, `${label.text} is off-screen`);
					visited.add(label.text);
					await page.keyboard.press('ArrowDown');
				}
				const enabled = await items.evaluateAll((all) => all.filter((b) => !b.disabled).length);
				assert.equal(visited.size, enabled, `menu ${m}: the arrows visited ${visited.size} of ${enabled} items`);
				await page.keyboard.press('Escape');
				await page.locator('[role="menu"]').waitFor({ state: 'detached' });
				assert.ok(
					await page.locator('button.more').nth(m).evaluate((el) => el === document.activeElement),
					'Escape did not hand focus back to the button'
				);
			}
		}, { viewport: { width: 1280, height: 900 } });
	});

	// The floors a finger needs, after the margins came down (da-jgrn): a
	// border is at least 8px across and every `…` badge at least 24px each way.
	for (const [name, viewport] of [
		['1280x900', { width: 1280, height: 900 }],
		['390x844', { width: 390, height: 844 }]
	]) {
		test(`every border is ≥ 8px and every … badge ≥ 24px at ${name}`, async () => {
			await withApp(async ({ page }) => {
				const sizes = await page.evaluate(() => ({
					borders: [...document.querySelectorAll('[role="separator"]')].map((el) => {
						const b = el.getBoundingClientRect();
						return Math.min(b.width, b.height);
					}),
					badges: [...document.querySelectorAll('button.more')].map((el) => {
						const b = el.getBoundingClientRect();
						return [b.width, b.height];
					})
				}));
				assert.ok(sizes.borders.length > 0 && sizes.badges.length > 0);
				for (const across of sizes.borders) assert.ok(across >= 8, `a border is ${across}px across`);
				for (const [w, h] of sizes.badges) assert.ok(w >= 24 && h >= 24, `a badge is ${w}x${h}`);
			}, { viewport });
		});
	}

	// Locked (da-2a5y), a layout has FEWER targets, not unreachable ones:
	// no `…` and no border is in the DOM at all, and what is left — the
	// tabs — is still walked to.
	test('a locked layout has no … and no border, and its tabs are still in the Tab order', async () => {
		await withApp(
			async ({ page }) => {
				const reached = await tabWalk(page);
				const controls = await page.evaluate(() => ({
					hidden: document.querySelectorAll('button.more, [role="separator"]').length,
					tabs: [...document.querySelectorAll('[role="tab"]')].map((el) => el.dataset.walk ?? null)
				}));
				assert.equal(controls.hidden, 0, 'a locked layout still offers a … or a border');
				assert.equal(controls.tabs.length, 2);
				// The active tab is the strip's one stop; the arrows walk the rest.
				assert.ok(controls.tabs.some((walk) => walk && reached.has(walk)), 'the tab strip is not in the Tab order');
			},
			{
				viewport: { width: 1280, height: 900 },
				prepare: (context) =>
					context.addInitScript((json) => localStorage.setItem('datum.layout.draft.v1.default', json), LOCKED)
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

/** A root holder whose one tab is locked, over [the REPL | a tabbed tail/nowview pane]. */
const bar = { edge: 'top', align: 'start' };
const LOCKED = JSON.stringify({
	version: 1,
	name: 'default',
	root: {
		id: 'root',
		active: 0,
		bar,
		tabs: [
			{
				id: 'root-tab',
				locked: true,
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

// The input widget's own page (da-0dhc): a prompt at the bottom and the
// history above it, and the history is the thing that grows. A widget
// whose history pushed its prompt off the bottom of a phone would be a
// box nobody can type into — so it runs with more history than any of
// these screens can show.
describe('the input widget at /input', () => {
	for (const viewport of SCREENS) {
		const size = `${viewport.width}x${viewport.height}`;

		test(`keeps its prompt on screen under a long history at ${size}`, async () => {
			await withApp(
				async (app) => {
					await app.goto('/input');
					await assertOnScreen(app.page, '.input-widget input', "the input widget's prompt");
				},
				{
					viewport,
					prepare: (context) =>
						context.addInitScript((key) => {
							const ranAt = Date.now();
							const lines = Array.from({ length: 60 }, (_, i) => `occur coffee cups=${i}`);
							localStorage.setItem(
								key,
								JSON.stringify({
									scrollback: lines.map((line, i) => ({
										line,
										ranAt: ranAt - (60 - i) * 60_000,
										result: { argv: line.split(' '), code: 0, stdout: '', stderr: '' }
									})),
									history: lines
								})
							);
						}, 'datum.repl.v1')
				}
			);
		});
	}
});
