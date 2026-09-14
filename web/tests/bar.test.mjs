// The bottom bar as the app's status line (da-xfa4).
//
// The bar is the one fixed thing on the screen, and da-xfa4 made it say
// what the last command answered as well as take the next one. Everything
// here is measured on the page rather than described: the claim that
// carries the file is the FIRST one — the bar's height is the same number
// after a command that returns two lines, a command that returns three,
// and a command that returns a table. A status line that grew with its
// content would reflow the dashboard above it on every entry logged, which
// is the bug this shape exists to make impossible.
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
			data: el.querySelector('.data')?.innerText ?? null,
			hid: el.querySelector('.hid')?.textContent,
			id: el.querySelector('.hid')?.getAttribute('title'),
			size: parseFloat(getComputedStyle(el).fontSize),
			lines: [...el.children].length
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
	test('no data is two lines, data is three and smaller — and both are the CLI’s own strings', async () => {
		await withApp(async (app) => {
			const { page } = app;

			await app.submit('occur water');
			await aDoc(page);
			const plain = await section(page);
			const [plainDoc] = await cliDocs(app, ['get', 'water']);
			assert.equal(plain.lines, 2, 'an entry with no data drew a third line');
			assert.equal(plain.data, null);
			assert.equal(plain.hid, plainDoc.hid);
			assert.equal(plain.info.replace(ESC, ''), plainDoc.info);

			await app.submit('occur coffee beans=ethiopian cups=2');
			await page.waitForFunction(
				(hid) => document.querySelector('.prompt-bar .doc .hid')?.textContent !== hid,
				plain.hid
			);
			const rich = await section(page);
			const [richDoc] = await cliDocs(app, ['get', 'coffee']);
			assert.equal(rich.lines, 3, 'an entry with data drew no data line');
			assert.equal(rich.data, richDoc.data_inline);
			assert.ok(
				rich.size < plain.size,
				`three lines at ${rich.size}px is not smaller than two at ${plain.size}px`
			);
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
});

describe('a command with no echo', () => {
	test('a tail shows its own LAST four lines, with an expander that opens the REPL', async () => {
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

			// One expansion, two entry points.
			await page.locator(`${BAR} .expand`).click();
			// A REPL opened here is a bare `Pane` — a `<section class="pane">`
			// with its own h2 — not a pane of the layout tree, which is what
			// carries `data-title`.
			await page.locator(`${BAR} .expansion section.pane`).waitFor();
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
	test('opens the REPL upward and closes it again', async () => {
		await withApp(async (app) => {
			const { page } = app;
			await app.submit('occur coffee');
			assert.equal(await page.locator(`${BAR} .expansion`).count(), 0);

			await page.locator(`${BAR} .caret`).click();
			const repl = page.locator(`${BAR} .expansion section.pane`);
			await repl.waitFor();
			// It opens UPWARD: over the page, not into the bar.
			const [bar, panel] = await Promise.all([
				page.locator(BAR).evaluate((el) => el.getBoundingClientRect().top),
				page.locator(`${BAR} .expansion`).evaluate((el) => el.getBoundingClientRect().bottom)
			]);
			assert.ok(Math.abs(panel - bar) <= 1, `the panel's foot at ${panel}, the bar's head at ${bar}`);
			// And it is the scrollback: the command just typed is in it.
			assert.ok((await repl.innerText()).includes('occur coffee'));

			await page.locator(`${BAR} .caret`).click();
			assert.equal(await page.locator(`${BAR} .expansion`).count(), 0);
			assert.deepEqual(app.errors, []);
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
