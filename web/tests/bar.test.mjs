// The bottom bar as the app's status line (da-xfa4), reshaped into the seam
// (da-ckw3).
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
// da-ckw3 gave it its anatomy: INPUT over OUTPUT, each the height the
// result tier alone used to have, the input row holding the mark, the
// clock WITH SECONDS at every width, the `❯` and the box — and a caret
// that folds the output away rather than opening the CLI widget upward. What the
// bar opens onto now is the log UNDER it, which is `tests/barlog.test.mjs`.
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
//
// One of 3 files split from bar.test.mjs by topic, so each stays well inside
// test.mjs's 300 s file cap (da-5rpcr); the helpers they share are
// support/bar.mjs.

import test, { before, after, describe } from 'node:test';
import assert from 'node:assert/strict';
import { atEachScreen, sharedApp, startApp, stopApp, todayAt, withApp } from './support/app.mjs';
import { fold as foldBar, unfold as openBar, DESKTOP, PHONE, DAY, BAR, OUT, DOC, repl, barHeight, section, cliDoc } from './support/bar.mjs';
import { withBar } from './support/layout.mjs';

before(startApp);
after(stopApp);

/** The narrowest screen the bar has to work on (the bead's trap 2). */
const NARROW = { viewport: { width: 320, height: 640 }, prepare: withBar };

const desktop = repl(sharedApp(DESKTOP));

const narrow = repl(sharedApp(NARROW));

const day = sharedApp(DAY);

/** What the dashboard above the bar has, which is the thing that must not move. */
const mainHeight = (page) => page.locator('main').evaluate((el) => el.getBoundingClientRect().height);

/**
 * The OUTPUT section showing, whatever this screen's default is (da-ckw3).
 *
 * A phone-width screen starts FOLDED, so every claim about what the output
 * says has to unfold it first — and the fold is the reader's, so it is done
 * here the way a reader does it: a drag. There is no button to press any
 * more (`support/bar.mjs` has the why).
 */
const unfold = openBar;

/** Wait until the bar's result area has drawn a document at all. */
const aDoc = (page) => page.locator(DOC).first().waitFor();

const ESC = /\u001b\[[0-9;]*m/g;

describe('the bar never changes height', () => {
	test('a command with data, one without, and one with no echo at all all leave it where it was', async () => {
		await day.solo(async (app) => {
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
		});
	});
});

describe('one document', () => {
	test('data or none, the field and state over the time at one size, and the data under them on one line', async () => {
		await desktop(async (app) => {
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
			// No data, no rows: not an empty one, not a hidden one.
			assert.equal(await page.locator(`${DOC} .rows`).count(), 0);
			assert.equal(plain.hid, plainDoc.hid);
			// The line the CLI printed, cut where it says its time ends.
			assert.equal(plain.what, plainDoc.info.slice(plainDoc.info_time.length).trimStart());
			assert.equal(plain.when, plainDoc.info_time);

			await app.submit('occur coffee cups=2 roast=dark');
			await page.waitForFunction(
				(hid) => document.querySelector('.prompt-bar .group:last-child .doc .hid')?.textContent !== hid,
				plain.hid
			);
			const rich = await section(page);
			const richDoc = await cliDoc(app, ['get', 'coffee']);
			assert.equal(rich.lines, 2, 'an entry with data drew a third line of its info');
			// The band's one line of data is the CLI's own join (da-hxjrk):
			// the log draws every key one to a row (below).
			assert.deepEqual(rich.rows, [richDoc.data_inline]);
			// The lines did not shrink to make room for it.
			assert.equal(rich.size, plain.size, `${rich.size}px beside ${plain.size}px`);
			assert.deepEqual(app.errors, []);
		});
	});

	// da-te4id: `track` and `ignore` answered in raw lines, which take the whole
	// section; as entries they stand as wide as a `switch` does, and no taller.
	test('track and ignore draw as entries, as wide as a switch, with the section its height', async () => {
		await desktop(async (app) => {
			const { page } = app;
			const box = (selector) =>
				page.locator(selector).first().evaluate((el) => {
					const { width, height } = el.getBoundingClientRect();
					return { width, height };
				});
			const answer = async (line, action) => {
				await app.clock.forward(1000);
				await app.submit(line);
				await page.locator(`${DOC} .action[title="${action}"]`).waitFor();
				return { group: await box(OUT), section: await box(`${BAR} .section.output`) };
			};
			const switched = await answer('switch mood happy', 'CHANGE');
			assert.ok(switched.group.width < switched.section.width / 2, 'a switch took the section: nothing to compare with');
			for (const [line, action] of [
				['ignore mood', 'IGNORE'],
				['track mood', 'TRACK▨'],
				['ignore tea', 'IGNORE'],
				['track tea --verb occur', 'TRACK●']
			]) {
				const drawn = await answer(line, action);
				assert.equal(drawn.section.height, switched.section.height, `${line}: the section moved`);
				assert.ok(drawn.group.width < drawn.section.width, `${line} took the whole section`);
				// Within a character: a line a little over the group's floor widens it.
				if (line !== 'track tea --verb occur') assert.ok(Math.abs(drawn.group.width - switched.group.width) < 8, `${line} is not an entry's width: ${drawn.group.width} against ${switched.group.width}`);
			}
			assert.deepEqual(app.errors, []);
		});
	});

	test('past four keys: the band cuts them on one line, the log has every one, one to a row', async () => {
		await desktop(async (app) => {
			const { page } = app;
			await app.declare('brew');
			await app.submit('occur brew a=1 b=2 c=3 d=4 e=5 f=6');
			await page.locator(`${DOC} .rows`).waitFor();

			const doc = await cliDoc(app, ['get', 'brew']);
			assert.equal(doc.data.length, 6, doc.data.join(' | '));
			assert.deepEqual((await section(page)).rows, [doc.data_inline]);
			const logged = page.locator(`.cli .doc[data-hid="${doc.hid}"] .rows .row`);
			await logged.first().waitFor();
			assert.deepEqual(await logged.allInnerTexts(), doc.data, 'the log lost a key');
			assert.deepEqual(app.errors, []);
		});
	});

	test('the full id is behind the hid: a title for a pointer, a tap for a finger', async () => {
		await desktop(async (app) => {
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
		});
	});
});

describe('several documents', () => {
	test('a two-sided command draws a section each', async () => {
		await desktop(async (app) => {
			const { page } = app;
			await app.declare('coffee');
			await app.declare('mood', 'switch');
			await app.submit('occur coffee , switch mood happy');
			await page.waitForFunction(() => document.querySelectorAll('.prompt-bar .group:last-child .doc').length === 2);

			const [first, second] = [await section(page, 0), await section(page, 1)];
			// Each side echoed its own document: `DATUM_SHOW` reaches EVERY
			// side, not only the last.
			assert.equal(first.did, 'MOMENT');
			assert.equal(second.did, 'CHANGE');
			assert.notEqual(first.hid, second.hid);
			assert.ok(first.info.includes('coffee'), first.info);
			assert.ok(second.info.includes('happy'), second.info);
			assert.deepEqual(app.errors, []);
		});
	});

	test('an array of fields draws a section each', async () => {
		await desktop(async (app) => {
			const { page } = app;
			await app.submit('moment coffee,tea x=1');
			await page.waitForFunction(() => document.querySelectorAll('.prompt-bar .group:last-child .doc').length === 2);

			const [first, second] = [await section(page, 0), await section(page, 1)];
			assert.equal(first.did, 'MOMENT');
			assert.equal(second.did, 'MOMENT');
			assert.ok(first.info.includes('coffee'), first.info);
			assert.ok(second.info.includes('tea'), second.info);
			assert.equal(first.when, second.when);
			assert.deepEqual(app.errors, []);
		});
	});

	test('more than fit draws +N, and the modal lists every one', async () => {
		await desktop(async (app) => {
			const { page } = app;
			const fields = ['a', 'b', 'c', 'd', 'e', 'f', 'g', 'h'];
			for (const f of fields) await app.declare(f);
			await app.submit(fields.map((f) => `occur ${f}`).join(' , '));
			await page.locator(`${OUT} .more`).waitFor();

			const drawn = await page.locator(DOC).count();
			const more = await page.locator(`${OUT} .more`).innerText();
			assert.ok(drawn >= 1, 'not one whole section survived');
			assert.ok(drawn < fields.length, 'every section fitted — nothing to prove');
			// The COUNT, not an ellipsis: how many more there are is the thing
			// worth knowing.
			assert.equal(more, `+${fields.length - drawn}`);

			await page.locator(`${OUT} .more`).click();
			const listed = page.locator('dialog.sheet .doc');
			await listed.first().waitFor();
			assert.equal(await listed.count(), fields.length, 'the modal lost a document');
			assert.deepEqual(
				await page.locator('dialog.sheet .doc .what').allInnerTexts().then((t) =>
					t.map((s) => s.trim().split(/\s+/).at(0)).sort()
				),
				[...fields].sort()
			);
			assert.deepEqual(app.errors, []);
		});
	});

	test('320px: one whole section, and the box still takes a command', async () => {
		await narrow(async (app) => {
			const { page } = app;
			await unfold(page);
			for (const f of ['a', 'b', 'c', 'tea']) await app.declare(f);
			await app.submit('occur a , occur b , occur c');
			await page.locator(`${OUT} .more`).waitFor();

			// A whole section at the same size, never half of two.
			assert.equal(await page.locator(DOC).count(), 1);
			const only = await section(page);
			assert.ok(only.info.trim().length > 0, 'the one section on a phone reads as nothing');
			assert.equal(await page.locator(`${OUT} .more`).innerText(), '+2');

			// And the input still works at this width.
			await app.submit('occur tea');
			await page.waitForFunction(() =>
				document.querySelector('.prompt-bar .group:last-child .doc .info')?.textContent.includes('tea')
			);
			assert.deepEqual(app.errors, []);
		});
	});
});

describe('a command with no echo', () => {
	// The LAST lines, as many as the section shows: it is a share of the
	// page now (da-mwp4m), so it holds four or more, and the lines it cannot
	// hold are cut from the top, where the oldest rows of a table are.
	test('a tail shows its own LAST lines, as many as fit', async () => {
		await desktop(async (app) => {
			const { page } = app;
			for (const f of 'abcdefghijkl') assert.equal((await app.run(['occur', f])).code, 0);
			await app.submit('tail');
			await page.locator(`${OUT} .raw p`).first().waitFor();

			const printed = (await app.run(['tail'])).stdout.replace(ESC, '').replace(/\n+$/, '').split('\n');
			// What is drawn whole inside the box that clips the lines, not
			// what is in the DOM.
			const visible = await page.locator(`${OUT} .raw`).evaluate((raw) => {
				const box = raw.getBoundingClientRect();
				return [...raw.querySelectorAll('p')]
					.filter((p) => p.getBoundingClientRect().top >= box.top - 0.5 && p.getBoundingClientRect().bottom <= box.bottom + 0.5)
					.map((p) => p.innerText);
			});
			assert.ok(visible.length >= 4, `the section showed ${visible.length} lines`);
			assert.ok(printed.length > visible.length, 'the tail fitted — nothing to prove');
			assert.deepEqual(visible, printed.slice(-visible.length), 'the bar showed lines that were not the CLI’s last');

			// What it cut is one tap away, at the bar's own control — the
			// area itself offers none any more (da-bhvy).
			assert.equal(await page.locator(`${BAR} .expand`).count(), 0);
			assert.deepEqual(app.errors, []);
		});
	});

	// Each raw line against the box that clips it (da-3d1ro): its line boxes
	// up and down, its text across, wherever it wrapped. Not its text up and
	// down: the font's content area stands past a line of this line-height.
	const inBox = (raw) => {
		const box = raw.getBoundingClientRect();
		const range = document.createRange();
		return [...raw.querySelectorAll('p')].map((p) => {
			range.selectNodeContents(p);
			const rects = [...range.getClientRects()];
			const { top, bottom } = p.getBoundingClientRect();
			return {
				text: p.textContent,
				lines: new Set(rects.map((r) => Math.round(r.top))).size,
				start: top >= box.top - 0.5,
				end: bottom <= box.bottom + 0.5,
				across: rects.every((r) => r.left >= box.left - 0.5 && r.right <= box.right + 0.5)
			};
		});
	};

	test('a line too long for a phone wraps and is whole, in the frame it had (da-3d1ro)', async () => {
		await withApp(async (app) => {
			const { page } = app;
			for (const f of 'abcdefghijkl') assert.equal((await app.run(['occur', f])).code, 0);
			const height = await barHeight(page);
			await app.submit('check');
			await page.locator(`${OUT} .raw p`).first().waitFor();

			// What that run printed: a second `check` says something else.
			const { result } = await page.evaluate(() => window.datum.bar.last());
			const [line, ...more] = await page.locator(`${OUT} .raw`).evaluate(inBox);
			assert.deepEqual(more, []);
			assert.equal(line.text, result.stdout.replace(ESC, '').trim());
			assert.ok(line.lines >= 2, `${line.text.length} characters took ${line.lines} line(s): nothing to prove`);
			assert.ok(line.start && line.end && line.across, `a piece of the line lies outside its box: ${JSON.stringify(line)}`);
			assert.equal(await barHeight(page), height);
			assert.deepEqual(app.errors, []);
		}, PHONE);
	});

	test('one line that does not fit even wrapped keeps its start, in the frame it had (da-3d1ro)', async () => {
		// The iPhone 12's viewport, Safari's bars taken off: `check` is a line short.
		await withApp(async (app) => {
			const { page } = app;
			const height = await barHeight(page);
			await app.submit('check');
			await page.locator(`${OUT} .raw p`).first().waitFor();

			const [line] = await page.locator(`${OUT} .raw`).evaluate(inBox);
			assert.ok(!line.end, 'the line fitted: nothing to prove');
			assert.ok(line.start, 'the verdict at the start of the line was cut');
			assert.equal(await barHeight(page), height);
			assert.deepEqual(app.errors, []);
		}, { ...PHONE, viewport: { width: 390, height: 664 } });
	});

	test('lines that do not fit even wrapped are cut from the top, in the frame they had (da-3d1ro)', async () => {
		await withApp(async (app) => {
			const { page } = app;
			for (const f of 'abcdefghijkl') assert.equal((await app.run(['occur', f])).code, 0);
			const height = await barHeight(page);
			await app.submit('tail');
			await page.locator(`${OUT} .raw p`).first().waitFor();

			const lines = await page.locator(`${OUT} .raw`).evaluate(inBox);
			assert.ok(!lines[0].start, 'the tail fitted: nothing to prove');
			assert.ok(lines.at(-1).start && lines.at(-1).end, 'the newest row is not whole');
			assert.equal(await barHeight(page), height);
			assert.deepEqual(app.errors, []);
		}, PHONE);
	});

	test('an error is marked, and shown from stderr', async () => {
		await desktop(async (app) => {
			const { page } = app;
			await app.submit('occur');
			// `.raw.error` — the error mark went with the raw lines when they
			// moved into `RawLines.svelte`, which both fixed-height surfaces
			// now share (da-ckw3: the bar's row IS the history's first row).
			await page.locator(`${OUT} .raw.error`).waitFor();
			const shown = await page.locator(`${OUT} .raw p`).allInnerTexts();
			const printed = (await app.run(['occur'])).stderr.replace(ESC, '').replace(/\n+$/, '').split('\n');
			assert.ok(shown.length > 0);
			assert.deepEqual(shown, printed.slice(-4));
		});
	});

	test('a run that wrote and then failed shows its documents AND its error, and drops the pill of the hid it deleted (da-rrttb)', async () => {
		await desktop(async (app) => {
			const { page } = app;
			await app.declare('tea');
			const { hid } = await cliDoc(app, ['occur', 'tea']);
			const height = await barHeight(page);
			// The hid pill holding a good hid and a bad one: `del <good>,<bad>`
			// deletes the good one, echoes its DELETE and exits 1 (da-0uvyr).
			await page.evaluate((text) => window.datum.bar.setPills([{ id: 'hid', kind: 'hid', text }]), `${hid},nope9`);
			await page.locator(`${BAR} .pill.hid`).waitFor();
			await app.submit('del');
			await page.locator(`${OUT} .raw.error`).waitFor();

			// What the run itself said: exit 1, and the error's last line.
			const { result } = await page.evaluate(() => window.datum.bar.last());
			assert.equal(result.code, 1);
			const said = result.stderr.replace(ESC, '').trim().split('\n').at(-1);
			assert.match(said, /nope9/, 'the error names the hid it did not find');
			// What the command did is the label's title, and its word (da-nafql).
			assert.deepEqual(await page.locator(`${DOC} .action`).evaluateAll((els) => els.map((el) => el.title)), ['DELETE']);
			assert.equal((await section(page)).hid, hid);
			assert.deepEqual(await page.locator(`${OUT} .raw.error p`).allInnerTexts(), [said]);
			// Inside the output section, and the bar did not grow to hold it:
			// the row is the section's height less the 1px rule under it.
			const fit = await page.locator(`${BAR} .section.output`).evaluate((out) => {
				const box = out.getBoundingClientRect();
				const rule = parseFloat(getComputedStyle(out).borderBottomWidth);
				const [head, line] = ['.group:last-child .head', '.group:last-child .raw.error p'].map((s) => out.querySelector(s).getBoundingClientRect());
				return { head: head.top - box.top, line: box.bottom - rule - line.bottom };
			});
			assert.ok(fit.head >= 0 && fit.line >= -0.5, `the entry is cut out of the output section: ${JSON.stringify(fit)}`);
			assert.equal(await barHeight(page), height);

			// The deleted hid left the pill; the one that was never there stays.
			await page.waitForFunction(() => window.datum.bar.pills().find((p) => p.kind === 'hid')?.text === 'nope9');
			assert.deepEqual(app.errors, []);
		});
	});

	test('with no document in the stdout, the raw lines still show', async () => {
		await withApp(async (app) => { // its own: it installs a runner
			const { page } = app;
			// An older module: one that does not know `DATUM_SHOW` and prints
			// the two lines the default show prints. The web must fall back to
			// them rather than show nothing (the bead's trap 1).
			await page.evaluate(() => {
				const previous = window.datum.setRunner((argv, stdin, onStderr, key, stage) =>
					previous(argv, stdin, onStderr, key, stage)
				);
			});

			await app.declare('water');
			await app.submit('occur water');
			await page.locator(`${OUT} .raw p`).first().waitFor();
			assert.equal(await page.locator(DOC).count(), 0, 'a document was drawn from no document');
			const shown = await page.locator(`${OUT} .raw p`).allInnerTexts();
			assert.match(shown[0].replace(ESC, ''), /^MOMENT /);
			assert.ok(shown.some((l) => l.includes('water')), shown.join(' | '));
			assert.deepEqual(app.errors, []);
		}, DESKTOP);
	});
});

describe('the anatomy: OUTPUT over INPUT', () => {
	// vividn, 2026-09-17: "Put dbname, clock, > and input all on the input
	// section. The output section is reserved just for output." And,
	// 2026-10-27 (da-mwp4m): «keep the input bar on the bottom, and put the
	// output/return bar above it again … take up a bit more room».

	/** The boxes the bar is made of, and the two heights it declares. */
	const parts = (page) =>
		page.evaluate(() => {
			const box = (s) => {
				const el = document.querySelector(s);
				if (!el) return null;
				const { top, bottom, left, right, height, width } = el.getBoundingClientRect();
				return { top, bottom, left, right, height, width };
			};
			// The declared heights, in the pixels they resolve to: a custom
			// property computes to the token it was written as, and only
			// something drawn in it says how tall that is. In `.pages`, the
			// size container the bar's own `cqh` resolves against.
			const px = (name) => {
				const probe = document.createElement('div');
				// Out of the flow, or `.pages`, already full, shrinks it to 0.
				probe.style.cssText = `position: absolute; height: var(${name})`;
				document.querySelector('.pages').append(probe);
				const drawn = probe.getBoundingClientRect().height;
				probe.remove();
				return drawn;
			};
			return {
				bar: box('.prompt-bar'),
				input: box('.prompt-bar .section.input'),
				output: box('.prompt-bar .section.output'),
				mark: box('.prompt-bar .brand'),
				clock: box('.prompt-bar .section.input .stamp'),
				caret: box('.prompt-bar .line > .prompt'),
				field: box('.prompt-bar input'),
				reads: document.querySelector('.prompt-bar .section.input .stamp').textContent,
				// The prompt line's own rule, which this row overrides: read
				// COMPUTED, because the override and the rule it overrides are
				// the same specificity until one of them says `form` (#46).
				rule: getComputedStyle(document.querySelector('.prompt-bar form.line')).borderTopWidth,
				declared: { input: px('--input-height'), output: px('--output-height') }
			};
		});

	// They were equal — vividn's own "so 50/50" — until he looked at it built:
	// "decrease height of input bar so that there is not so much wasted space
	// with padding" (2026-09-18). So the claim is no longer that they match,
	// it is that each is the height the shell declares for it and the input
	// is the SHORTER of the two, which is the change he asked for.
	test('the output section is above the input, the input on the foot of the page, each its declared height, and the input the shorter', async (t) => {
		await desktop(async (app) => {
			const { page } = app;
			await atEachScreen(t, page, [{ width: 1280, height: 900 }, { width: 390, height: 664 }], async () => {
				const p = await parts(page);
				assert.ok(p.output, 'the output section is not there to measure');
				assert.ok(
					p.output.bottom <= p.input.top + 1,
					`the output ends at ${p.output.bottom}, the input starts at ${p.input.top}`
				);
				assert.ok(Math.abs(p.input.bottom - p.bar.bottom) <= 1, `the input ends at ${p.input.bottom}, the bar at ${p.bar.bottom}`);
				assert.ok(
					p.declared.input < p.declared.output,
					`the input row is ${p.declared.input}px against the output's ${p.declared.output}px`
				);
				assert.ok(
					Math.abs(p.input.height - p.declared.input) <= 1,
					`the input row is ${p.input.height}px against a declared ${p.declared.input}px`
				);
				assert.ok(
					Math.abs(p.output.height - p.declared.output) <= 1,
					`the output is ${p.output.height}px against a declared ${p.declared.output}px`
				);
				// Output only, and the whole width of the bar: nothing stands
				// beside it any more.
				assert.ok(
					Math.abs(p.output.left - p.bar.left) <= 1 && Math.abs(p.output.right - p.bar.right) <= 1,
					`the output is ${p.output.left}–${p.output.right} in a bar of ${p.bar.left}–${p.bar.right}`
				);
			});
			assert.deepEqual(app.errors, []);
		});
	});

	test('the input row is the mark, the clock with seconds, the ❯ and the box — in that order, at every width', async (t) => {
		await desktop(async (app) => {
			const { page } = app;
			await atEachScreen(t, page, [{ width: 1280, height: 900 }, { width: 390, height: 664 }, { width: 360, height: 640 }], async (viewport) => {
				const p = await parts(page);
				// vividn: "On mobile too please show seconds on the clock."
				assert.match(p.reads, /^\d\d:\d\d:\d\d$/, `the clock reads ${p.reads} at ${viewport.width}px`);
				// A phone stacks the mark over the clock (da-ovdtt): one column,
				// then the `❯`.
				const stacked = viewport.width <= 480;
				const head = {
					left: Math.min(p.mark.left, p.clock.left),
					right: Math.max(p.mark.right, p.clock.right),
					top: Math.min(p.mark.top, p.clock.top),
					bottom: Math.max(p.mark.bottom, p.clock.bottom)
				};
				if (stacked)
					assert.ok(p.mark.bottom <= p.clock.top + 1, `the mark ends at ${p.mark.bottom}, the clock starts at ${p.clock.top}`);
				const order = [
					...(stacked ? [['the mark over the clock', head]] : [['the mark', p.mark], ['the clock', p.clock]]),
					['the ❯', p.caret],
					['the box', p.field]
				];
				for (let i = 1; i < order.length; i += 1)
					assert.ok(
						order[i - 1][1].right <= order[i][1].left + 1,
						`${order[i - 1][0]} ends at ${order[i - 1][1].right}, ${order[i][0]} starts at ${order[i][1].left}`
					);
				// One row: every one of them is inside the input section and
				// none of them is in the output.
				for (const [what, box] of order)
					assert.ok(
						box.top >= p.input.top - 1 && box.bottom <= p.input.bottom + 1,
						`${what} is ${box.top}–${box.bottom}, the input row ${p.input.top}–${p.input.bottom}`
					);
				// And the box is still a box you can type a command into.
				assert.ok(
					p.field.width >= 150,
					`the box is ${p.field.width.toFixed(0)}px wide at ${viewport.width}px`
				);
				// Nothing stands above this row, so nothing rules it off: the
				// line's own border drew a hairline from the `❯` to the right
				// edge until the override was made to win (vremya on #46).
				assert.equal(p.rule, '0px', `the prompt line still rules itself off with ${p.rule}`);
			});
			assert.deepEqual(app.errors, []);
		});
	});
});

describe('the arrow folds the output, and a drag does too', () => {
	// vividn: "The ^ and down carret now should toggle between just the input
	// section being shown and the input over output section." And, on the
	// default: "Height on phones, yes on mobile start collapsed on desktop
	// start open."
	//
	// The `^` was a BUTTON for two rounds. vividn, 2026-09-18: "The ^ button
	// obscures the text. Let's just get rid of this button and instead just
	// put a little carrat above the input bar to suggest that it can be
	// pulled up." So what these tests press is the GESTURE, and what the
	// chevron has to be is a mark that takes no pointer at all.

	const output = (page) => page.locator(`${BAR} .section.output`);

	// Every screen starts open now (da-mwp4m). vividn, 2026-10-27: «make the
	// output section open by default, always above the input bar»: welcome,
	// where a first visit lands, too (da-qu9j8).
	test('a fresh phone starts open, and the fold it chooses survives a reload', async () => {
		await withApp(async (app) => {
			const { page } = app;
			await output(page).waitFor();

			await foldBar(page);
			await app.reload();
			await page.locator(`${BAR} input:not([disabled])`).waitFor();
			assert.equal(await output(page).count(), 0, 'the reload forgot the fold');
			await openBar(page);
			assert.deepEqual(app.errors, []);
		}, { ...PHONE, home: null });
	});

	test('a fresh desktop starts open, and folds', async () => {
		await withApp(async (app) => {
			const { page } = app;
			await output(page).waitFor();
			await foldBar(page);
			await app.reload();
			await page.locator(`${BAR} input:not([disabled])`).waitFor();
			assert.equal(await output(page).count(), 0, 'the reload forgot the fold');
			assert.deepEqual(app.errors, []);
		}, { viewport: { width: 1280, height: 900 }, home: null });
	});

	test('the dashboard reflows when a person folds, and never when a command answers', async () => {
		await withApp(async (app) => { // its own: the fold is remembered by the device
			const { page } = app;
			const open = { bar: await barHeight(page), main: await mainHeight(page) };

			await foldBar(page);
			const folded = { bar: await barHeight(page), main: await mainHeight(page) };
			assert.ok(folded.bar < open.bar, `folded the bar is ${folded.bar}px, open ${open.bar}px`);
			assert.ok(
				folded.main > open.main,
				`folded the dashboard is ${folded.main}px, open ${open.main}px — it gained nothing`
			);
			// What it gained is exactly what the output section was.
			assert.ok(
				Math.abs(folded.main - open.main - (open.bar - folded.bar)) <= 1,
				`the dashboard gained ${folded.main - open.main}px for a bar that lost ${open.bar - folded.bar}px`
			);

			// And now a command, folded and open alike, moves neither.
			for (const line of ['occur water', 'occur brew a=1 b=2 c=3 d=4 e=5 f=6', 'tail']) {
				await app.submit(line);
				await app.idle();
				assert.equal(await barHeight(page), folded.bar, `folded, the bar moved after \`${line}\``);
				assert.equal(await mainHeight(page), folded.main, `folded, the dashboard reflowed after \`${line}\``);
			}
			assert.deepEqual(app.errors, []);
		}, DAY);
	});

	// The trap a lid on the bar's top line used to be: the brand and the
	// clock are on that line too, and a control across all of it would take
	// their taps. There is no lid any more — the hint takes no pointer — so
	// what this now holds is that nothing ELSE has taken their place. (The ⓘ
	// was the brand's neighbour here until da-y992 moved it to the root bar;
	// the brand, which is a menu button now, took its place in this test.)
	test('the brand and the clock keep their taps', async (t) => {
		await desktop(async (app) => {
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
		});
	});
});

describe('folded, a command still answers', () => {
	// vividn: "When just the input section is shown, show all the ACTION words
	// on the right side after a command is run"; "ACTION words should fade,
	// but on a longer timescale than the input text, maybe over 15 seconds
	// or something. Maybe FAILED should take even longer?"; and on the
	// failure: "red Error that opens on tap is enough I think, as long as
	// the ERROR is prominent enough".

	/**
	 * The app with a clock the test holds, page and CLI together — and the
	 * bar folded, as this device chose it: every screen starts open
	 * (da-mwp4m), and these are about what a folded bar says.
	 */
	const held = (at) => ({
		...PHONE,
		clock: at,
		holdTimers: true,
		prepare: async (context) => {
			await withBar(context);
			await context.addInitScript(() => localStorage.setItem('datum.bar.output.v1', 'folded'));
		}
	});

	const marks = (page) => page.locator(`${BAR} .reopen .action`);
	const LINE_OF_TWO = 'update alfa n=1 , switch mood happy';

	/** What is left to type in, to the pixel it is drawn at. */
	const boxWidth = (page) =>
		page.locator(`${BAR} input`).evaluate((el) => el.getBoundingClientRect().width);

	// vividn, 2026-10-30: «let's put it on the far right, and have it
	// protruding into the swipe space. make it wide and tall enough for the
	// ACTION verb to appear in there, and put the ACTION verb there after
	// entering a command, that invites the user to pull it open to see»
	// (da-k5kwu).
	test('the ACTION word stands in the tab in the CLI’s own color, and fades', async () => {
		const AT = todayAt(12, 0);
		await withApp(async (app) => {
			const { page } = app;
			// A held clock does not settle by itself: the pane's own box
			// arrives through a timer (tick.test.mjs).
			await page.clock.runFor(200);
			await app.idle();
			assert.equal(await page.locator(`${BAR} .section.output`).count(), 0, 'the bar was not folded');
			assert.equal(await marks(page).count(), 0, 'a word with nothing logged');

			await app.submit('occur coffee');
			await marks(page).waitFor();
			// The word the same run's document draws, in its color.
			const drawn = page.locator('.cli .doc .action').last();
			assert.equal(await marks(page).innerText(), await drawn.innerText());
			const painted = await marks(page).evaluate((el) => getComputedStyle(el).color);
			assert.equal(painted, await drawn.evaluate((el) => getComputedStyle(el).color));
			assert.equal(painted, 'rgb(38, 162, 105)', 'not a new entry green (#26a269)');

			// Driven, not slept: fifteen seconds is not a wait a suite takes.
			await app.clock.forward(16_000);
			await marks(page).waitFor({ state: 'detached' });

			// A `tail` echoes no document, so there is nothing to say for it.
			await app.submit('tail');
			await app.idle();
			await page.clock.runFor(100);
			assert.equal(await marks(page).count(), 0, 'a reader saw a word for a command with no document');
			assert.deepEqual(app.errors, []);
		}, held(AT));
	});

	test('a line of two says UPD,CH in two colors, with the box unmoved at 360px, and a failure in a line says the failure', async () => {
		const AT = todayAt(12, 0);
		await withApp(async (app) => {
			const { page } = app;
			await page.clock.runFor(200);
			await app.idle();

			await app.submit('occur alfa');
			await marks(page).waitFor();
			const one = await boxWidth(page);
			await app.submit(LINE_OF_TWO);
			await page.waitForFunction(() => document.querySelector('.prompt-bar .reopen .said')?.textContent.trim() === 'UPD,CH');
			assert.ok(Math.abs((await boxWidth(page)) - one) <= 0.5, 'the word moved the box');
			const colors = await marks(page).evaluateAll((els) => els.map((el) => getComputedStyle(el).color));
			assert.equal(colors.length, 2);
			assert.notEqual(colors[0], colors[1], 'the two actions are one color');

			const fit = await page.evaluate(() => {
				const tab = document.querySelector('.prompt-bar .reopen').getBoundingClientRect();
				const range = document.createRange();
				range.selectNodeContents(document.querySelector('.prompt-bar .reopen .said'));
				const ink = range.getBoundingClientRect();
				return { tab: [tab.left, tab.top, tab.right, tab.bottom], ink: [ink.left, ink.top, ink.right, ink.bottom] };
			});
			assert.ok(
				fit.ink[0] >= fit.tab[0] && fit.ink[1] >= fit.tab[1] && fit.ink[2] <= fit.tab[2] && fit.ink[3] <= fit.tab[3],
				`the word ${fit.ink} is out of the tab ${fit.tab}`
			);
			// «needs very small margins» (vividn): six characters and little else.
			assert.ok(fit.tab[2] - fit.tab[0] - (fit.ink[2] - fit.ink[0]) <= 9, `margins of ${fit.tab[2] - fit.tab[0] - (fit.ink[2] - fit.ink[0])}px beside the word`);

			await app.submit('occur bravo , occur nope --nonsense');
			await page.waitForFunction(() => document.querySelector('.prompt-bar .reopen .said')?.textContent.trim() === 'ERROR!');
			assert.deepEqual(app.errors, []);
		}, { ...held(AT), viewport: { width: 360, height: 640 } });
	});

	test('a failure says ERROR!, in red, for longer — and a tap opens the output on what went wrong', async () => {
		const AT = todayAt(12, 0);
		await withApp(async (app) => {
			const { page } = app;
			await page.clock.runFor(200);
			await app.idle();

			await app.submit('occur nope --nonsense');
			await page.locator(`${BAR} .failed`).waitFor();
			const failed = page.locator(`${BAR} .failed`);
			assert.equal((await failed.innerText()).trim(), 'ERROR!');
			assert.equal(
				await failed.locator('.chip').evaluate((el) => getComputedStyle(el).backgroundColor),
				await page.evaluate(() => {
					const probe = document.createElement('span');
					probe.style.backgroundColor = 'var(--error)';
					document.body.append(probe);
					const painted = getComputedStyle(probe).backgroundColor;
					probe.remove();
					return painted;
				}),
				"the ERROR is not the page's own error colour"
			);

			// Past the moment an ACTION word would have gone, and still there.
			await app.clock.forward(16_000);
			assert.equal(await failed.count(), 1, 'the ERROR went with the ACTION words');
			// It does NOT open by itself: vividn chose the word over an unfold.
			assert.equal(await page.locator(`${BAR} .section.output`).count(), 0);

			await failed.click();
			// The opening is a settle on animation frames, which run on the held clock.
			await page.clock.runFor(1_000);
			await page.locator(`${BAR} .section.output`).waitFor();
			const shown = (await page.locator(`${OUT} .raw`).innerText()).replace(ESC, '');
			const printed = (await app.run(['occur', 'nope', '--nonsense'])).stderr.replace(ESC, '');
			assert.ok(
				printed.split('\n').filter(Boolean).some((line) => shown.includes(line.trim().slice(0, 30))),
				`the output says ${JSON.stringify(shown)} for ${JSON.stringify(printed)}`
			);
			assert.deepEqual(app.errors, []);
		}, held(AT));
	});

	test('a refused re-log exits 11, «not an error», and says EXISTS in its colour — a collision still says ERROR (da-97w7z)', async () => {
		const AT = todayAt(12, 0);
		await withApp(async (app) => {
			const { page } = app;
			await page.clock.runFor(200);
			await app.idle();

			/** What the folded bar says once `line` has answered: ERROR, or its words. */
			const said = async (line) => {
				await app.submit(line);
				await app.idle();
				return page.evaluate(() => {
					const failed = document.querySelector('.prompt-bar .failed');
					if (failed) return { failed: failed.innerText.trim() };
					return [...document.querySelectorAll('.prompt-bar .reopen .action')].map((el) => ({
						text: el.innerText,
						color: getComputedStyle(el).color
					}));
				});
			};

			// A field resolves a collision by updating unless it refuses (da-wudrc).
			await said('occur coffee --conflict refuse');
			assert.equal((await app.run(['occur', 'coffee', '--conflict', 'refuse'])).code, 11, 'the second log is not a re-log');
			const relog = await said('occur coffee --conflict refuse');
			// The colour is the one the same run's scrollback document drew.
			const drawn = page.locator('.cli .doc .action[title="EXISTS"]').first();
			assert.deepEqual(relog, [{ text: 'EXISTS', color: await drawn.evaluate((el) => getComputedStyle(el).color) }]);

			// The same exit with nothing echoed is a slug collision: a failure.
			assert.deepEqual(await said('occur coffee n=2 --conflict refuse'), { failed: 'ERROR!' });
			assert.deepEqual(app.errors, []);
		}, held(AT));
	});
});
