// Widget options (da-gvdc, da-vhwq): a kind declares its options in the
// registry, and the menu the widget's NAME opens draws one item each.
//
// vividn, 2026-09-17: «have the REPL be in reverse order (also allow the repl
// widget to be reversed as well as a widget option. (widgets in general
// should support options, a modal that can be opened from the menu.»
//
// And, 2026-09-21, the change this file is now written against: «Clicking
// on the widget name instead opens a menu to select session-level widget
// options, most of which can be changed and manipulated directly in the
// menu/submenus without having to open a new modal», and «when unlocked,
// maybe we have an extra option in this menu, something like "save options
// with layout" that then modifies the actual layout. When locked, nothing
// should be able to modify the layout like currently happens.»
//
// ⚑ THE THREE ARE ONE. What is held to here is that whole contract:
//
//  - a switch is an item that carries its own state and turns over in
//    place; a choice is a submenu of words. No modal.
//  - EVERY change is THIS SESSION'S — it changes what this tab shows, it
//    leaves the layout's dot alone, and a reload starts back at what the
//    layout says. A widget moving a setting of its own (nowview's − / +)
//    goes down that same path, which is why
//  - a LOCKED layout cannot be modified by anything, the widget's own
//    in-body controls included: the only writer left is «Save options with
//    layout», and the lock hides it.
//  - `Reset widget` still means the kind's defaults, session and all.
//  - a key this build does not declare is kept, not dropped.

import test, { before, after, describe } from 'node:test';
import assert from 'node:assert/strict';
import { sharedApp, startApp, stopApp, withApp } from './support/app.mjs';
import {
	DESKTOP,
	choiceLines,
	choose,
	chooseRoot,
	chooser,
	exactly,
	inRoom,
	manage,
	menuButton,
	menuLines,
	optionChecked,
	pane,
	PHONE,
	pickOption,
	switchItem,
	titledMenuButton,
	titledPane,
	toggleOption,
	toggleRootLock
} from './support/layout.mjs';

before(startApp);
after(stopApp);

// ONE APP for most of this file (da-6b3dj, sharedApp in support/app.mjs).
// An option this session sets on `main`'s own widgets outlives `solo`, so a
// test that sets one takes a ROOM, whose widgets are new ids (`inRoom`);
// `solo` serves a test that only reads, or whose last act hands the session
// back (a save, or the pane closed). What still opens an app of its own
// reloads, seeds the REPL's storage before boot, or is a screen no other
// test here asks for.
const shared = sharedApp(DESKTOP);

const SAVE = 'Save options with layout';
const REPL = 'section.pane[data-title="repl"]';
const NEWEST = 'newest first';

/** Open a pane's menu, read its lines, and close it again. */
async function linesOf(page, id) {
	await menuButton(page, id).click();
	const lines = await menuLines(page);
	await page.keyboard.press('Escape');
	await page.locator('[role="menu"]').waitFor({ state: 'detached' });
	return lines;
}

/** The REPL pane's echoed commands, top to bottom. */
const replEchoes = (page) =>
	page.locator(`${REPL} .echo`).allInnerTexts().then((ts) => ts.map((t) => t.replace(/^\s*❯\s*/, '').trim()));

const dotted = async (page, name) =>
	(await page.locator('.root-bar [role="tab"].modified').filter({ hasText: name }).count()) > 0;

/** The tree as one string: what a session setting must never appear in. */
const tree = (page) => page.evaluate(() => JSON.stringify(window.datum.layout.current()));

describe('widget options', () => {
	test('every declared option has a default, and a choice default is one of its choices', async () => {
		await shared.solo(async ({ page }) => {
			const kinds = await page.evaluate(() => window.datum.layout.kinds());
			const declared = kinds.filter((k) => k.options);
			// `tail` since da-hkjp: its type size is a declared option of the
			// KIND rather than a number written into a layout, so it is here
			// and its default must be one of its choices like any other.
			assert.deepEqual(declared.map((k) => k.kind).sort(), [
				'daystrip',
				'input',
				'nowview',
				'repl',
				'tail',
				'widgetbar'
			]);
			for (const { kind, options, defaultSettings } of declared) {
				for (const o of options) {
					assert.ok(o.key in defaultSettings, `${kind}.${o.key} has no default`);
					const d = defaultSettings[o.key];
					if (o.type === 'boolean') assert.equal(typeof d, 'boolean', `${kind}.${o.key}`);
					else assert.ok(o.choices.some((c) => c.value === d), `${kind}.${o.key}: ${d} is not a choice`);
				}
			}
		});
	});

	// The rows are IN the menu now, not behind an `Options…` that opens a
	// modal: a kind with options reads them under `Reset widget`, and a
	// kind with neither has no name menu at all — `Change Widget`, which
	// used to carry that menu on its own, is the `▾`'s since da-z8klv.
	//
	// `tail` gained an option in da-hkjp (its `type`), so the kind standing
	// here for «declares none» is `custom`. The contrast is the point, not
	// which kind makes it.
	test('the options are items of the name’s menu; a kind with none has none', async () => {
		await shared.solo(async ({ page }) => {
			assert.deepEqual(await linesOf(page, 'repl'), ['Reset widget', NEWEST]);
			assert.deepEqual(await linesOf(page, 'nowview'), ['Reset widget', 'history', 'orientation']);
			assert.deepEqual(await linesOf(page, 'bar'), [
				'Reset widget',
				'the buttons stand',
				'a switch is labelled with its',
				'show ignored fields'
			]);
			assert.deepEqual(await linesOf(page, 'strip'), ['Reset widget', 'today frozen at the bottom', 'orientation']);
			assert.deepEqual(await linesOf(page, 'tail'), ['Reset widget', 'type']);
			assert.equal(await menuButton(page, 'custom').count(), 0, 'a kind that declares nothing has a name menu');
			// No modal is left anywhere in the app.
			assert.equal(await page.locator('dialog.options').count(), 0);

			await manage(page, 'custom', 'Change Widget');
			await chooser(page).waitFor();
			await chooser(page).locator('button').filter({ hasText: exactly('command') }).click();
			await chooser(page).waitFor({ state: 'detached' });
			// `command` declares neither options nor defaults, holds nothing
			// in this session and holds no layout, so the NAME's slice is
			// empty and no menu is drawn at all (da-z8klv, which moved
			// Change Widget to the `▾`).
			assert.equal(await menuButton(page, 'custom').count(), 0, 'command has a name menu');
		});
	});

	// A switch says whether it is on in the role the keyboard and a screen
	// reader read it off, and turns over WITHOUT closing the menu — so a
	// second switch is one press away and `Save options with layout`
	// appears in the list the moment there is something to save.
	test('a switch is a menuitemcheckbox that turns over in place, and the save appears beside it', async () => {
		await shared.room('wo-switch', async ({ page }, room) => {
			await menuButton(page, inRoom(room, 'repl')).click();
			assert.equal(await switchItem(page, NEWEST).getAttribute('aria-checked'), 'false');
			assert.equal(await page.getByRole('menuitem', { name: SAVE, exact: true }).count(), 0, 'nothing held, and a save offered');
			await switchItem(page, NEWEST).click();
			await switchItem(page, NEWEST, true).waitFor();
			assert.ok(await page.locator('[role="menu"]').isVisible(), 'the switch closed the menu');
			await page.getByRole('menuitem', { name: SAVE, exact: true }).waitFor();
			await page.keyboard.press('Escape');
			await page.locator('[role="menu"]').waitFor({ state: 'detached' });
		});
	});

	// A choice is a submenu of WORDS, one of them ticked — the `<select>`
	// the modal had, in the menu. The default is named as one.
	test('a choice opens a submenu of its values, the one in force ticked', async () => {
		await shared.solo(async ({ page }) => {
			assert.deepEqual(await choiceLines(page, 'bar', 'a switch is labelled with its'), ['✓ field · default', 'state']);
			const windows = await choiceLines(page, 'nowview', 'history');
			assert.equal(windows.length, 13, windows.join(' | '));
			assert.equal(windows[0], 'now');
			assert.ok(windows.includes('✓ 30m · default'), windows.join(' | '));
		});
	});

	// The pivot (da-a6guc): `vertical` swaps the strip for datum's own
	// `dayview --orientation vertical`, a week asked for at the pane's size.
	// Which way time runs is read off the renderer's own `data-axis` on each
	// day — `y` is time down a column — and not off the picture's shape,
	// which is the pane's: the default strip pane is short, so the week
	// comes back wider than tall. Back to `horizontal` is the strip again.
	for (const [name, options] of [['desktop', DESKTOP], ['phone', PHONE]]) {
		test(`orientation pivots the dayview to columns, and back (${name})`, async () => {
			const pivot = async ({ page }, room) => {
				const strip = room ? inRoom(room, 'strip') : 'strip';
				const days = (axis) => pane(page, strip).locator(`svg.svg g.day-data[data-axis="${axis}"]`);
				await days('x').first().waitFor({ state: 'attached' });
				assert.equal(await days('y').count(), 0, 'the strip drew time down a column');

				await pickOption(page, strip, 'orientation', 'vertical');
				await page.waitForFunction(
					(id) => document.querySelectorAll(`section[data-pane="${id}"] svg.svg g.day-data[data-axis="y"]`).length === 7,
					strip
				);
				assert.equal(await days('x').count(), 0, 'a band is left beside the columns');
				const [drawn, box] = await Promise.all(
					[pane(page, strip).locator('svg.svg'), pane(page, strip)].map((l) => l.evaluate((e) => e.getBoundingClientRect().width))
				);
				assert.ok(drawn <= box, `the week overflows the pane: ${drawn} > ${box}`);
				assert.equal(await dotted(page, room ?? 'main'), false, 'the pivot dirtied the layout');

				await pickOption(page, strip, 'orientation', 'horizontal');
				await days('x').first().waitFor({ state: 'attached' });
				assert.equal(await days('y').count(), 0, 'the columns outlived the pivot back');
			};
			// The phone is the one test here on its screen: an app of its own.
			await (options === DESKTOP ? shared.room('wo-pivot', pivot) : withApp(pivot, options));
		});
	}

	test('repl reversed: newest first, this session’s, saved only when asked, undone by Reset widget', async () => {
		await withApp(async (app) => { // reloads
			const { page, submit } = app;
			for (const line of ['occur coffee', 'occur water', 'occur tea']) await submit(line);
			const oldestFirst = ['occur coffee', 'occur water', 'occur tea'];
			assert.deepEqual(await replEchoes(page), oldestFirst);

			await toggleOption(page, 'repl', NEWEST);
			assert.deepEqual(await replEchoes(page), [...oldestFirst].reverse());
			// ⚑ The change this bead is: the layout never heard of it.
			assert.equal(await dotted(page, 'main'), false, 'a session setting dirtied the layout');
			assert.ok(!(await tree(page)).includes('"reversed":true'), 'a session setting reached the tree');

			// An entry keeps its own order: the command, then what it
			// answered. Inside its own cell since da-5iyo.
			const first = await page
				.locator(`${REPL} .body .entry`)
				.first()
				.evaluate((el) => [...el.children].map((e) => e.className));
			assert.match(first[0], /head/, `the first entry opens with ${first[0]}`);

			// The bar's own log is newest-first WHATEVER this widget's option
			// says (da-ckw3): it is not the widget, and the option is not its.
			const bar = await page.locator('.prompt-bar').boundingBox();
			await page.mouse.move(bar.x + bar.width / 2, bar.y + 10);
			await page.mouse.wheel(0, 120);
			await page.locator('.log .echo').first().waitFor();
			assert.deepEqual(
				(await page.locator('.log .echo').allInnerTexts()).map((t) => t.replace(/^\s*❯\s*/, '').trim()),
				[...oldestFirst].reverse().slice(1)
			);
			await page.keyboard.press('Escape');
			await page.waitForFunction(
				() =>
					parseFloat(getComputedStyle(document.querySelector('.shell')).getPropertyValue('--log-height')) < 0.5
			);

			// A reload starts back at what the LAYOUT says — the session is
			// memory, and nothing was asked of the draft.
			await app.reload();
			assert.deepEqual(await replEchoes(page), oldestFirst, 'a session setting survived a reload');

			// Asked, it becomes the layout's: the dot lights, and a save keeps it.
			await toggleOption(page, 'repl', NEWEST);
			await choose(page, 'repl', SAVE);
			await page.waitForFunction(() => document.querySelector('.root-bar [role="tab"].modified'));
			assert.ok((await tree(page)).includes('"reversed":true'), 'the save did not reach the tree');
			// And the session let go of it, so the menu no longer offers a save.
			assert.deepEqual(await linesOf(page, 'repl'), ['Reset widget', NEWEST]);
			assert.equal(await optionChecked(page, 'repl', NEWEST), true);

			await chooseRoot(page, 'Save layout');
			await page.waitForFunction(() => !document.querySelector('.root-bar [role="tab"].modified'));
			await app.reload();
			assert.deepEqual(await replEchoes(page), [...oldestFirst].reverse(), 'the save lost the option');

			// `Reset widget` is the kind's defaults, and it is a layout edit.
			await choose(page, 'repl', 'Reset widget');
			assert.deepEqual(await replEchoes(page), oldestFirst);
			assert.equal(await optionChecked(page, 'repl', NEWEST), false);
			assert.deepEqual(app.errors, []);
		}, DESKTOP);
	});

	// `Reset widget` means the widget as it STARTS, so what this session
	// set goes with it — a session setting left standing over freshly
	// written defaults is exactly the thing «defaults» did not go back to.
	test('Reset widget drops what this session set as well as what the layout says', async () => {
		await shared.room('wo-reset', async (app, room) => {
			const { page } = app;
			const repl = inRoom(room, 'repl');
			await toggleOption(page, repl, NEWEST);
			await choose(page, repl, SAVE);
			await page.waitForFunction(() => JSON.stringify(window.datum.layout.current()).includes('"reversed":true'));
			await toggleOption(page, repl, NEWEST);
			await choose(page, repl, 'Reset widget');
			assert.equal(await optionChecked(page, repl, NEWEST), false);
			assert.ok(!(await tree(page)).includes('"reversed":true'));
			assert.deepEqual(app.errors, []);
		});
	});

	test('repl reversed pins to the top on every run, as it pins to the bottom otherwise', async () => {
		await shared.room('wo-pin', async (app, room) => {
			const { page, submit } = app;
			await toggleOption(page, inRoom(room, 'repl'), NEWEST);
			const body = page.locator(`${REPL} .body`);
			// Enough to overflow with room to spare, not exactly enough: at
			// 16 the scrollback measured 469px in a 469px box once da-gvcen
			// made a first log ONE document instead of two — the echoes fell
			// from five rows of four columns to four, and the test lost the
			// scrolling it exists to check. The assertion below is the guard
			// that says so, so the count may be raised but never trusted.
			for (let i = 0; i < 24; i++) await submit('occur water');
			const dims = await body.evaluate((b) => [b.scrollHeight, b.clientHeight]);
			assert.ok(dims[0] > dims[1], `the scrollback does not overflow: ${dims}`);
			await body.evaluate((b) => (b.scrollTop = b.scrollHeight));
			await submit('occur coffee');
			await page.waitForFunction((sel) => document.querySelector(sel)?.scrollTop === 0, `${REPL} .body`);
			assert.equal((await replEchoes(page))[0], 'occur coffee');
		});
	});

	// Measured on #33's review: keyed by position, a reversed REPL of 60
	// entries rebuilt all of them for one more command (≈2 500 mutation
	// records against ≈60 oldest-first). One command must cost about the same
	// either way round.
	test('repl reversed: one more command does not rebuild the scrollback', async () => {
		const scrollback = Array.from({ length: 60 }, (_, i) => ({
			line: `occur n${i}`,
			ranAt: Date.now() - (60 - i) * 1000,
			result: { argv: ['occur', `n${i}`], code: 0, stdout: `line ${i}\nanother line ${i}`, stderr: '' }
		}));
		await withApp( // seeds the REPL's storage before boot
			async (app) => {
				const { page, submit } = app;
				/** Mutation records under the REPL's body while `line` runs. */
				const cost = async (line) => {
					await page.evaluate((sel) => {
						window.__records = 0;
						window.__observer = new MutationObserver((rs) => (window.__records += rs.length));
						window.__observer.observe(document.querySelector(sel), {
							subtree: true,
							childList: true,
							characterData: true,
							attributes: true
						});
					}, `${REPL} .body`);
					await submit(line);
					await app.idle();
					return page.evaluate(() => (window.__observer.disconnect(), window.__records));
				};
				const oldestFirst = await cost('occur water');
				await toggleOption(page, 'repl', NEWEST);
				const newestFirst = await cost('occur tea');
				assert.equal((await replEchoes(page))[0], 'occur tea');
				assert.ok(newestFirst <= 2 * oldestFirst + 20, `newest-first ${newestFirst} records, oldest-first ${oldestFirst}`);
			},
			{
				...DESKTOP,
				prepare: (context) =>
					context.addInitScript(
						([key, value]) => localStorage.getItem(key) ?? localStorage.setItem(key, value),
						['datum.repl.v1', JSON.stringify({ scrollback, history: [] })]
					)
			}
		);
	});

	// The menu keeps no copy: a setting the widget moves itself is what the
	// menu says the next time it is opened, and it went to the SESSION, so
	// the layout is as clean as it was.
	test('nowview: the menu asks for the history, and follows the widget’s own +', async () => {
		await shared.room('wo-history', async (app, room) => {
			const { page } = app;
			const nowview = inRoom(room, 'nowview');
			await app.spyOnRunner();
			await pickOption(page, nowview, 'history', '45m');
			await page.waitForFunction(() => {
				const argv = window.__datumSeen.filter((a) => a[0] === 'nowview').at(-1);
				return argv?.[argv.indexOf('--history') + 1] === '45m';
			});
			assert.equal(await dotted(page, room), false, 'a session setting dirtied the layout');

			await page.locator('section.pane[data-title="nowview"] button[aria-label="longer window"]').click();
			await page.waitForFunction(() => {
				const argv = window.__datumSeen.filter((a) => a[0] === 'nowview').at(-1);
				return argv?.[argv.indexOf('--history') + 1] === '60m';
			});
			assert.ok((await choiceLines(page, nowview, 'history')).includes('✓ 60m'));
			assert.equal(await dotted(page, room), false, 'the widget’s own + dirtied the layout');
			assert.ok(!(await tree(page)).includes('"history":"60m"'), 'the widget’s own + reached the tree');
			assert.deepEqual(app.errors, []);
		});
	});

	// ⚠ THE DEFECT THIS BEAD NAMES, driven rather than read: before
	// da-vhwq, pressing nowview's − under a locked layout moved `history`
	// in the tree and lit the dot, because the lock hid the chrome and
	// never defended the data path. It is not a guard that fixes it: the
	// path itself goes to the session now, and the one act that writes the
	// tree is the one the lock takes away.
	test('under a lock: the options still change, the save is gone, and the widget’s own − leaves the tree alone', async () => {
		await shared.room('wo-lock', async (app, room) => {
			const { page } = app;
			await toggleRootLock(page);
			// AFTER the lock: the lock is a field of the tree too (`setLocked`),
			// so the tree to hold everything below to is the LOCKED one.
			const before = await tree(page);

			// The locked menu of a PANE that holds no layout of its own is the
			// options alone — no Reset widget, no save, and no acts, which
			// belong to the tab that HOLDS a layout (da-crp5). Change Widget
			// is not there either: it is the `▾`'s, and the `▾` is hidden.
			const lines = await linesOf(page, inRoom(room, 'repl'));
			assert.deepEqual(lines, [NEWEST]);
			// A kind that declares NO options has no menu left at all under a
			// lock. That kind is `custom`: `tail` gained its `type` in
			// da-hkjp, so it KEEPS its menu — and what it keeps is the option
			// alone, which is the rule this test is about.
			assert.equal(await pane(page, inRoom(room, 'custom')).locator('button.title').count(), 0, 'a kind with no options kept a menu');
			assert.deepEqual(await linesOf(page, inRoom(room, 'tail')), ['type'], 'a kind with options lost its menu under a lock');

			// The option still works, and is still the session's.
			await toggleOption(page, inRoom(room, 'repl'), NEWEST);
			assert.ok(!(await tree(page)).includes('"reversed":true'));
			assert.equal(await dotted(page, room), false, 'a session setting dirtied a LOCKED layout');
			assert.equal(await page.getByRole('menuitem', { name: SAVE, exact: true }).count(), 0);

			// And the widget's own in-body control — the one the lock never
			// hid — leaves the tree byte for byte as it found it.
			await page.locator('section.pane[data-title="nowview"] button[aria-label="shorter window"]').click();
			await page.waitForFunction(
				() => document.querySelectorAll('section.pane[data-title="nowview"] svg').length > 0
			);
			assert.equal(await dotted(page, room), false, 'the widget’s own − dirtied a LOCKED layout');
			assert.equal(await tree(page), before, 'a locked layout was modified');
			assert.deepEqual(app.errors, []);
		});
	});

	// `solo`, not a room: the edit below walks the WHOLE root, and in a room
	// `main`'s repl behind it would be the first one `settings` finds. The
	// save hands the session's value to the tree, which `solo` puts back.
	test('a key this build does not declare survives a save', async () => {
		await shared.solo(async ({ page }) => {
			const settings = () =>
				page.evaluate(() => {
					const find = (p) => p.tabs.flatMap((t) => (t.content.kind === 'repl' ? [t.content] : (t.content.children ?? []).flatMap(find)));
					return find(window.datum.layout.current().root)[0].settings;
				});
			await page.evaluate(() => {
				const walk = (p) => ({
					...p,
					tabs: p.tabs.map((t) => ({
						...t,
						content:
							t.content.kind === 'repl'
								? { ...t.content, settings: { future: 1 } }
								: t.content.children
									? { ...t.content, children: t.content.children.map(walk) }
									: t.content
					}))
				});
				window.datum.layout.apply((l) => ({ ...l, root: walk(l.root) }));
			});
			await toggleOption(page, 'repl', NEWEST);
			await choose(page, 'repl', SAVE);
			await page.waitForFunction(() => JSON.stringify(window.datum.layout.current()).includes('"reversed":true'));
			assert.deepEqual(await settings(), { future: 1, reversed: true });
		});
	});

	// da-xhio. vividn, 2026-09-21: «IGNORED isn't displayed anyway, though there
	// should probably be a toggle for it, let's put it in the options menu for
	// now», and of how long it lasts: «I guess it should just live per session?»
	//
	// The first option that was this session's, and now the rule rather than
	// the exception. What is held to is the whole contract, not just the
	// filter: the button comes back, it is visibly still ignored, Track +
	// still offers it, the layout is not dirtied, «defaults» puts it back,
	// and a reload starts it off again.
	test('fields: «show ignored fields» is this session’s — marked, composing, and never the layout’s', async () => {
		await withApp(async (app) => { // reloads
			const { page } = app;
			const button = page.locator('.bar button[data-field="run"]');
			// Two points, one of them ignored: the mark below says something
			// only because the field beside it does not wear it. And a state
			// field, ignored too, whose tap opens its states rather than runs.
			await app.submit('track walk --verb occur');
			await app.submit('track run --verb occur');
			await app.submit('switch mood good');
			// Drawn before it is taken away, as track.test.mjs is careful to do.
			await button.waitFor();
			// A second on between writes to one field: the clock is held, and
			// each needs an instant of its own beside the one before it.
			await app.clock.forward(1000);
			await app.submit('switch mood meh');
			await app.submit('ignore run');
			await app.clock.forward(1000);
			await app.submit('ignore mood');
			await page.waitForFunction(() => !document.querySelector('.bar [data-field="run"], .bar [data-field="mood"]'));

			await toggleOption(page, 'bar', 'show ignored fields');

			// Back in the bar, and saying what it is: the word in the title, the
			// dashes and the fade in the drawing.
			await button.waitFor();
			assert.equal(
				await button.getAttribute('title'),
				'ignored · press to log · hold to filter the page to it'
			);
			assert.equal(await button.getAttribute('data-ignored'), '');
			const lines = (b) => {
				const s = getComputedStyle(b);
				return { opacity: parseFloat(s.opacity), border: s.borderTopStyle, outline: s.outlineStyle };
			};
			const drawn = await button.evaluate(lines);
			assert.ok(drawn.opacity < 1, `an ignored field drawn at opacity ${drawn.opacity}`);
			// A tap RUNS it, so its dashes are DOUBLE: a dashed border and a
			// dashed outline inside it (da-nmna0).
			assert.deepEqual(
				{ border: drawn.border, outline: drawn.outline },
				{ border: 'dashed', outline: 'dashed' },
				'an ignored point is not drawn in two dashed lines'
			);
			// A state field's tap OPENS its states, so it keeps one dashed line.
			const mood = await page.locator('.bar button[data-field="mood"]').evaluate(lines);
			assert.deepEqual(
				{ border: mood.border, outline: mood.outline },
				{ border: 'dashed', outline: 'none' },
				'an ignored state field is not drawn in one dashed line'
			);
			const tracked = page.locator('.bar button[data-field="walk"]');
			assert.equal(await tracked.getAttribute('data-ignored'), null);
			// A tracked point RUNS on a tap, so it wears the double outline (da-z8z40).
			assert.equal(await tracked.evaluate((b) => getComputedStyle(b).borderTopStyle), 'double');
			assert.equal(await tracked.evaluate((b) => parseFloat(getComputedStyle(b).opacity)), 1);

			// It is a SECOND way to see them, not a replacement for the one that
			// exists: Track + still offers `run` back.
			await page.locator('.bar button.track').click();
			await page.locator('dialog.spec [data-ignored="run"]').waitFor();
			await page.keyboard.press('Escape');
			await page.locator('dialog.spec').waitFor({ state: 'detached' });

			// The layout never hears of it: no dot, and the key is nowhere in
			// the tree the draft is written from.
			assert.equal(await dotted(page, 'main'), false, 'a session setting dirtied the layout');
			assert.equal((await tree(page)).includes('showIgnored'), false, 'a session setting reached the tree');

			// «defaults» means the widget as it starts, session and all.
			await choose(page, 'bar', 'Reset widget');
			await button.waitFor({ state: 'detached' });
			assert.equal(await optionChecked(page, 'bar', 'show ignored fields'), false);

			// And a reload starts it off, however the draft was left.
			await toggleOption(page, 'bar', 'show ignored fields');
			await button.waitFor();
			await app.reload();
			await page.locator('.bar button.track').waitFor();
			assert.equal(await button.count(), 0, 'the toggle survived a reload');
			assert.deepEqual(app.errors, []);
		}, DESKTOP);
	});

	// ⛔ `defaults.ts` mints tab ids from a literal name, so `main` and
	// `mobile` both hold a `strip-tab` — and a session setting keyed by the
	// bare tab id would have been one layout's AND the other's the moment
	// options went session-level. So the key carries the LAYOUT (da-vhwq,
	// `session.ts`).
	//
	// ⚑ In the LIVE tree the two never collide anyway, because `inline`
	// (shell.ts) re-mints an id the tree already holds as `<id>@<name>`.
	// That is a second mechanism and not this one: it is about drawing two
	// trees at once, it depends on the ORDER they were opened in, and a key
	// that leant on it would be a key that changed when it stopped. What is
	// driven here is the property both of them are for.
	test('a setting on one layout’s widget is not another layout’s', async () => {
		await shared.room('wo-apart', async (app, room) => {
			const { page } = app;
			const FROZEN = 'today frozen at the bottom';
			const dayStrip = await titledPane(page, 'dayview').first().getAttribute('data-pane');
			await toggleOption(page, dayStrip, FROZEN);
			assert.equal(await optionChecked(page, dayStrip, FROZEN), false);

			await page.evaluate(() => window.datum.shell.open('mobile'));
			await page.locator('.root-bar [role="tab"][aria-selected="true"]').filter({ hasText: 'mobile' }).waitFor();
			const mobileStrip = await titledPane(page, 'dayview').first().getAttribute('data-pane');
			assert.equal(await optionChecked(page, mobileStrip, FROZEN), true, 'the two layouts shared one tab’s setting');

			// BOTH DIRECTIONS (the panel on #64): main's write not showing on
			// mobile is half the claim. Write on mobile too and hold the two
			// to their own values — a collision the other way round, where the
			// second layout's write lands on the first layout's entry, reads
			// the same as isolation until this is asserted.
			await toggleOption(page, mobileStrip, FROZEN);
			assert.equal(await optionChecked(page, mobileStrip, FROZEN), false);

			// Back on `main`: its own strip still holds what was set on it, so
			// leaving a root tab and coming back keeps this session's — and
			// mobile's write did not reach it.
			await page.locator('.root-bar [role="tab"]').filter({ hasText: exactly(room) }).click();
			await page.locator('section.pane[data-title="repl"]').waitFor();
			assert.equal(await optionChecked(page, dayStrip, FROZEN), false, 'leaving a root tab lost what it held');

			// And once more the other way, with the two now set apart: put
			// main's back to the default and mobile's must not follow.
			await toggleOption(page, dayStrip, FROZEN);
			assert.equal(await optionChecked(page, dayStrip, FROZEN), true);
			await page.locator('.root-bar [role="tab"]').filter({ hasText: exactly('mobile') }).click();
			await titledPane(page, 'dayview').first().waitFor();
			assert.equal(await optionChecked(page, mobileStrip, FROZEN), false, 'main’s write reached mobile');
			assert.deepEqual(app.errors, []);
		});
	});

	// A tab that has gone takes what this session set with it, so a later
	// tab minted with the same literal id does not inherit it.
	test('closing a tab forgets what this session set on it', async () => {
		await shared.room('wo-close', async (app, room) => {
			const { page } = app;
			await toggleOption(page, inRoom(room, 'repl'), NEWEST);
			const held = () => page.evaluate(() => Object.keys(window.datum.layout.session()).length);
			assert.equal(await held(), 1);
			await page.evaluate(() => window.datum.layout.apply((l) => l));
			assert.equal(await held(), 1, 'a change that closed nothing dropped a setting');
			await page.locator('section.pane[data-title="repl"] .titlebar button.manage').click();
			await page.getByRole('menuitem', { name: 'Close pane', exact: true }).click();
			await page.locator('section.pane[data-title="repl"]').waitFor({ state: 'detached' });
			assert.equal(await held(), 0, 'a closed tab kept its session setting');
			assert.deepEqual(app.errors, []);
		});
	});

	test('a phone, 360×640: the menu whole on the screen, rows tappable, Escape closes it and focus goes back', async () => {
		await withApp( // a fresh phone at 360x640
			async ({ page }) => {
				const button = titledMenuButton(page, 'nowview').first();
				await button.click();
				const menu = page.locator('[role="menu"]');
				const box = await menu.boundingBox();
				assert.ok(box.x >= 0 && box.y >= 0 && box.x + box.width <= 360 && box.y + box.height <= 640, JSON.stringify(box));
				for (const item of await menu.locator('> button').all()) {
					assert.ok((await item.boundingBox()).height >= 24, 'an item too small to tap');
				}
				// The submenu of words is on the screen too.
				await page.locator('[role="menu"] > button').filter({ hasText: exactly('history') }).click();
				const list = page.locator('[role="menu"][aria-orientation="vertical"]');
				await list.waitFor();
				const sub = await list.boundingBox();
				assert.ok(sub.x >= 0 && sub.y >= 0 && sub.x + sub.width <= 360 && sub.y + sub.height <= 640, JSON.stringify(sub));
				await page.keyboard.press('Escape');
				await page.keyboard.press('Escape');
				await page.locator('[role="menu"]').waitFor({ state: 'detached' });
				assert.equal(await button.evaluate((b) => b === document.activeElement), true, 'focus did not go back');
			},
			{ device: { viewport: { width: 360, height: 640 }, hasTouch: true, isMobile: true }, home: null }
		);
	});
});
