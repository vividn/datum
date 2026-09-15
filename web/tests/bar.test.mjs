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
import { startApp, stopApp, withApp } from './support/app.mjs';

before(startApp);
after(stopApp);

const DESKTOP = { viewport: { width: 1280, height: 800 } };
/** The narrowest screen the bar has to work on (the bead's trap 2). */
const NARROW = { viewport: { width: 320, height: 640 } };

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
		}, DESKTOP);
	});
});

describe('one document', () => {
	test('data or none, two lines at the two-line size — and the data beside them in a brace', async () => {
		await withApp(async (app) => {
			const { page } = app;

			await app.submit('occur water');
			await aDoc(page);
			const plain = await section(page);
			const [plainDoc] = await cliDocs(app, ['get', 'water']);
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
			const [richDoc] = await cliDocs(app, ['get', 'coffee']);
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
			await app.submit('occur brew a=1 b=2 c=3 d=4 e=5 f=6');
			await page.locator(`${DOC} .brace`).waitFor();

			const shown = await section(page);
			const [doc] = await cliDocs(app, ['get', 'brew']);
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
			await app.submit('occur water');
			await aDoc(page);
			const shown = await section(page);
			const [doc] = await cliDocs(app, ['get', 'water']);

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

	for (const viewport of [{ width: 1280, height: 900 }, { width: 390, height: 640 }]) {
		const size = `${viewport.width}x${viewport.height}`;

		test(`the control sits on the bar's top line at ${size}`, async () => {
			await withApp(async (app) => {
				const { page } = app;
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
			}, { viewport });
		});
	}

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

	// The trap of a lid on the bar's top line: the brand's ⓘ and the clock
	// are on that line too, and a control across all of it would take their
	// taps.
	for (const { viewport, size } of [
		{ viewport: DESKTOP, size: '1280x800' },
		{ viewport: NARROW, size: '320x640' }
	]) {
		test(`the ⓘ and the clock keep their taps at ${size}`, async () => {
			await withApp(async (app) => {
				const { page } = app;
				const reached = await page.evaluate(() =>
					['.prompt-bar .hint-button', '.prompt-bar .clock'].map((selector) => {
						const el = document.querySelector(selector);
						const { left, right, top, bottom } = el.getBoundingClientRect();
						const at = document.elementFromPoint((left + right) / 2, (top + bottom) / 2);
						return selector + (el === at || el.contains(at) ? ' reached' : ' covered');
					})
				);
				assert.deepEqual(reached, [
					'.prompt-bar .hint-button reached',
					'.prompt-bar .clock reached'
				]);

				await page.locator(`${BAR} .hint-button`).click();
				await page.locator(`${BAR} .beta-hint`).waitFor();
				assert.deepEqual(app.errors, []);
			}, viewport);
		});
	}
});

/** Where a pill's left edge is, and where the `❯` ends. */
const pillAt = (page) =>
	page.evaluate(() => ({
		pill: document.querySelector('.prompt-bar .pill')?.getBoundingClientRect().left,
		caret: document.querySelector('.prompt-bar .line > .prompt').getBoundingClientRect().right
	}));

/** Track `coffee` and get its button onto the widget bar, as ghost.test.mjs does. */
async function withCoffee(app) {
	assert.equal((await app.run(['spec', 'set', 'coffee', '--default-verb', 'occur'])).code, 0);
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
		}, DESKTOP);
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
