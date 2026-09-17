// The bottom bar as the app's status line (da-xfa4).
//
// The bar is the one fixed thing on the screen, and da-xfa4 made it say
// what the last command answered as well as take the next one. Everything
// here is measured on the page rather than described: the claim that
// carries the file is the FIRST one — the bar's height is the same number
// after a command that returns a bare entry, one that returns an entry
// with six keys of data, and one that returns a table. A status line that
// grew with its content would reflow the dashboard above it on every entry
// logged, which is the bug this shape exists to make impossible.
//
// A document is TWO lines whatever it carries (da-f4mw): its data stands
// beside them in a brace, up to four rows and then a `+N`. So the rows
// have to fit inside the two lines' own height — measured here, not
// described — and a document with no data has no brace in the DOM at all.
//
// The strings are never asserted against strings written here: a document
// is compared with what the CLI itself printed for the same command
// (`--show json` through the seam), which is the rule the web has about
// datum’s output (docs/web-conventions.md #6).

import test, { before, after, describe } from 'node:test';
import assert from 'node:assert/strict';
import { atEachScreen, startApp, stopApp, withApp } from './support/app.mjs';

before(startApp);
after(stopApp);

/**
 * Both screens open the REPL layout rather than the day page.
 *
 * The bar is drawn by the app's own layout, above whatever tree is in
 * front (`+layout.svelte`), so it is the same bar on every page — and
 * every one of these reads it alone. Opening the day frame for them draws
 * a fortnight of `dayview` bands first, better than a second a test and
 * twenty-odd tests over (da-x6ea).
 *
 * `DAY` is for the one claim that is ABOUT the page behind the bar: that
 * a taller receipt does not reflow the dashboard. That one wants a
 * dashboard.
 */
const DESKTOP = { viewport: { width: 1280, height: 800 }, query: 'repl' };
/** The narrowest screen the bar has to work on (the bead's trap 2). */
const NARROW = { viewport: { width: 320, height: 640 }, query: 'repl' };
const DAY = { viewport: DESKTOP.viewport };

const BAR = '.prompt-bar';
const DOC = `${BAR} .doc`;

/** The bar's height on screen, to the pixel it is drawn at. */
const barHeight = (page) =>
	page.locator(BAR).evaluate((el) => el.getBoundingClientRect().height);

/** What the dashboard above the bar has, which is the thing that must not move. */
const mainHeight = (page) => page.locator('main').evaluate((el) => el.getBoundingClientRect().height);

/** One document section's lines, as the reader sees them. */
const section = (page, i = 0) =>
	page
		.locator(DOC)
		.nth(i)
		.evaluate((el) => ({
			receipt: el.querySelector('.receipt')?.innerText.trim(),
			info: el.querySelector('.info')?.innerText,
			rows: [...el.querySelectorAll('.brace .row')].map((r) => r.innerText),
			more: el.querySelector('.brace .more')?.innerText ?? null,
			hid: el.querySelector('.hid')?.textContent,
			id: el.querySelector('.hid')?.getAttribute('title'),
			size: parseFloat(getComputedStyle(el).fontSize),
			lines: [...el.querySelector('.lines').children].length,
			// The trap the band's fixed height makes: the brace may not be
			// deeper than the two lines it stands beside.
			linesHigh: el.querySelector('.lines').getBoundingClientRect().height,
			braceHigh: el.querySelector('.brace')?.getBoundingClientRect().height ?? 0
		}));

/** Wait until the bar's result area has drawn a document at all. */
const aDoc = (page) => page.locator(DOC).first().waitFor();

/** The CLI's own documents for `argv`, straight off the seam. */
const cliDocs = async (app, argv) => {
	const result = await app.run([...argv, '--show', 'json']);
	assert.equal(result.code, 0, result.stderr);
	return JSON.parse(result.stdout.trim().split('\n').at(-1)).docs;
};

/** The ENTRY's document: a field's declaration (da-c8mh) reads first. */
const cliDoc = async (app, argv) => (await cliDocs(app, argv)).at(-1);

const ESC = /\u001b\[[0-9;]*m/g;

describe('the bar never changes height', () => {
	test('a command with data, one without, and one with no echo at all all leave it where it was', async () => {
		await withApp(async (app) => {
			const { page } = app;
			const was = { bar: await barHeight(page), main: await mainHeight(page) };
			// Not a round number by accident: the height is the bar's own
			// declaration, and what is asserted is that it does not move.
			assert.ok(was.bar > 0);

			for (const line of [
				'occur water',
				'occur coffee beans=ethiopian cups=2 roast=dark',
				'occur brew a=1 b=2 c=3 d=4 e=5 f=6',
				'occur coffee cups=2 , occur tea',
				'tail',
				'occur nope --nonsense'
			]) {
				await app.submit(line);
				await app.idle();
				assert.equal(await barHeight(page), was.bar, `the bar moved after \`${line}\``);
				assert.equal(await mainHeight(page), was.main, `the dashboard reflowed after \`${line}\``);
			}
			assert.deepEqual(app.errors, []);
		}, DAY);
	});
});

describe('one document', () => {
	test('data or none, two lines at the two-line size — and the data beside them in a brace', async () => {
		await withApp(async (app) => {
			const { page } = app;

			// The fields already exist, so each command echoes its own
			// entry alone — a field's first log echoes the TRACK document
			// that declares it too (da-c8mh).
			await app.declare('water');
			await app.declare('coffee');
			await app.submit('occur water');
			await aDoc(page);
			const plain = await section(page);
			const plainDoc = await cliDoc(app, ['get', 'water']);
			assert.equal(plain.lines, 2, 'an entry with no data drew a third line');
			// No data, no brace: not an empty one, not a hidden one.
			assert.equal(await page.locator(`${DOC} .brace`).count(), 0);
			assert.equal(plain.hid, plainDoc.hid);
			assert.equal(plain.info.replace(ESC, ''), plainDoc.info);

			await app.submit('occur coffee cups=2 roast=dark');
			await page.waitForFunction(
				(hid) => document.querySelector('.prompt-bar .doc .hid')?.textContent !== hid,
				plain.hid
			);
			const rich = await section(page);
			const richDoc = await cliDoc(app, ['get', 'coffee']);
			assert.equal(rich.lines, 2, 'an entry with data drew a third line');
			// The CLI's `data` items, one to a row, in the CLI's order — not
			// `data_inline`, which is the terminal's own join.
			assert.deepEqual(rich.rows, richDoc.data);
			assert.equal(rich.rows.length, 2, rich.rows.join(' | '));
			assert.equal(rich.more, null);
			// The lines did not shrink to make room for it, which is the
			// whole of the change.
			assert.equal(rich.size, plain.size, `${rich.size}px beside ${plain.size}px`);
			assert.ok(
				rich.braceHigh <= rich.linesHigh + 1,
				`the brace is ${rich.braceHigh}px against the lines' ${rich.linesHigh}px`
			);
			assert.deepEqual(app.errors, []);
		}, DESKTOP);
	});

	test('past four keys: four rows and a +N, whose sheet lists every one', async () => {
		await withApp(async (app) => {
			const { page } = app;
			await app.declare('brew');
			await app.submit('occur brew a=1 b=2 c=3 d=4 e=5 f=6');
			await page.locator(`${DOC} .brace`).waitFor();

			const shown = await section(page);
			const doc = await cliDoc(app, ['get', 'brew']);
			assert.equal(doc.data.length, 6, doc.data.join(' | '));
			assert.equal(shown.lines, 2);
			assert.deepEqual(shown.rows, doc.data.slice(0, 4));
			assert.equal(shown.more, '+2');
			assert.ok(
				shown.braceHigh <= shown.linesHigh + 1,
				`four rows and a chip are ${shown.braceHigh}px against ${shown.linesHigh}px`
			);

			await page.locator(`${DOC} .brace .more`).click();
			const keys = page.locator('dialog.sheet .key');
			await keys.first().waitFor();
			assert.deepEqual(await keys.allInnerTexts(), doc.data, 'the sheet lost a key');
			assert.deepEqual(app.errors, []);
		}, DESKTOP);
	});

	test('the full id is behind the hid: a title for a pointer, a tap for a finger', async () => {
		await withApp(async (app) => {
			const { page } = app;
			await app.declare('water');
			await app.submit('occur water');
			await aDoc(page);
			const shown = await section(page);
			const doc = await cliDoc(app, ['get', 'water']);

			assert.equal(shown.hid, doc.hid, 'the hid on screen is not the CLI’s');
			assert.equal(shown.id, doc.id, 'the title behind it is not the full id');
			assert.notEqual(doc.hid, doc.id, 'this entry’s hid and id are the same string');

			// Touch has no hover: the tap has to say it too.
			await page.locator(`${DOC} .hid`).click();
			assert.equal(await page.locator(`${DOC} .hid`).textContent(), doc.id);
			await page.locator(`${DOC} .hid`).click();
			assert.equal(await page.locator(`${DOC} .hid`).textContent(), doc.hid);
			assert.deepEqual(app.errors, []);
		}, DESKTOP);
	});
});

describe('several documents', () => {
	test('a two-sided command draws a section each', async () => {
		await withApp(async (app) => {
			const { page } = app;
			await app.declare('coffee');
			await app.declare('mood', 'switch');
			await app.submit('occur coffee , switch mood happy');
			await page.waitForFunction(() => document.querySelectorAll('.prompt-bar .doc').length === 2);

			const [first, second] = [await section(page, 0), await section(page, 1)];
			// Each side echoed its own document: the web appends `--show json`
			// to EVERY side, not only the last.
			assert.match(first.receipt, /^CREATE /);
			assert.match(second.receipt, /^CREATE /);
			assert.notEqual(first.hid, second.hid);
			assert.ok(first.info.includes('coffee'), first.info);
			assert.ok(second.info.includes('happy'), second.info);
			assert.deepEqual(app.errors, []);
		}, DESKTOP);
	});

	test('more than fit draws +N, and the modal lists every one', async () => {
		await withApp(async (app) => {
			const { page } = app;
			const fields = ['a', 'b', 'c', 'd', 'e', 'f', 'g', 'h'];
			for (const f of fields) await app.declare(f);
			await app.submit(fields.map((f) => `occur ${f}`).join(' , '));
			await page.locator(`${BAR} .more`).waitFor();

			const drawn = await page.locator(DOC).count();
			const more = await page.locator(`${BAR} .more`).innerText();
			assert.ok(drawn >= 1, 'not one whole section survived');
			assert.ok(drawn < fields.length, 'every section fitted — nothing to prove');
			// The COUNT, not an ellipsis: how many more there are is the thing
			// worth knowing.
			assert.equal(more, `+${fields.length - drawn}`);

			await page.locator(`${BAR} .more`).click();
			const listed = page.locator('dialog.sheet .doc');
			await listed.first().waitFor();
			assert.equal(await listed.count(), fields.length, 'the modal lost a document');
			assert.deepEqual(
				await page.locator('dialog.sheet .doc .info').allInnerTexts().then((t) =>
					t.map((s) => s.trim().split(/\s+/).at(1)).sort()
				),
				[...fields].sort()
			);
			assert.deepEqual(app.errors, []);
		}, DESKTOP);
	});

	test('320px: one whole section, and the box still takes a command', async () => {
		await withApp(async (app) => {
			const { page } = app;
			for (const f of ['a', 'b', 'c', 'tea']) await app.declare(f);
			await app.submit('occur a , occur b , occur c');
			await page.locator(`${BAR} .more`).waitFor();

			// A whole section at the same size, never half of two.
			assert.equal(await page.locator(DOC).count(), 1);
			const only = await section(page);
			assert.ok(only.info.trim().length > 0, 'the one section on a phone reads as nothing');
			assert.equal(await page.locator(`${BAR} .more`).innerText(), '+2');

			// And the input still works at this width.
			await app.submit('occur tea');
			await page.waitForFunction(() =>
				document.querySelector('.prompt-bar .doc .info')?.textContent.includes('tea')
			);
			assert.deepEqual(app.errors, []);
		}, NARROW);
	});

	test('320px: a section too narrow for the rows draws the count, and the info line is whole', async () => {
		await withApp(async (app) => {
			const { page } = app;
			await app.declare('brew');
			await app.submit('occur brew a=1 bean=ethiopian c=3 grind=fine e=5 water=94C');
			await page.locator(`${DOC} .brace`).waitFor();

			const shown = await section(page);
			// Every key behind the count: four rows of a character or two
			// would not be the data, and the sheet has all of it.
			assert.deepEqual(shown.rows, []);
			assert.equal(shown.more, '+6');

			// And the brace took none of the width the two lines wanted
			// (the bead's trap 2): the info line is not ellipsized at all.
			const cut = await page
				.locator(`${DOC} .info`)
				.evaluate((el) => el.scrollWidth - el.clientWidth);
			assert.ok(cut <= 1, `the info line lost ${cut}px to the brace`);

			await page.locator(`${DOC} .brace .more`).click();
			const keys = page.locator('dialog.sheet .key');
			await keys.first().waitFor();
			assert.equal(await keys.count(), 6);
			assert.deepEqual(app.errors, []);
		}, NARROW);
	});
});

describe('a command with no echo', () => {
	test('a tail shows its own LAST four lines', async () => {
		await withApp(async (app) => {
			const { page } = app;
			for (const f of ['a', 'b', 'c', 'd', 'e', 'f']) assert.equal((await app.run(['occur', f])).code, 0);
			await app.submit('tail');
			await page.locator(`${BAR} .raw p`).first().waitFor();

			const printed = (await app.run(['tail'])).stdout.replace(ESC, '').replace(/\n+$/, '').split('\n');
			assert.ok(printed.length > 4, 'the tail fitted — nothing to prove');
			assert.deepEqual(
				await page.locator(`${BAR} .raw p`).allInnerTexts(),
				printed.slice(-4),
				'the bar showed lines that were not the CLI’s last four'
			);

			// What it cut is one tap away, at the bar's own control — the
			// area itself offers none any more (da-bhvy).
			assert.equal(await page.locator(`${BAR} .expand`).count(), 0);
			assert.deepEqual(app.errors, []);
		}, DESKTOP);
	});

	test('an error is marked, and shown from stderr', async () => {
		await withApp(async (app) => {
			const { page } = app;
			await app.submit('occur');
			await page.locator(`${BAR} .result.error`).waitFor();
			const shown = await page.locator(`${BAR} .raw p`).allInnerTexts();
			const printed = (await app.run(['occur'])).stderr.replace(ESC, '').replace(/\n+$/, '').split('\n');
			assert.ok(shown.length > 0);
			assert.deepEqual(shown, printed.slice(-4));
		}, DESKTOP);
	});

	test('with no document in the stdout, the raw lines still show', async () => {
		await withApp(async (app) => {
			const { page } = app;
			// An older module: one that does not know `--show json` and prints
			// the two lines the default show prints. The web must fall back to
			// them rather than show nothing (the bead's trap 1).
			await page.evaluate(() => {
				const previous = window.datum.setRunner((argv, ...rest) => {
					const bare = [];
					for (let i = 0; i < argv.length; i += 1) {
						if (argv[i] === '--show') i += 1;
						else bare.push(argv[i]);
					}
					return previous(bare, ...rest);
				});
			});

			await app.declare('water');
			await app.submit('occur water');
			await page.locator(`${BAR} .raw p`).first().waitFor();
			assert.equal(await page.locator(DOC).count(), 0, 'a document was drawn from no document');
			const shown = await page.locator(`${BAR} .raw p`).allInnerTexts();
			assert.match(shown[0].replace(ESC, ''), /^CREATE /);
			assert.ok(shown.some((l) => l.includes('water')), shown.join(' | '));
			assert.deepEqual(app.errors, []);
		}, DESKTOP);
	});
});

describe('the ^', () => {
	// One control, at the bar's TOP EDGE, and it toggles (da-bhvy). vividn:
	// "The (tap to expand) should be at the top — it can overlay the top
	// line of the entire input bar. And it should toggle, hitting it again
	// should close the repl." And what it opens stands ON the input row:
	// "the bottom of the repl should actually be on top of the output bar,
	// so that the repl perfectly overlays the output of the last command".
	// So the scrollback's last entry is the very answer the tier was
	// showing — and whole, where the tier had cut it.

	/** Where the control, the two tiers and the open REPL are drawn. */
	const boxes = (page) =>
		page.evaluate(() => {
			const box = (s) => {
				const el = document.querySelector(s);
				if (!el) return null;
				const { top, bottom, left, right, height } = el.getBoundingClientRect();
				return { top, bottom, left, right, height };
			};
			return {
				bar: box('.prompt-bar'),
				control: box('.prompt-bar .caret'),
				tier: box('.prompt-bar .tier.top'),
				prompt: box('.prompt-bar .tier.bottom'),
				expansion: box('.prompt-bar .expansion')
			};
		});

	/** Open the REPL and wait for it. A REPL here is a bare `Pane`. */
	async function open(page) {
		await page.locator(`${BAR} .caret`).click();
		await page.locator(`${BAR} .expansion section.pane`).waitFor();
	}

	test("the control sits on the bar's top line", async (t) => {
		await withApp(async (app) => {
			const { page } = app;
			await atEachScreen(t, page, [{ width: 1280, height: 900 }, { width: 390, height: 640 }], async (viewport) => {
				const size = `${viewport.width}x${viewport.height}`;
				const shut = await boxes(page);
				assert.ok(
					Math.abs(shut.control.top - shut.bar.top) <= 1,
					`the control's head at ${shut.control.top}, the bar's at ${shut.bar.top}`
				);
				// A fingertip, at every pointer (conventions #12).
				assert.ok(shut.control.height >= 24, `the control is ${shut.control.height}px tall`);
				assert.ok(
					shut.control.left >= 0 && shut.control.right <= viewport.width,
					`the control is ${shut.control.left}–${shut.control.right} on ${size}`
				);

				// And it stays where it was: the way out is the way in.
				await open(page);
				const lit = await boxes(page);
				assert.ok(
					Math.abs(lit.control.top - lit.bar.top) <= 1,
					`open, the control's head at ${lit.control.top}, the bar's at ${lit.bar.top}`
				);
				assert.deepEqual(app.errors, []);
				// Shut again, so the next screen presses the same control.
				await page.keyboard.press('Escape');
				await page.locator(`${BAR} .expansion`).waitFor({ state: 'detached' });
			});
		}, DESKTOP);
	});

	test('one press opens the REPL, the next closes it, and so does Escape', async () => {
		await withApp(async (app) => {
			const { page } = app;
			await app.submit('occur coffee');
			const control = page.locator(`${BAR} .caret`);
			const expansion = page.locator(`${BAR} .expansion`);
			assert.equal(await expansion.count(), 0);
			assert.equal(await control.getAttribute('aria-expanded'), 'false');

			await open(page);
			assert.equal(await control.getAttribute('aria-expanded'), 'true');
			// It is the scrollback: the command just typed is in it.
			assert.ok((await expansion.innerText()).includes('occur coffee'));

			await control.click();
			await expansion.waitFor({ state: 'detached' });
			assert.equal(await control.getAttribute('aria-expanded'), 'false');

			await open(page);
			await page.keyboard.press('Escape');
			await expansion.waitFor({ state: 'detached' });
			assert.equal(await control.getAttribute('aria-expanded'), 'false');
			assert.deepEqual(app.errors, []);
		}, DESKTOP);
	});

	test('open, its foot is on the input row and the prompt keeps the focus', async () => {
		await withApp(async (app) => {
			const { page } = app;
			await app.submit('occur coffee');
			await open(page);

			const { bar, tier, prompt, expansion } = await boxes(page);
			assert.ok(
				Math.abs(expansion.bottom - tier.bottom) <= 1,
				`the REPL's foot at ${expansion.bottom}, the result tier's at ${tier.bottom}`
			);
			assert.ok(
				expansion.bottom <= prompt.top + 1,
				`the REPL's foot at ${expansion.bottom} is past the prompt's head at ${prompt.top}`
			);
			// Upward, over the page — not into the bar, which never moves.
			assert.ok(expansion.top < bar.top, `the REPL's head at ${expansion.top}, the bar's at ${bar.top}`);

			assert.ok(
				await page.locator(`${BAR} input`).evaluate((el) => el === document.activeElement),
				'the prompt lost the focus to the control'
			);
			// And the line under it still runs: the new command lands in the
			// REPL, and in the tier it is covering.
			await app.submit('occur tea');
			await page.waitForFunction(() =>
				document.querySelector('.prompt-bar .expansion').innerText.includes('occur tea')
			);
			assert.deepEqual(app.errors, []);
		}, DESKTOP);
	});

	test('a document the tier cut to four rows and a +2 reads whole in the REPL', async () => {
		await withApp(async (app) => {
			const { page } = app;
			await app.declare('brew');
			await app.submit('occur brew a=1 b=2 c=3 d=4 e=5 f=6');
			await page.locator(`${DOC} .brace`).waitFor();

			const cut = await section(page);
			assert.equal(cut.rows.length, 4);
			assert.equal(cut.more, '+2');

			await open(page);
			// The scrollback's LAST entry — the one the tier was showing.
			const doc = page.locator(`${BAR} .expansion .doc`).last();
			const keys = await doc.locator('.brace .row').allInnerTexts();
			assert.deepEqual(keys, cut.rows.concat(['e: 5', 'f: 6']), 'the REPL cut the data too');
			assert.equal(await doc.locator('.brace .more').count(), 0, 'the REPL still counts rows it did not draw');

			// Closed again, the tier shows what it showed before.
			await page.locator(`${BAR} .caret`).click();
			await page.locator(`${BAR} .expansion`).waitFor({ state: 'detached' });
			const again = await section(page);
			assert.deepEqual(again.rows, cut.rows);
			assert.equal(again.more, '+2');
			assert.deepEqual(app.errors, []);
		}, DESKTOP);
	});

	test('a table the tier cut to four lines reads whole in the REPL', async () => {
		await withApp(async (app) => {
			const { page } = app;
			for (const f of ['a', 'b', 'c', 'd', 'e', 'f']) assert.equal((await app.run(['occur', f])).code, 0);
			await app.submit('tail');
			await page.locator(`${BAR} .raw p`).first().waitFor();

			const printed = (await app.run(['tail'])).stdout.replace(ESC, '').replace(/\n+$/, '').split('\n');
			assert.ok(printed.length > 4, 'the tail fitted — nothing to prove');
			assert.deepEqual(await page.locator(`${BAR} .raw p`).allInnerTexts(), printed.slice(-4));

			await open(page);
			const out = page.locator(`${BAR} .expansion pre[data-stream="stdout"]`).last();
			const whole = (await out.innerText())
				.replace(/\n+$/, '')
				.split('\n')
				.map((line) => line.trimEnd());
			assert.deepEqual(whole, printed.map((line) => line.trimEnd()));
			assert.deepEqual(app.errors, []);
		}, DESKTOP);
	});

	// The trap of a lid on the bar's top line: the brand and the clock are
	// on that line too, and a control across all of it would take their
	// taps. (The ⓘ was the brand's neighbour here until da-y992 moved it to
	// the root bar; the brand, which is a menu button now, took its place in
	// this test.)
	test('the brand and the clock keep their taps', async (t) => {
		await withApp(async (app) => {
			const { page } = app;
			await atEachScreen(t, page, [DESKTOP.viewport, NARROW.viewport], async () => {
				const reached = await page.evaluate(() =>
					['.prompt-bar .brand', '.prompt-bar .clock'].map((selector) => {
						const el = document.querySelector(selector);
						const { left, right, top, bottom } = el.getBoundingClientRect();
						const at = document.elementFromPoint((left + right) / 2, (top + bottom) / 2);
						return selector + (el === at || el.contains(at) ? ' reached' : ' covered');
					})
				);
				assert.deepEqual(reached, [
					'.prompt-bar .brand reached',
					'.prompt-bar .clock reached'
				]);

				await page.locator(`${BAR} .brand`).click();
				await page.locator('[role="menu"][aria-label="databases"]').waitFor();
				assert.deepEqual(app.errors, []);
				// Shut again, so the next screen opens it for itself.
				await page.keyboard.press('Escape');
				await page.locator('[role="menu"][aria-label="databases"]').waitFor({ state: 'detached' });
			});
		}, DESKTOP);
	});
});

/** Where a pill's left edge is, and where the `❯` ends. */
const pillAt = (page) =>
	page.evaluate(() => ({
		pill: document.querySelector('.prompt-bar .pill')?.getBoundingClientRect().left,
		caret: document.querySelector('.prompt-bar .line > .prompt').getBoundingClientRect().right
	}));

/** Track `coffee` and get its button onto the widget bar, as ghost.test.mjs does. */
async function withCoffee(app) {
	assert.equal((await app.run(['spec', 'set', 'coffee', '--verb', 'occur'])).code, 0);
	await app.submit('tail');
	await app.idle();
	await app.page.locator('.bar [data-field="coffee"]').waitFor();
}

const setPill = (page, pill) =>
	page.evaluate((p) => window.datum.bar.setPills([p]), { id: 'p', kind: 'time', ...pill });

describe('the pills ride with the text', () => {
	test('a pill sits just after the ❯ when the box is empty, and moves right as the line grows', async () => {
		await withApp(async (app) => {
			const { page } = app;
			await setPill(page, { text: '-t 14:30' });
			await page.locator(`${BAR} .pill`).waitFor();

			const empty = await pillAt(page);
			// "a little bit of space between the > and the pill so that it is
			// easy to click into the box" — and not so much that it reads as
			// belonging to something else.
			assert.ok(
				empty.pill - empty.caret > 4 && empty.pill - empty.caret < 40,
				`the pill is ${empty.pill - empty.caret}px from the ❯`
			);

			const input = page.locator(`${BAR} input`);
			await input.fill('occur coffee beans=ethiopian');
			await page.waitForFunction(
				(was) => document.querySelector('.prompt-bar .pill').getBoundingClientRect().left > was + 20,
				empty.pill
			);

			// It never leaves the box, however long the line gets.
			await input.fill('occur coffee ' + 'x'.repeat(400));
			await app.idle();
			const long = await page.evaluate(() => {
				const pill = document.querySelector('.prompt-bar .pill').getBoundingClientRect();
				const box = document.querySelector('.prompt-bar .field').getBoundingClientRect();
				return { pill: pill.right, box: box.right };
			});
			assert.ok(long.pill <= long.box + 1, `the pill at ${long.pill} is outside the box at ${long.box}`);
			assert.deepEqual(app.errors, []);
		}, DESKTOP);
	});

	test('a press spends its unlocked pill beside the ghost; a locked one stays and comes back left', async () => {
		await withApp(async (app) => {
			const { page } = app;
			await withCoffee(app);
			await setPill(page, { text: '-t 14:30', locked: true });
			await page.locator(`${BAR} .pill`).waitFor();
			const waiting = (await pillAt(page)).pill;

			// A press nobody typed — the widget bar's, which is the press that
			// carries the pills (`pressWithPills`).
			await app.pressWidget('coffee');
			// The ghost is the command ALONE: the pill is drawn as a pill
			// beside it, so saying it in the text too would say it twice.
			assert.equal(await page.locator(`${BAR} .ghost`).innerText(), 'occur coffee');
			await page.waitForFunction(
				(was) => document.querySelector('.prompt-bar .pill').getBoundingClientRect().left > was + 20,
				waiting
			);
			const beside = (await pillAt(page)).pill;

			// The ghost goes; the locked pill stays and returns leftward.
			await page.waitForFunction(() => !document.querySelector('.prompt-bar .ghost'));
			await page.waitForFunction(
				(was) => document.querySelector('.prompt-bar .pill').getBoundingClientRect().left < was - 20,
				beside
			);
			assert.equal(await page.locator(`${BAR} .pill`).count(), 1, 'a locked pill was spent');
			assert.deepEqual(app.errors, []);
			// The day page: `withCoffee` presses the FIELDS widget, which is a
			// pane of the day frame and is not on the repl layout.
		}, DAY);
	});

	test('an unlocked pill leaves the live set at once and fades as a spent one', async () => {
		await withApp(async (app) => {
			const { page } = app;
			await withCoffee(app);
			await setPill(page, { text: '-t 14:30' });
			await page.locator(`${BAR} .pill`).waitFor();

			await app.pressWidget('coffee');
			// Gone from the live pills — so gone from the next command's argv
			// and from the time cursor — and on screen only as a spent one.
			await page.waitForFunction(() => window.datum.bar.pills().length === 0);
			assert.equal(await page.locator(`${BAR} .pill`).count(), 0);
			assert.equal(await page.locator(`${BAR} .spent`).count(), 1);
			await page.waitForFunction(() => !document.querySelector('.prompt-bar .spent'));
			assert.deepEqual(app.errors, []);
			// The day page, as above: the press is the fields widget's.
		}, DAY);
	});
});

/**
 * Wait for the pills to have stopped moving after a line was typed.
 *
 * Two things happen and neither is a promise the test can await: the mirror
 * measures the new text a frame after it is in the box (da-xfa4), and the
 * pills then SLIDE to where it puts them over 120ms — or, on a screen too
 * narrow for them, do not move at all. So the condition is the one that
 * covers both: the mirror says the typed line, and nothing has moved for a
 * quarter of a second.
 */
const settled = async (page, selector, typed) => {
	await page.evaluate(() => delete window.__settle);
	await page.waitForFunction(
		([s, text]) => {
			if (document.querySelector('.prompt-bar .mirror').textContent !== text) return false;
			const left = document.querySelector(s).getBoundingClientRect().left;
			const seen = window.__settle;
			if (!seen || Math.abs(seen.left - left) > 0.5) {
				window.__settle = { left, since: performance.now() };
				return false;
			}
			return performance.now() - seen.since > 250;
		},
		[selector, typed]
	);
};

/** Every live pill on the bar, as `setPills` would have them. */
const setPills = (page, texts) =>
	page.evaluate(
		(all) => window.datum.bar.setPills(all.map((text, i) => ({ id: `p${i}`, kind: 'time', text }))),
		texts
	);

/**
 * What the reader can SEE of the first live pill — read off the page, and
 * the kind's colour resolved through a probe rather than written here, so
 * the colour stays in the one place the component keeps it.
 */
const pillLook = (page) =>
	page.evaluate(() => {
		const pill = document.querySelector('.prompt-bar .pill');
		const style = getComputedStyle(pill);
		const probe = document.createElement('span');
		probe.style.color = style.getPropertyValue('--pill');
		document.body.append(probe);
		const kind = getComputedStyle(probe).color;
		probe.remove();
		const buttons = [...pill.querySelectorAll('button')];
		return {
			background: style.backgroundColor,
			kind,
			size: style.fontSize,
			inputSize: getComputedStyle(document.querySelector('.prompt-bar input')).fontSize,
			text: pill.textContent,
			labels: buttons.map((b) => b.getAttribute('aria-label')),
			lockIsSvg: !!pill.querySelector('button.lock > svg'),
			// The × is a MARK inside the remove button, never a target of its own.
			xIsOwnButton: buttons.some((b) => b.textContent.trim() === '×'),
			nested: buttons.some((b) => b.querySelector('button')),
			height: pill.getBoundingClientRect().height,
			ring: style.boxShadow
		};
	});

// vividn, on the deployed build (da-tnv3): "We need to make the pills much
// more prominent, they are almost invisible right now. Solid background
// color, click anywhere except lock to dismiss… increase their font size
// to be the same as the input bar… Let's use a flat lock icon, mono
// colour." Every claim below is measured on the page: the fill is compared
// with the kind's own colour and the size with the input's own size, so
// neither number is written twice.
describe('the pills are prominent', () => {
	test('solid in its kind’s colour, at the input’s size, with an SVG lock and no × of its own', async () => {
		await withApp(async (app) => {
			const { page } = app;
			await setPill(page, { text: '-t 14:30' });
			await page.locator(`${BAR} .pill`).waitFor();

			const look = await pillLook(page);
			assert.notEqual(look.background, 'rgba(0, 0, 0, 0)', 'the pill is not filled');
			assert.equal(look.background, look.kind, `filled ${look.background}, its kind is ${look.kind}`);
			assert.equal(look.size, look.inputSize, `the pill at ${look.size}, the input at ${look.inputSize}`);

			// No emoji anywhere in what the pill says, and the lock is drawn.
			assert.equal(look.text.replace('×', ''), '-t 14:30');
			assert.ok(look.lockIsSvg, 'the lock is not an svg');
			assert.ok(!look.xIsOwnButton, 'the × is a button of its own');
			assert.ok(!look.nested, 'a button inside a button');
			assert.deepEqual(look.labels, ['remove -t 14:30', 'lock -t 14:30']);
			assert.deepEqual(app.errors, []);
		}, DESKTOP);
	});

	test('a press anywhere but the lock removes it — text, ×, padding and Enter alike', async () => {
		await withApp(async (app) => {
			const { page } = app;
			const seen = await app.spyOnRunner();
			const live = () => page.evaluate(() => window.datum.bar.pills().length);

			// The lock is the one part that is not the remove button: it flips
			// `aria-pressed`, marks the pill, and the pill stays.
			await setPill(page, { text: '-t 14:30' });
			await page.locator(`${BAR} .pill button.lock`).click();
			assert.equal(
				await page.locator(`${BAR} .pill button.lock`).getAttribute('aria-pressed'),
				'true'
			);
			assert.equal(await live(), 1, 'the lock removed the pill');
			const locked = await pillLook(page);
			assert.notEqual(locked.ring, 'none', 'a locked pill reads no differently');

			// A locked pill still carries into the argv a submitted line builds.
			await app.submit('tail');
			assert.ok(
				(await seen()).some((argv) => argv.join(' ').includes('-t 14:30')),
				'the pill never reached the argv'
			);

			// Everywhere else on it is one press: the text, the × beside it,
			// the padding under the pointer, and Enter on the focused button.
			for (const target of ['.pill .label', '.pill .x', '.pill button.remove']) {
				await setPill(page, { text: '-t 14:30' });
				await page.locator(`${BAR} ${target}`).waitFor();
				await page.locator(`${BAR} ${target}`).click();
				await page.waitForFunction(() => window.datum.bar.pills().length === 0);
				assert.equal(await page.locator(`${BAR} .pill`).count(), 0, `${target} did not remove it`);
			}
			await setPill(page, { text: '-t 14:30' });
			await page.locator(`${BAR} .pill button.remove`).press('Enter');
			await page.waitForFunction(() => window.datum.bar.pills().length === 0);

			// And gone from the argv with it.
			const before = (await seen()).length;
			await app.submit('tail');
			assert.ok(
				!(await seen())
					.slice(before)
					.some((argv) => argv.join(' ').includes('-t 14:30')),
				'a removed pill still reached the argv'
			);
			assert.deepEqual(app.errors, []);
		}, DESKTOP);
	});

	test('three pills do not grow the bar', async (t) => {
		await withApp(async (app) => {
			const { page } = app;
			await atEachScreen(t, page, [{ width: 1280, height: 900 }, { width: 390, height: 640 }], async () => {
				const bare = await barHeight(page);
				await setPills(page, ['-t 14:30', '-d yesterday', '30m']);
				await page.locator(`${BAR} .pill`).first().waitFor();
				await app.idle();
				assert.equal(await barHeight(page), bare, 'the pills grew the bar');

				// The trap the larger font makes (da-xfa4's mirror): the pills
				// still ride AFTER what is typed, not over it — or, when the
				// line has outgrown the box, stand at its right edge, which is
				// the clamp Prompt.svelte documents.
				await page.locator(`${BAR} input`).fill('occur coffee');
				await settled(page, `${BAR} .pills`, 'occur coffee');
				const after = await page.evaluate(() => {
					const at = (s) => document.querySelector(`.prompt-bar ${s}`).getBoundingClientRect();
					const pills = at('.pills');
					return {
						pill: at('.pill').left,
						caret: at('.line > .prompt').right,
						text: at('.mirror').right,
						pillsRight: pills.right,
						box: at('.field').right
					};
				});
				assert.ok(after.pill > after.caret, `the pill at ${after.pill}, the ❯ ends at ${after.caret}`);
				assert.ok(
					after.pill >= after.text - 1 || after.pillsRight >= after.box - 1,
					`the pill at ${after.pill} is over the typed text, which ends at ${after.text}`
				);
				assert.equal(await barHeight(page), bare, 'the pills grew the bar');
				assert.deepEqual(app.errors, []);
				// Cleared, so the next screen starts from a bare bar.
				await setPills(page, []);
				await page.locator(`${BAR} input`).fill('');
				await page.locator(`${BAR} .pill`).waitFor({ state: 'detached' });
			});
		}, DESKTOP);
	});
});

describe('the clock', () => {
	test('is under the logo, on the input row’s line', async () => {
		await withApp(async (app) => {
			const { page } = app;
			const boxes = await page.evaluate(() => {
				const rect = (s) => {
					const r = document.querySelector(s).getBoundingClientRect();
					return { top: r.top, bottom: r.bottom, mid: r.top + r.height / 2, text: '' };
				};
				return {
					brand: rect('.prompt-bar .brand'),
					stamp: rect('.prompt-bar .stamp'),
					input: rect('.prompt-bar input'),
					clock: document.querySelector('.prompt-bar .stamp').textContent
				};
			});
			assert.match(boxes.clock, /^\d\d:\d\d:\d\d$/, `the clock reads ${boxes.clock}`);
			assert.ok(boxes.stamp.top >= boxes.brand.bottom - 1, 'the clock is not under the logo');
			// On the prompt's line: the input row's 40% is the clock's band too.
			assert.ok(
				Math.abs(boxes.stamp.mid - boxes.input.mid) < 12,
				`the clock's middle at ${boxes.stamp.mid}, the prompt's at ${boxes.input.mid}`
			);
			assert.deepEqual(app.errors, []);
		}, DESKTOP);
	});
});
