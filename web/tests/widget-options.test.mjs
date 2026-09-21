// Widget options (da-gvdc): a kind declares its options in the registry,
// and `Options…` in the pane menu opens one modal drawn from them.
//
// vividn, 2026-09-17: «have the REPL be in reverse order (also allow the repl
// widget to be reversed as well as a widget option. (widgets in general
// should support options, a modal that can be opened from the menu.»
//
// What these hold to: the declarations are sound for every kind; the item is
// there exactly for a kind that declares options; the modal keeps no copy of
// the settings — a change is a draft edit of the enclosing layout (its dot,
// Restore, Save) and a change made behind it shows in it; and the first
// customer, the REPL's `reversed`, flips the order of entries and pins the
// view to the top.
//
// And ONE option is not the layout's at all: an option declared `session`
// (da-xhio) is this tab's for as long as the page lives, so it leaves no dot
// and no draft and a reload starts it back at its default. The fields
// widget's «show ignored fields» is the first of them, at the bottom here.

import test, { before, after, describe } from 'node:test';
import assert from 'node:assert/strict';
import { startApp, stopApp, withApp } from './support/app.mjs';
import { DESKTOP, choose, chooseRoot, chooser, exactly, menuButton, menuLines, pane, titledMenuButton } from './support/layout.mjs';

before(startApp);
after(stopApp);

const OPTIONS = 'Options…';
const modal = (page) => page.locator('dialog.options');
const REPL = 'section.pane[data-title="repl"]';

/** Open a pane's menu, read its lines, and close it again. */
async function linesOf(page, id) {
	await menuButton(page, id).click();
	const lines = await menuLines(page);
	await page.keyboard.press('Escape');
	await page.locator('[role="menu"]').waitFor({ state: 'detached' });
	return lines;
}

/** `Options…` on pane `id`, and the modal it opens. */
async function openOptions(page, id) {
	await choose(page, id, OPTIONS);
	await modal(page).waitFor();
}

const closeModal = async (page) => {
	await page.keyboard.press('Escape');
	await modal(page).waitFor({ state: 'detached' });
};

/** The REPL pane's echoed commands, top to bottom. */
const replEchoes = (page) =>
	page.locator(`${REPL} .echo`).allInnerTexts().then((ts) => ts.map((t) => t.replace(/^\s*❯\s*/, '').trim()));

const dotted = async (page, name) =>
	(await page.locator('.root-bar [role="tab"].modified').filter({ hasText: name }).count()) > 0;

/** Set `reversed` on the REPL through the modal, and close it. */
async function setReversed(page, on) {
	await openOptions(page, 'repl');
	await modal(page).locator('[data-option="reversed"] input').setChecked(on);
	await closeModal(page);
}

describe('widget options', () => {
	test('every declared option has a default, and a choice default is one of its choices', async () => {
		await withApp(async ({ page }) => {
			const kinds = await page.evaluate(() => window.datum.layout.kinds());
			const declared = kinds.filter((k) => k.options);
			assert.deepEqual(declared.map((k) => k.kind).sort(), ['daystrip', 'input', 'nowview', 'repl', 'widgetbar']);
			for (const { kind, options, defaultSettings } of declared) {
				for (const o of options) {
					assert.ok(o.key in defaultSettings, `${kind}.${o.key} has no default`);
					const d = defaultSettings[o.key];
					if (o.type === 'boolean') assert.equal(typeof d, 'boolean', `${kind}.${o.key}`);
					else assert.ok(o.choices.some((c) => c.value === d), `${kind}.${o.key}: ${d} is not a choice`);
				}
			}
		}, DESKTOP);
	});

	test('the item: on repl, nowview, fields and dayview; not on tail or command', async () => {
		await withApp(async ({ page }) => {
			for (const id of ['repl', 'nowview', 'bar', 'strip']) {
				const lines = await linesOf(page, id);
				assert.ok(lines.includes(OPTIONS), `${id} reads ${lines.join(' | ')}`);
			}
			assert.ok(!(await linesOf(page, 'tail')).includes(OPTIONS));

			await choose(page, 'custom', 'Choose…');
			await chooser(page).waitFor();
			await chooser(page).locator('button').filter({ hasText: exactly('command') }).click();
			await chooser(page).waitFor({ state: 'detached' });
			const command = await linesOf(page, 'custom');
			assert.ok(!command.includes(OPTIONS), `command reads ${command.join(' | ')}`);
		}, DESKTOP);
	});

	test('repl reversed: newest first, a draft edit, kept by a save, undone by Reset widget and «defaults»', async () => {
		await withApp(async (app) => {
			const { page, submit } = app;
			for (const line of ['occur coffee', 'occur water', 'occur tea']) await submit(line);
			const oldestFirst = ['occur coffee', 'occur water', 'occur tea'];
			assert.deepEqual(await replEchoes(page), oldestFirst);

			await setReversed(page, true);
			assert.deepEqual(await replEchoes(page), [...oldestFirst].reverse());
			assert.equal(await dotted(page, 'day'), true, 'the option left the layout unmodified');
			// An entry keeps its own order: the command, then what it answered.
			const first = await page.locator(`${REPL} .body > *`).evaluateAll((els) => els.map((e) => e.className));
			assert.match(first[0], /head/, `the first entry opens with ${first[0]}`);

			// The bar's own log is newest-first WHATEVER this widget's option
			// says (da-ckw3): it is not the widget, and the option is not its.
			// The widget is reversed here, so what this catches is the log
			// reading the widget's setting rather than its own.
			const bar = await page.locator('.prompt-bar').boundingBox();
			await page.mouse.move(bar.x + bar.width / 2, bar.y + 10);
			await page.mouse.wheel(0, 120);
			await page.locator('.log .echo').first().waitFor();
			// Rows TWO and onward: the bar's output section is row one of the
			// same list (da-ckw3, vividn 2026-09-21), so the newest is up there
			// and not repeated down here. What this still catches is the one
			// it was written for — the log reading the widget's `reversed`
			// instead of its own — because the order below is still newest
			// first while the widget is reversed too.
			assert.deepEqual(
				(await page.locator('.log .echo').allInnerTexts()).map((t) => t.replace(/^\s*❯\s*/, '').trim()),
				[...oldestFirst].reverse().slice(1)
			);
			assert.deepEqual(
				(await page.locator('.prompt-bar .section.output .echo').allInnerTexts()).map((t) =>
					t.replace(/^\s*❯\s*/, '').trim()
				),
				[oldestFirst[oldestFirst.length - 1]]
			);
			await page.keyboard.press('Escape');
			// The LOG's height, not `.pages.scrollTop`: the page never scrolls
			// under the one-number model (da-ckw3), so a scrollTop check here
			// was 0 before Escape and 0 after it, and waited for nothing.
			await page.waitForFunction(
				() =>
					parseFloat(
						getComputedStyle(document.querySelector('.shell')).getPropertyValue('--log-height')
					) < 0.5
			);

			// The draft survives a reload; Restore takes the edit back.
			await app.reload();
			assert.deepEqual(await replEchoes(page), [...oldestFirst].reverse(), 'the draft lost the option');
			await chooseRoot(page, 'Restore layout');
			await page.waitForFunction(() => !document.querySelector('.root-bar [role="tab"].modified'));
			assert.deepEqual(await replEchoes(page), oldestFirst);

			// Saved, it is the layout's.
			await setReversed(page, true);
			await chooseRoot(page, 'Save layout');
			await page.waitForFunction(() => !document.querySelector('.root-bar [role="tab"].modified'));
			await app.reload();
			assert.deepEqual(await replEchoes(page), [...oldestFirst].reverse(), 'the save lost the option');

			await choose(page, 'repl', 'Reset widget');
			assert.deepEqual(await replEchoes(page), oldestFirst);

			await setReversed(page, true);
			await openOptions(page, 'repl');
			await modal(page).getByRole('button', { name: 'defaults' }).click();
			assert.equal(await modal(page).locator('[data-option="reversed"] input').isChecked(), false);
			await closeModal(page);
			assert.deepEqual(await replEchoes(page), oldestFirst);
			assert.deepEqual(app.errors, []);
		}, DESKTOP);
	});

	test('repl reversed pins to the top on every run, as it pins to the bottom otherwise', async () => {
		await withApp(async (app) => {
			const { page, submit } = app;
			await setReversed(page, true);
			const body = page.locator(`${REPL} .body`);
			for (let i = 0; i < 16; i++) await submit('occur water');
			const dims = await body.evaluate((b) => [b.scrollHeight, b.clientHeight]);
			assert.ok(dims[0] > dims[1], `the scrollback does not overflow: ${dims}`);
			await body.evaluate((b) => (b.scrollTop = b.scrollHeight));
			await submit('occur coffee');
			await page.waitForFunction((sel) => document.querySelector(sel)?.scrollTop === 0, `${REPL} .body`);
			assert.equal((await replEchoes(page))[0], 'occur coffee');
		}, DESKTOP);
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
		await withApp(
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
				await setReversed(page, true);
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

	test('nowview: the modal asks for the history, and follows the widget’s own +', async () => {
		await withApp(async (app) => {
			const { page } = app;
			await app.spyOnRunner();
			await openOptions(page, 'nowview');
			const select = modal(page).locator('[data-option="history"] select');
			assert.equal(await select.inputValue(), '30m');
			assert.equal(await select.locator('option[value="0"]').innerText(), 'now');
			assert.match(await select.locator('option[value="30m"]').innerText(), /default/);

			await select.selectOption('45m');
			await page.waitForFunction(() => {
				const argv = window.__datumSeen.filter((a) => a[0] === 'nowview').at(-1);
				return argv?.[argv.indexOf('--history') + 1] === '45m';
			});

			// Behind the modal: the widget's own + (the page under a modal is
			// inert to the pointer, so the button is pressed from script).
			await page.locator('section.pane[data-title="nowview"] button[aria-label="longer window"]').evaluate((b) => b.click());
			await page.waitForFunction(() => document.querySelector('dialog.options select')?.value === '60m');
			assert.deepEqual(app.errors, []);
		}, DESKTOP);
	});

	test('a key this build does not declare survives a change', async () => {
		await withApp(async ({ page }) => {
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
			await setReversed(page, true);
			assert.deepEqual(await settings(), { future: 1, reversed: true });
		}, DESKTOP);
	});

	test('under a lock the menu keeps Options…, as the widget keeps its own controls', async () => {
		await withApp(async ({ page }) => {
			await chooseRoot(page, 'Lock layout');
			assert.deepEqual(await linesOf(page, 'repl'), [OPTIONS]);
			assert.equal(await pane(page, 'tail').locator('button.title').count(), 0, 'a kind with no options kept a menu');
			await setReversed(page, true);
			assert.equal(await page.evaluate(() => JSON.stringify(window.datum.layout.current()).includes('"reversed":true')), true);
		}, DESKTOP);
	});

	// da-xhio. vividn, 2026-09-21: «IGNORED isn't displayed anyway, though there
	// should probably be a toggle for it, let's put it in the options menu for
	// now», and of how long it lasts: «I guess it should just live per session?»
	//
	// So this option is the first of a KIND — declared `session`, it is this
	// tab's for as long as the page lives and the layout never hears of it.
	// What is held to here is that whole contract, not just the filter: the
	// button comes back, it is visibly still ignored, Track + still offers it,
	// the layout is not dirtied, «defaults» puts it back, and a reload starts
	// it off again. da-vhwq moves the row out of this modal; none of the above
	// should change when it does.
	test('fields: «show ignored fields» is this session’s — marked, composing, and never the layout’s', async () => {
		await withApp(async (app) => {
			const { page } = app;
			const button = page.locator('.bar button[data-field="run"]');
			// Two fields, one of them ignored: the mark below says something
			// only because the field beside it does not wear it.
			await app.submit('track walk --verb occur');
			await app.submit('track run --verb occur');
			// Drawn before it is taken away, as track.test.mjs is careful to do:
			// a bar that has not got round to drawing `run` satisfies "no run
			// button" too.
			await button.waitFor();
			await app.submit('ignore run');
			await page.waitForFunction(() => !document.querySelector('.bar [data-field="run"]'));

			await openOptions(page, 'bar');
			const row = modal(page).locator('[data-option="showIgnored"]');
			assert.equal(await row.locator('small').innerText(), 'this session', 'the row claims a default it does not keep');
			await row.locator('input').setChecked(true);
			await closeModal(page);

			// Back in the bar, and saying what it is: the word in the title, the
			// dashes and the fade in the drawing.
			await button.waitFor();
			assert.equal(await button.getAttribute('title'), 'ignored · press to log · hold for when / how long');
			assert.equal(await button.getAttribute('data-ignored'), '');
			const drawn = await button.evaluate((b) => {
				const s = getComputedStyle(b);
				return { opacity: parseFloat(s.opacity), border: s.borderTopStyle };
			});
			assert.ok(drawn.opacity < 1, `an ignored field drawn at opacity ${drawn.opacity}`);
			assert.equal(drawn.border, 'dashed', 'an ignored field drawn with a tracked field’s outline');
			const tracked = page.locator('.bar button[data-field="walk"]');
			assert.equal(await tracked.getAttribute('data-ignored'), null);
			assert.equal(await tracked.evaluate((b) => getComputedStyle(b).borderTopStyle), 'solid');
			assert.equal(await tracked.evaluate((b) => parseFloat(getComputedStyle(b).opacity)), 1);

			// It is a SECOND way to see them, not a replacement for the one that
			// exists: Track + still offers `run` back.
			await page.locator('.bar button.track').click();
			await page.locator('dialog.track [data-ignored="run"]').waitFor();
			await page.keyboard.press('Escape');
			await page.locator('dialog.track').waitFor({ state: 'detached' });

			// The layout never hears of it: no dot, and the key is nowhere in
			// the tree the draft is written from.
			assert.equal(await dotted(page, 'day'), false, 'a session option dirtied the layout');
			assert.equal(
				await page.evaluate(() => JSON.stringify(window.datum.layout.current()).includes('showIgnored')),
				false,
				'a session option reached the tree'
			);

			// «defaults» means the widget as it starts, session and all.
			await openOptions(page, 'bar');
			await modal(page).getByRole('button', { name: 'defaults' }).click();
			assert.equal(await modal(page).locator('[data-option="showIgnored"] input').isChecked(), false);
			await closeModal(page);
			await button.waitFor({ state: 'detached' });

			// And a reload starts it off, however the draft was left.
			await openOptions(page, 'bar');
			await modal(page).locator('[data-option="showIgnored"] input').setChecked(true);
			await closeModal(page);
			await button.waitFor();
			await app.reload();
			await page.locator('.bar button.track').waitFor();
			assert.equal(await button.count(), 0, 'the toggle survived a reload');
			assert.deepEqual(app.errors, []);
		}, DESKTOP);
	});

	test('a phone, 360×640: the modal whole on the screen, rows tappable, Escape and ✕ close it and focus goes back', async () => {
		await withApp(
			async ({ page }) => {
				const button = titledMenuButton(page, 'nowview').first();
				for (const close of [() => page.keyboard.press('Escape'), () => modal(page).getByRole('button', { name: 'close' }).click()]) {
					await button.click();
					await page.getByRole('menuitem', { name: OPTIONS, exact: true }).click();
					await modal(page).waitFor();
					const box = await modal(page).boundingBox();
					assert.ok(box.x >= 0 && box.y >= 0 && box.x + box.width <= 360 && box.y + box.height <= 640, JSON.stringify(box));
					for (const row of await modal(page).locator('.row').all()) {
						assert.ok((await row.boundingBox()).height >= 44, 'a row too small to tap');
					}
					await close();
					await modal(page).waitFor({ state: 'detached' });
					assert.equal(await button.evaluate((b) => b === document.activeElement), true, 'focus did not go back');
				}
			},
			{ device: { viewport: { width: 360, height: 640 }, hasTouch: true, isMobile: true }, home: null }
		);
	});
});
