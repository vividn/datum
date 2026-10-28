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

import test, { before, after, describe } from 'node:test';
import assert from 'node:assert/strict';
import { devices } from 'playwright';
import { atEachScreen, sharedApp, startApp, stopApp, todayAt, withApp, seedLines } from './support/app.mjs';
import { fold as foldBar, unfold as openBar } from './support/bar.mjs';
import { withBar } from './support/layout.mjs';

before(startApp);
after(stopApp);

/**
 * Both screens open `CLI_BESIDE`, the CLI widget over a note, rather than the day page.
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
const DESKTOP = { viewport: { width: 1280, height: 800 }, prepare: withBar };
/** The narrowest screen the bar has to work on (the bead's trap 2). */
const NARROW = { viewport: { width: 320, height: 640 }, prepare: withBar };
/** A phone, which is where this is judged (da-ckw3). */
const PHONE = { viewport: { width: 390, height: 844 }, device: { isMobile: true, hasTouch: true }, prepare: withBar };
const DAY = { viewport: DESKTOP.viewport };

const BAR = '.prompt-bar';
/** The newest command's group in the output strip (da-qua5g): what a command just answered. */
const OUT = `${BAR} .group:last-child`;
const DOC = `${OUT} .doc`;

// ONE APP per screen for most of this file (da-6b3dj, sharedApp in
// support/app.mjs): the database is emptied between tests and every claim
// reads the bar's own last answer. The repl-page apps are tidied by hand
// rather than by `solo`, which would close the repl tab and draw `main`
// behind the bar: a sheet left open, a half-typed line, the page and the
// pills. What still opens an app of its own is a fresh device (`home:
// null`), a held clock, a runner a test installs, a fold (which the device
// remembers), and a screen only one test asks for.
const repl = (shared) => async (fn) => {
	const app = await shared();
	await app.page.evaluate(() => document.querySelectorAll('dialog[open]').forEach((d) => d.close()));
	await app.page.locator(`${BAR} input`).fill('');
	await app.home();
	await app.empty();
	return fn(app);
};
const desktop = repl(sharedApp(DESKTOP));
const narrow = repl(sharedApp(NARROW));
const day = sharedApp(DAY);

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
			// The line the CLI printed, whatever lines the strip cut it into (da-7xp82).
			info: el.querySelector('.info')?.textContent,
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
	test('data or none, two lines at the two-line size — and the data beside them in a brace', async () => {
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
			// No data, no brace: not an empty one, not a hidden one.
			assert.equal(await page.locator(`${DOC} .brace`).count(), 0);
			assert.equal(plain.hid, plainDoc.hid);
			assert.equal(plain.info.replace(ESC, ''), plainDoc.info);

			await app.submit('occur coffee cups=2 roast=dark');
			await page.waitForFunction(
				(hid) => document.querySelector('.prompt-bar .group:last-child .doc .hid')?.textContent !== hid,
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
		});
	});

	test('past four keys: four rows and a +N, whose sheet lists every one', async () => {
		await desktop(async (app) => {
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
			assert.match(first.receipt, /^CREATE /);
			assert.match(second.receipt, /^CREATE /);
			assert.notEqual(first.hid, second.hid);
			assert.ok(first.info.includes('coffee'), first.info);
			assert.ok(second.info.includes('happy'), second.info);
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
				await page.locator('dialog.sheet .doc .info').allInnerTexts().then((t) =>
					t.map((s) => s.trim().split(/\s+/).at(1)).sort()
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

	test('320px: a section too narrow for the rows draws the count, and the info line is whole', async () => {
		await narrow(async (app) => {
			const { page } = app;
			await unfold(page);
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
		});
	});
});

// THE STRIP (da-qua5g). vividn, 2026-10-27: «when it's just the single row,
// new commands can come in and push previous entries to the left forming a
// little scrollback so that you can scroll back to see previous commands.
// Some commands produce multiple documents, so these appear side by side, so
// make sure that it is clear where the boundaries between different commands
// are. As new outputs are appended, animate the old ones sliding to the left».
describe('the output is a strip of commands, the newest at the right', () => {
	/** The strip as drawn: what it shows, and each group's command, box, documents and rule. */
	const strip = (page) =>
		page.locator(`${BAR} .roll`).evaluate(async (roll) => {
			// Read where things REST: a slide in flight is a transform, and
			// the older groups a slide's end mounts are scrolled past in the
			// frames after it.
			await Promise.all(roll.querySelector('.track').getAnimations().map((a) => a.finished));
			await new Promise((r) => requestAnimationFrame(() => requestAnimationFrame(r)));
			const view = roll.getBoundingClientRect();
			const span = (el) => {
				const r = el.getBoundingClientRect();
				return { left: r.left, right: r.right };
			};
			return {
				view: span(roll),
				atEnd: roll.scrollLeft + roll.clientWidth >= roll.scrollWidth - 2,
				groups: [...roll.querySelectorAll('.group')].map((g) => ({
					line: g.querySelector('.echo').innerText.replace(/^❯ /, ''),
					...span(g),
					head: g.querySelector('.head').getBoundingClientRect().top - view.top,
					docs: [...g.querySelectorAll('.doc')].map(span),
					rule: parseFloat(getComputedStyle(g).borderLeftWidth)
				}))
			};
		});
	/** Every slide the strip plays from here on, as the keyframes it was asked for. */
	const watchSlides = (page) =>
		page.evaluate(() => {
			window.__slides = [];
			const animate = Element.prototype.animate;
			Element.prototype.animate = function (frames, options) {
				if (this.classList.contains('track')) window.__slides.push(frames[0].transform);
				return animate.call(this, frames, options);
			};
		});
	const inView = (s, g) => g.left >= s.view.left - 1 && g.right <= s.view.right + 1;

	test('commands in a row: oldest to newest, left to right, the newest in view, and the old ones slid left', async () => {
		await withApp(async (app) => {
			const { page } = app;
			await watchSlides(page);
			// Five: a phone holds three whole since a group is its documents'
			// width (da-7xp82).
			const lines = ['occur coffee', 'occur water', 'occur tea', 'occur toast', 'occur juice'];
			for (const line of lines) await app.submit(line);
			// Every arrival but the first, which had nothing to slide.
			await page.waitForFunction((n) => window.__slides.length === n, lines.length - 1);

			const s = await strip(page);
			assert.deepEqual(s.groups.map((g) => g.line), lines.slice(-s.groups.length));
			for (let i = 1; i < s.groups.length; i += 1)
				assert.ok(s.groups[i].left >= s.groups[i - 1].right, `${s.groups[i].line} is not right of ${s.groups[i - 1].line}`);
			// Five do not fit a phone, so the row scrolled to its newest end.
			assert.ok(s.groups[0].left < s.view.left, 'the strip never overflowed — nothing to prove');
			assert.ok(s.atEnd, 'the strip is not at its newest end');
			assert.ok(inView(s, s.groups.at(-1)), `the newest is out of view: ${JSON.stringify(s)}`);
			// Each arrival played the old ones sliding left, from the right.
			for (const slide of await page.evaluate(() => window.__slides))
				assert.ok(parseFloat(/translateX\((-?[\d.]+)px\)/.exec(slide)[1]) > 0, slide);
			assert.deepEqual(app.errors, []);
		}, PHONE);
	});

	// da-7xp82. vividn, 2026-10-28: «it is too tall right now, it should be
	// wide enough for a single entry to fit nicely (+ maybe space for one line
	// for extra data) / let's make the font size a bit smaller in the output
	// bar, and when it is collapsed, let's put a line break between the occur
	// time and the field/state so that it is not so wide».
	test('one entry tall: the section holds an entry’s lines and one more, in type smaller than the input’s', async () => {
		await withApp(async (app) => {
			const { page } = app;
			await app.submit('occur coffee');
			await page.locator(`${DOC} .info .what`).waitFor();
			const m = await page.locator(`${BAR} .section.output`).evaluate((out) => {
				const roll = out.querySelector('.roll');
				const group = out.querySelector('.group:last-child');
				const high = (s) => group.querySelector(s).getBoundingClientRect().height;
				const style = getComputedStyle(roll);
				return {
					line: parseFloat(getComputedStyle(group.querySelector('.info')).lineHeight),
					room: roll.clientHeight - parseFloat(style.paddingTop) - parseFloat(style.paddingBottom),
					// The command's line, and the document's: the receipt, the time, the field and state.
					entry: high('.head') + high('.doc .lines'),
					font: parseFloat(getComputedStyle(group.querySelector('.doc')).fontSize),
					input: parseFloat(getComputedStyle(document.querySelector('.prompt-bar input')).fontSize)
				};
			});
			// The command, the receipt, the time, the field and state: four lines.
			assert.ok(Math.abs(m.entry - 4 * m.line) <= 1, `the entry is ${m.entry}px, not four lines of ${m.line}px`);
			assert.ok(Math.abs(m.room - 5 * m.line) <= 1, `the section holds ${m.room}px, not five lines of ${m.line}px`);
			assert.ok(m.font < m.input, `the output's type is ${m.font}px against the input's ${m.input}px`);
			assert.deepEqual(app.errors, []);
		}, PHONE);
	});

	test('an entry’s time is a line of its own, above its field and state, as the CLI cut it', async () => {
		await withApp(async (app) => {
			const { page } = app;
			await app.declare('mood', 'switch');
			await app.submit('switch mood happy');
			await page.locator(`${DOC} .info .what`).waitFor();
			const doc = await cliDoc(app, ['get', 'mood']);
			const parts = await page.locator(`${DOC} .info`).evaluate((info) =>
				Object.fromEntries(
					['when', 'what'].map((p) => {
						const el = info.querySelector(`.${p}`);
						const r = el.getBoundingClientRect();
						return [p, { text: el.textContent, top: r.top, bottom: r.bottom, left: r.left }];
					})
				)
			);
			assert.equal(parts.when.text, doc.info_time);
			assert.equal(parts.what.text, doc.info.slice(doc.info_time.length).trimStart());
			assert.match(parts.what.text, /^mood\b/);
			assert.ok(parts.when.bottom <= parts.what.top + 0.5, `the time is not above the field: ${JSON.stringify(parts)}`);
			assert.ok(Math.abs(parts.when.left - parts.what.left) <= 0.5, `the two lines do not start together: ${JSON.stringify(parts)}`);
			assert.deepEqual(app.errors, []);
		}, PHONE);
	});

	// A group was 21rem a document whatever it held, so a 390px phone showed
	// one command whole (count it on staging: 1). Each is its documents' own
	// width now, and at least the 22ch its head reads in.
	test('at 390px, two commands are in view whole, the hid uncut', async () => {
		await withApp(async (app) => {
			const { page } = app;
			for (const line of ['occur coffee', 'occur water', 'occur tea', 'occur toast']) await app.submit(line);
			await page.waitForFunction(() => document.querySelectorAll('.prompt-bar .group .doc').length === 4);
			const s = await strip(page);
			const whole = s.groups.filter((g) => inView(s, g));
			assert.ok(whole.length >= 2, `${whole.length} of ${s.groups.length} in view whole: ${JSON.stringify(s)}`);
			const hid = await page.locator(`${DOC} .hid`).evaluate((el) => el.scrollWidth - el.clientWidth);
			assert.ok(hid <= 0, `the hid is cut by ${hid}px`);
			assert.ok(inView(s, s.groups.at(-1)), 'the newest is out of view');
			assert.deepEqual(app.errors, []);
		}, PHONE);
	});

	test('a command landing mid-slide cancels the slide and slides from where the row rests', async () => {
		await withApp(async (app) => {
			const { page } = app;
			// Every slide a minute long, so the next arrival is sure to land inside it.
			await page.evaluate(() => {
				window.__played = [];
				const animate = Element.prototype.animate;
				Element.prototype.animate = function (frames, options) {
					const slide = animate.call(this, frames, { ...options, duration: 60000 });
					if (this.classList.contains('track')) window.__played.push({ slide, from: frames[0].transform });
					return slide;
				};
			});
			for (const line of ['occur coffee', 'occur water', 'occur tea']) await app.submit(line);
			await page.waitForFunction(() => window.__played.length === 2);
			const read = await page.evaluate(() => {
				const [first, second] = window.__played;
				second.slide.cancel();
				const g = document.querySelector('.prompt-bar .group:last-child');
				return {
					first: first.slide.playState,
					moved: parseFloat(/translateX\((-?[\d.]+)px\)/.exec(second.from)[1]),
					room: g.getBoundingClientRect().width + parseFloat(getComputedStyle(g).marginLeft)
				};
			});
			assert.equal(read.first, 'idle', 'the slide in flight was left playing');
			// The row was already past its width, so the old groups moved by exactly the new one's room.
			assert.ok(Math.abs(read.moved - read.room) <= 1, `slid ${read.moved}px for a group of ${read.room}px`);
			assert.deepEqual(app.errors, []);
		}, PHONE);
	});

	test('the strip settling after a command — mounting or dropping old groups — does not take the slide off', async () => {
		await desktop(async (app) => {
			const { page } = app;
			// A long scrollback, so the strip has mounted past its first few and drops and mounts as it settles.
			for (const line of ['occur coffee', 'occur water', 'occur tea', 'occur toast', 'occur juice', 'occur jam']) await app.submit(line);
			await page.evaluate(() => {
				window.__played = [];
				const animate = Element.prototype.animate;
				Element.prototype.animate = function (frames, options) {
					const slide = animate.call(this, frames, { ...options, duration: 60000 });
					if (this.classList.contains('track')) window.__played.push(slide);
					return slide;
				};
			});
			await app.submit('occur bread');
			await page.waitForFunction(() => window.__played.length === 1);
			await page.waitForTimeout(500);
			const state = await page.evaluate(() => {
				const slide = window.__played[0];
				const was = slide.playState;
				slide.cancel();
				return was;
			});
			assert.equal(state, 'running', 'the slide was taken off before it ended');
			assert.deepEqual(app.errors, []);
		});
	});

	test('a reader who asked for no motion gets the same row with no slide', async () => {
		await withApp(async (app) => {
			const { page } = app;
			await page.emulateMedia({ reducedMotion: 'reduce' });
			await watchSlides(page);
			for (const line of ['occur coffee', 'occur water', 'occur tea']) await app.submit(line);
			await page.waitForFunction(() => document.querySelectorAll('.prompt-bar .group').length === 3);
			const s = await strip(page);
			assert.ok(s.atEnd && inView(s, s.groups.at(-1)), 'the newest is out of view');
			assert.deepEqual(await page.evaluate(() => window.__slides), []);
		}, PHONE);
	});

	test('a command’s documents stand side by side in one group, ruled off from the next under its own head', async () => {
		await desktop(async (app) => {
			const { page } = app;
			await app.declare('coffee');
			await app.declare('mood', 'switch');
			await app.submit('occur coffee , switch mood happy');
			await app.submit('occur water');
			await page.locator(`${OUT} .doc`).waitFor();

			const [both, next] = (await strip(page)).groups.slice(-2);
			assert.equal(both.line, 'occur coffee , switch mood happy');
			assert.equal(both.docs.length, 2, 'the two documents are not in one group');
			for (const doc of both.docs) assert.ok(doc.left >= both.left - 0.5 && doc.right <= both.right + 0.5, 'a document is outside its group');
			assert.ok(both.docs[1].left >= both.docs[0].right, 'the documents are not side by side');
			// The boundary: a rule at the next group's left edge, past the
			// last of this one's documents, and the next command's own head
			// at the top of the row beside it.
			assert.equal(next.line, 'occur water');
			assert.ok(next.rule >= 1, 'no rule between two commands');
			assert.ok(next.left > both.docs[1].right, 'the next group begins inside this one');
			assert.ok(next.head >= 0 && next.head < 8, `the next command's head is not at the top: ${next.head}`);
			assert.deepEqual(app.errors, []);
		});
	});

	test('scrolled back, the older commands mount and a new one does not pull the reader forward; back at the end, it does', async () => {
		await withApp(async (app) => {
			const { page } = app;
			// More than the strip mounts at the end, which is a row's width
			// past the view: a group is its documents' width (da-7xp82).
			const lines = ['coffee', 'water', 'tea', 'toast', 'juice', 'bread', 'jam', 'rice', 'soup', 'pear', 'plum', 'fig', 'kiwi', 'lime'].map((f) => `occur ${f}`);
			for (const line of lines) await app.submit(line);
			const groups = () => page.evaluate(() => document.querySelectorAll('.prompt-bar .group').length);
			// Only the newest few are mounted while the reader is at the end (vremya on #1269),
			// once the strip settles: a phone's four newest no longer reach a row past the view,
			// so the last slide's end mounts more and the settling trims them to three rows
			// (`rest`, PromptBar.svelte), 13 to 8 in the half second after (da-7xp82).
			await page
				.waitForFunction((n) => document.querySelectorAll('.prompt-bar .group').length < n, lines.length, { timeout: 5000 })
				.catch(() => {});
			assert.ok((await groups()) < lines.length, `all ${lines.length} commands were mounted at the end`);

			// Back to the first command, as a finger or a wheel would scroll: each time the
			// row nears the oldest mounted, more mount, and the reader stays where they were.
			const roll = page.locator(`${BAR} .roll`);
			for (let i = 0; i < 10 && (await strip(page)).groups[0].line !== 'occur coffee'; i += 1) {
				await roll.evaluate((el) => el.scrollTo({ left: 0 }));
				await page.waitForTimeout(100);
			}
			await roll.evaluate((el) => el.scrollTo({ left: 0 }));
			await page.waitForFunction(() => document.querySelector('.prompt-bar .roll').scrollLeft === 0);
			const back = await strip(page);
			assert.deepEqual(back.groups.map((g) => g.line), lines);
			assert.ok(inView(back, back.groups[0]), 'scrolling back did not reach the first command');

			await app.submit('occur milk');
			await page.waitForFunction(() => document.querySelector('.prompt-bar .group:last-child .echo').innerText.includes('milk'));
			const kept = await strip(page);
			assert.equal(kept.groups[0].line, 'occur coffee', 'the command being read was unmounted');
			assert.ok(Math.abs(kept.groups[0].left - back.groups[0].left) <= 1, 'the reader was moved');
			assert.ok(!kept.atEnd, 'the strip jumped to its end');

			await roll.evaluate((el) => el.scrollTo({ left: el.scrollWidth }));
			await page.waitForFunction(() => {
				const el = document.querySelector('.prompt-bar .roll');
				return el.scrollLeft + el.clientWidth >= el.scrollWidth - 2;
			});
			await app.submit('occur egg');
			await page.waitForFunction(() => document.querySelector('.prompt-bar .group:last-child .echo').innerText.includes('egg'));
			const s = await strip(page);
			assert.ok(s.atEnd && inView(s, s.groups.at(-1)), 'back at the end, the newest is out of view');
			assert.deepEqual(app.errors, []);
		}, PHONE);
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
			assert.match(said, /nope9: Entry not found/);
			assert.deepEqual(await page.locator(`${DOC} .action`).allInnerTexts(), ['DELETE']);
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
			assert.match(shown[0].replace(ESC, ''), /^CREATE /);
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
	// output section open by default, always above the input bar».
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

	// THE HANDLE IS A CONTROL, and it is drawn above the row's text
	// (da-0b2vy, where it took the arrow's place).
	//
	// vividn, 2026-09-18: "Hitting ^ should not focus on the input bar or open
	// the keyboard. That is jarring." Then: "The ^ button obscures the text."
	// And 2026-10-27: «the top of the output bar should show a very clear
	// handle toggle. Clicking or swiping up on this makes it full screen». So:
	// a real button, labelled and focusable, whose INK AND HIT AREA are both
	// clear of the row's text, and whose tap goes closed → row → full → row.
	test('the handle moves the bar a state a tap, never focuses the box, and clears the row’s text', async () => {
		await withApp(async (app) => {
			const { page } = app;
			const handle = page.locator(`${BAR} .bar-handle`);
			const output = page.locator(`${BAR} .section.output`);
			const focused = () => page.evaluate(() => document.activeElement?.tagName.toLowerCase() ?? 'none');
			const labelled = (label) => page.locator(`${BAR} .bar-handle[aria-label="${label}"]`).waitFor();

			// Folded, as a reader who dragged it shut has it.
			await output.waitFor();
			await foldBar(page);
			// The drag's click swallow spent (#lib/grip.ts), or it takes the first tap.
			await page.locator(BAR).dispatchEvent('click');
			const seen = await page.evaluate(() => {
				const el = document.querySelector('.prompt-bar .bar-handle');
				const box = document.querySelector('.prompt-bar input').getBoundingClientRect();
				const r = el.getBoundingClientRect();
				return {
					tag: el.tagName.toLowerCase(),
					hidden: el.getAttribute('aria-hidden'),
					height: r.height,
					// Its lowest pixel is above the box's highest: neither the
					// tab nor the target reaches what is typed.
					clearsTextBy: box.top - r.bottom,
					// And a press at its middle reaches IT, not something under it.
					takesThePress: document
						.elementFromPoint(r.left + r.width / 2, r.top + r.height / 2)
						?.closest('button')
						?.classList.contains('bar-handle')
				};
			});
			assert.equal(seen.tag, 'button', `the handle is a <${seen.tag}>`);
			assert.notEqual(seen.hidden, 'true', 'the handle is hidden from a screen reader');
			assert.ok(seen.height >= 24, `the handle is ${seen.height}px tall`);
			assert.ok(seen.clearsTextBy > 0, `the handle overlaps the box's text by ${-seen.clearsTextBy}px`);
			assert.equal(seen.takesThePress, true, 'a press at the handle does not reach it');

			// Closed → the row → full → the row, and none takes the keyboard.
			await labelled('open the output section');
			await handle.click();
			await output.waitFor();
			await labelled('open the history full screen');
			assert.notEqual(await focused(), 'input', 'the handle opened the keyboard');
			await handle.click();
			await output.waitFor({ state: 'detached' });
			await labelled('collapse the history to one row');
			assert.notEqual(await focused(), 'input', 'the handle opened the keyboard');
			await handle.click();
			await output.waitFor();
			await labelled('open the history full screen');

			// Nor when there is a half-typed line under it to come back to.
			await page.locator(`${BAR} input`).click({ position: { x: 4, y: 12 } });
			await page.locator(`${BAR} input`).type('occur cof');
			await handle.click();
			await labelled('collapse the history to one row');
			assert.notEqual(await focused(), 'input', 'the handle opened the keyboard');
			assert.equal(await page.locator(`${BAR} input`).inputValue(), 'occur cof');
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

	const marks = (page) => page.locator(`${BAR} .marks`);

	/** What is left to type in, to the pixel it is drawn at. */
	const boxWidth = (page) =>
		page.locator(`${BAR} input`).evaluate((el) => el.getBoundingClientRect().width);

	test('its ACTION words stand at the bottom of the row, over by the ❯, in the CLI’s own colours, and fade', async () => {
		const AT = todayAt(12, 0);
		await withApp(async (app) => {
			const { page } = app;
			// A held clock does not settle by itself: the pane's own box
			// arrives through a timer (tick.test.mjs).
			await page.clock.runFor(200);
			await app.idle();
			assert.equal(await page.locator(`${BAR} .section.output`).count(), 0, 'the bar was not folded');

			// NOT declared first: the clock is held, and `track` would stamp
			// its own document at the very instant `occur` wants. Since
			// da-gvcen a first log is ONE document — it declares its field in
			// its own `following_state` — so this is the single-document
			// case, and what is asserted is that the words it DOES say are
			// the CLI's own, read off the scrollback of the same run.
			await app.submit('occur coffee');
			await marks(page).waitFor();
			const words = await page.locator(`${BAR} .marks .action`).allInnerTexts();
			// The CLI's own action words, in the CLI's order — read off the
			// documents the very same run drew in the scrollback, so nothing
			// here is a word this file made up and nothing is run twice.
			assert.ok(words.length > 0, 'the folded bar said nothing about a CREATE');
			assert.deepEqual(words, await page.locator('.cli .doc .action').allInnerTexts());
			// IN THE BOTTOM OF THE SECTION, AND OVER BY THE `❯`. vividn asked
			// first for the right of the row ("show all the ACTION words on the
			// right side"), then for the padding under it ("move the ACTION
			// results in the padding underneath the input"), and then, seeing
			// that on a phone, 2026-09-18: "Center the text vertically in the
			// input section. For the output ACTION items when the output
			// section is hidden. but them still in bottom part like this, but
			// over by the >. Over on the right side they are hidden."
			//
			// So the row no longer gives up a strip's height to them — it is
			// centred in the whole section — and the words OVERLAY the bottom
			// of it, which they can because they begin at the `❯` and the box's
			// own text begins after it. The claims are therefore where they
			// END (the section's floor), which half they are in (the bottom),
			// and where they START (at the `❯`, not at the right edge).
			const where = await page.evaluate(() => {
				const box = (s) => document.querySelector(s).getBoundingClientRect();
				return {
					marks: box('.prompt-bar .marks'),
					field: box('.prompt-bar input'),
					caret: box('.prompt-bar .line > .prompt'),
					clock: box('.prompt-bar .section.input .stamp'),
					section: box('.prompt-bar .section.input'),
					bar: box('.prompt-bar')
				};
			});
			assert.ok(
				Math.abs(where.marks.bottom - where.section.bottom) <= 1,
				`the words end at ${where.marks.bottom}, the input section at ${where.section.bottom}`
			);
			const middle = where.section.top + where.section.height / 2;
			assert.ok(
				where.marks.top >= middle,
				`the words start at ${where.marks.top}, above the section's middle at ${middle}`
			);
			// Over by the `❯`: at it, not at the right edge and not back at the
			// clock. Within a character of the `❯`'s own left edge.
			assert.ok(
				Math.abs(where.marks.left - where.caret.left) <= 12,
				`the words start at ${where.marks.left}, the ❯ at ${where.caret.left}`
			);
			assert.ok(
				where.marks.left > where.clock.right,
				`the words start at ${where.marks.left}, back over the clock which ends at ${where.clock.right}`
			);
			assert.ok(where.marks.right <= where.bar.right + 1);

			// "Make sure these ACTIONs are colored appropriately" (vividn). The
			// colour is datum's, not the web's: `Outcome::hex` publishes it as
			// `action_color` on `--show json` and both surfaces paint from it,
			// so the bar and the scrollback cannot disagree. A CREATE's green
			// is named here too, so a palette that quietly went grey would not
			// pass by matching a grey scrollback.
			const painted = await page.locator(`${BAR} .marks .action`).evaluateAll((els) =>
				els.map((el) => getComputedStyle(el).color)
			);
			assert.deepEqual(
				painted,
				await page.locator('.cli .doc .action').evaluateAll((els) => els.map((el) => getComputedStyle(el).color))
			);
			assert.ok(
				painted.includes('rgb(38, 162, 105)'),
				`no CREATE green (#26a269) among ${painted.join(', ')}`
			);

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

	// The words cost the box NOTHING. They were capped at a share of the row
	// (45%), which is not a floor: a four-sided command echoes eight
	// documents, filled the whole share, and left a 360px phone twenty pixels
	// to type in for fifteen seconds (measured after vremya's 127px for one
	// word, PR #46). Since vividn moved them into the strip under the row they
	// are not on the row at all, so the claim is the stronger one — the box
	// does not move — and the words are cut against the bar's width instead.
	test('however many documents a command echoes, the box does not move and the words are cut instead', async () => {
		const AT = todayAt(12, 0);
		await withApp(async (app) => {
			const { page } = app;
			await page.clock.runFor(200);
			await app.idle();

			await app.submit('occur alfa');
			await marks(page).waitFor();
			const one = await boxWidth(page);

			// Enough documents to overflow 360px. A first log used to echo
			// TWO — the entry and the TRACK row that declared its field — so
			// four sides were eight words; da-gvcen made a first log one
			// document, so the crowding has to come from the sides
			// themselves. Eight of them, and the assertions below MEASURE
			// that it really overflowed rather than trusting the count.
			await app.submit(
				'occur bravo , occur charlie , occur delta , occur echo , ' +
					'occur foxtrot , occur golf , occur hotel , occur india'
			);
			await page.waitForFunction(
				() => document.querySelectorAll('.prompt-bar .marks .action').length > 2
			);
			const many = await boxWidth(page);

			assert.ok(many >= 100, `eight documents left the box ${many.toFixed(0)}px at 360px`);
			assert.equal(
				await page.locator(`${BAR} .marks .action`).count(),
				8,
				'eight sides, eight documents'
			);
			assert.ok(
				Math.abs(many - one) <= 0.5,
				`the words moved the box: ${one.toFixed(1)}px for one, ${many.toFixed(1)}px for eight`
			);
			// Cut, not wrapped, and not taken out of the box.
			const cut = await page
				.locator(`${BAR} .marks`)
				.evaluate((el) => ({ over: el.scrollWidth - el.clientWidth, height: el.getBoundingClientRect().height }));
			assert.ok(cut.over > 0, 'eight words fitted, so nothing was being held back');
			assert.ok(cut.height <= 32, `the words wrapped to ${cut.height}px instead of being cut`);
			assert.deepEqual(app.errors, []);
		}, { ...held(AT), viewport: { width: 360, height: 640 } });
	});

	// THE LETTERS ARE NOT CUT OFF ALONG THE BOTTOM. vividn, 2026-09-21, of the
	// deployed build: "the ACTION verb is slightly clipped under the first
	// repl input history line." The strip's floor IS the input section's
	// floor — the test above pins them within a pixel of each other — so a
	// glyph that hangs below its line box hangs past the strip, and the
	// strip's `overflow` took it away.
	//
	// Two things put it there, and both are asserted, because either alone
	// would leave it cut on a machine whose monospace differs from this one:
	// the line box must HOLD the font's own box (`line-height: 1` is the
	// font SIZE, and a font's ascent and descent are taller than that), and
	// what clipping the strip does must be SIDEWAYS ONLY.
	//
	// Measured through a Range, which is the only box here that is the
	// font's: `.action` is a flex item, so it is blockified and its own rect
	// is the line box — the very thing whose being too short is the defect.
	test('the ACTION words are not clipped along the bottom of the strip', async () => {
		const AT = todayAt(12, 0);
		await withApp(async (app) => {
			const { page } = app;
			await page.clock.runFor(200);
			await app.idle();
			// FOLDED is the state it appears in, and the only one: the strip
			// is drawn `{#if !$output}`, so an open bar has no words to clip.
			assert.equal(await page.locator(`${BAR} .section.output`).count(), 0, 'the bar was not folded');

			await app.submit('occur coffee');
			await marks(page).waitFor();

			const ink = await page.evaluate(() => {
				const strip = document.querySelector('.prompt-bar .marks');
				const word = strip.querySelector('.action');
				const range = document.createRange();
				range.selectNodeContents(word);
				const glyphs = range.getBoundingClientRect();
				const box = strip.getBoundingClientRect();
				const style = getComputedStyle(strip);
				return {
					text: word.textContent,
					below: glyphs.bottom - box.bottom,
					overflowX: style.overflowX,
					overflowY: style.overflowY
				};
			});

			assert.ok(ink.text.trim().length > 0, 'there was no word to measure');
			assert.ok(
				ink.below <= 0,
				`${JSON.stringify(ink.text)} hangs ${ink.below.toFixed(2)}px below the strip it is clipped by`
			);
			// And if some other font ever does hang over anyway, it is seen
			// hanging over rather than cut: the strip clips one axis.
			assert.equal(ink.overflowY, 'visible', 'the strip clips the letters\u2019 own axis');
			assert.notEqual(ink.overflowX, 'visible', 'the words may push the row wider');
			assert.deepEqual(app.errors, []);
		}, held(AT));
	});

	test('a failure says ERROR, in red, for longer — and a tap opens the output on what went wrong', async () => {
		const AT = todayAt(12, 0);
		await withApp(async (app) => {
			const { page } = app;
			await page.clock.runFor(200);
			await app.idle();

			await app.submit('occur nope --nonsense');
			await page.locator(`${BAR} .failed`).waitFor();
			const failed = page.locator(`${BAR} .failed`);
			assert.equal((await failed.innerText()).trim(), 'ERROR');
			assert.ok(
				await failed.evaluate((el) => el.getBoundingClientRect().height >= 24),
				'the ERROR is not a fingertip'
			);
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

			// Past the moment a CREATE would have gone, and still there.
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

	test('a re-log exits 11, «not an error», and says EXISTS in its colour — a collision still says ERROR (da-97w7z)', async () => {
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
					return [...document.querySelectorAll('.prompt-bar .marks .action')].map((el) => ({
						text: el.innerText,
						color: getComputedStyle(el).color
					}));
				});
			};

			await said('occur coffee');
			assert.equal((await app.run(['occur', 'coffee'])).code, 11, 'the second log is not a re-log');
			const relog = await said('occur coffee');
			// The colour is the one the same run's scrollback document drew.
			const drawn = page.locator('.cli .doc .action', { hasText: 'EXISTS' }).first();
			assert.deepEqual(relog, [{ text: 'EXISTS', color: await drawn.evaluate((el) => getComputedStyle(el).color) }]);

			// The same exit with nothing echoed is a slug collision: a failure.
			assert.deepEqual(await said('occur coffee n=2'), { failed: 'ERROR' });
			assert.deepEqual(app.errors, []);
		}, held(AT));
	});
});

/**
 * Wait until the pills have stopped moving.
 *
 * Two lags to outlast, and neither is a keystroke: the mirror's width is
 * measured by a resize observer, and `.pills` transitions its `left` so a
 * pill left behind by a ghost is seen going back. Measured before both, a
 * pill is still standing where the previous line put it.
 */
const settlePills = (page) =>
	page.evaluate(
		() =>
			new Promise((done) => {
				let last = '';
				let same = 0;
				const tick = () => {
					const now = [...document.querySelectorAll('.prompt-bar .pill')]
						.map((p) => Math.round(p.getBoundingClientRect().left))
						.join(',');
					same = now === last ? same + 1 : 0;
					last = now;
					if (same > 4) return done();
					requestAnimationFrame(tick);
				};
				tick();
			})
	);

/** Where a pill's left edge is, and where the `❯` ends. */
const pillAt = (page) =>
	page.evaluate(() => ({
		pill: document.querySelector('.prompt-bar .pill')?.getBoundingClientRect().left,
		caret: document.querySelector('.prompt-bar .line > .prompt').getBoundingClientRect().right
	}));

/** Track `coffee` and get its button onto the widget bar, as ghost.test.mjs does. */
async function withCoffee(app) {
	// Typed, not quiet: a write is what refreshes the bar, a read does not (da-fprk0).
	await seedLines(app, ['setup coffee --verb occur']);
	await app.page.locator('.bar [data-field="coffee"]').waitFor();
}

const setPill = (page, pill) =>
	page.evaluate((p) => window.datum.bar.setPills([p]), { id: 'p', kind: 'time', ...pill });

describe('the pills ride with the text', () => {
	test('a pill sits just after the ❯ when the box is empty, and moves right as the line grows', async () => {
		await desktop(async (app) => {
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

			// However long the line gets it goes OFF THE RIGHT EDGE with it,
			// and the box scrolls right to reach it. vividn, 2026-09-18: "They
			// should be to right, and just go off the right edge of the screen.
			// The user can scroll to the right when input text gets long, so it
			// should be equivalently present in this rightwards scroll." This
			// replaces the claim that stood here — that the pill never leaves
			// the box, which is what parked it at the box's right edge.
			await input.fill('occur coffee ' + 'x'.repeat(400));
			await app.idle();
			await settlePills(page);
			const long = await page.evaluate(() => {
				const pill = document.querySelector('.prompt-bar .pill').getBoundingClientRect();
				const mirror = document.querySelector('.prompt-bar .mirror').getBoundingClientRect();
				const port = document.querySelector('.prompt-bar .field');
				return {
					pill: pill.left,
					text: mirror.right,
					reach: port.scrollWidth - port.clientWidth
				};
			});
			// Still after the text, wherever the text has got to, and with
			// somewhere to scroll to it.
			assert.ok(long.pill >= long.text - 1, `the pill at ${long.pill} is over the text ending at ${long.text}`);
			assert.ok(long.reach > 0, 'a line 400 characters long left the run with nothing to scroll');

			// And a scroll to the end of the run brings it whole into the box.
			await page.locator(`${BAR} .field`).evaluate((el) => (el.scrollLeft = el.scrollWidth));
			await page.waitForFunction(() => {
				const pill = document.querySelector('.prompt-bar .pill').getBoundingClientRect();
				const box = document.querySelector('.prompt-bar .field').getBoundingClientRect();
				return pill.left >= box.left - 1 && pill.right <= box.right + 1;
			});
			assert.deepEqual(app.errors, []);
		});
	});

	test('a press spends its unlocked pill beside the ghost; a locked one stays and comes back left', async () => {
		await day.solo(async (app) => {
			const { page } = app;
			await withCoffee(app);
			await setPill(page, { text: '-t 14:30', locked: true });
			await page.locator(`${BAR} .pill`).waitFor();
			const waiting = (await pillAt(page)).pill;
			const placed = await page.evaluate(() => parseFloat(document.querySelector('.prompt-bar .pills').style.left));

			// A press nobody typed — the widget bar's, which is the press that
			// carries the pills (`pressWithPills`). Watched from before it:
			// the ghost can be gone by the time the press lands (da-ehpe5).
			const saw = await app.watchGhosts();
			await app.pressWidget('coffee');
			// The ghost is the command ALONE: the pill is drawn as a pill
			// beside it, so saying it in the text too would say it twice.
			assert.deepEqual([...new Set((await saw('occur coffee')).map((g) => g.line))], ['occur coffee']);
			await page.waitForFunction(
				(was) => window.__datumGhosts.some((g) => g.left > was + 20),
				placed
			);

			// The ghost goes; the locked pill stays and returns leftward, to
			// where it waited.
			await page.waitForFunction(() => !document.querySelector('.prompt-bar .ghost'));
			await page.waitForFunction(
				(was) => Math.abs(document.querySelector('.prompt-bar .pill').getBoundingClientRect().left - was) < 1,
				waiting
			);
			assert.equal(await page.locator(`${BAR} .pill`).count(), 1, 'a locked pill was spent');
			assert.deepEqual(app.errors, []);
			// The day page: `withCoffee` presses the FIELDS widget, which is a
			// pane of the day frame and is not on the repl layout.
		});
	});

	test('a spent pill fades in step with the ghost it went into, though it is spent later', async () => {
		await withApp(async (app) => {
			const { page } = app;
			await withCoffee(app);

			// The ghost starts at the press and the pill is spent only when
			// the command lands (da-jrozq), so a pill that started a fade of
			// its own would lag the ghost by the whole command. Both clocks
			// read in one evaluation, the moment the spent pill is there.
			//
			// And a busy frame, made here rather than hoped for from a loaded
			// runner (da-d7ndh): the main thread held 150ms the moment the
			// spent pill is in the page, so a pill whose fade starts at the
			// next frame it gets falls that far behind, every run.
			await page.evaluate(() => {
				new MutationObserver(() => {
					if (window.__held || !document.querySelector('.prompt-bar .spent')) return;
					window.__held = performance.now();
					while (performance.now() - window.__held < 150);
				}).observe(document.body, { subtree: true, childList: true });
			});
			await pressLanded(app);
			const clocks = await page
				.waitForFunction(() => {
					const pill = document.querySelector('.prompt-bar .spent');
					const ghost = document.querySelector('.prompt-bar .ghost');
					if (!pill || !ghost) return null;
					// How far through, in ms: the pill takes the ghost's start
					// time (`withGhost`), so a busy frame cannot put it behind.
					const t = (el) => el.getAnimations()[0].effect.getComputedTiming().progress * 1000;
					return { pill: t(pill), ghost: t(ghost) };
				})
				.then((h) => h.jsonValue());
			assert.ok(Math.abs(clocks.pill - clocks.ghost) < 50, `pill at ${clocks.pill}ms, ghost at ${clocks.ghost}ms`);
			assert.ok(await page.evaluate(() => window.__held), 'the busy frame never came: nothing held the spent pill');
			await page.waitForFunction(() => !document.querySelector('.prompt-bar .spent'));
			assert.deepEqual(app.errors, []);
			// The wall clock: how far into its fade the pill joins is the
			// time the command took, on `Date` (#lib/bar.ts), which a held
			// clock says was none.
		}, { ...DAY, clock: null });
	});

	test('an unlocked pill leaves the live set at once and fades as a spent one', async () => {
		await day.solo(async (app) => {
			const { page } = app;
			await withCoffee(app);

			// Gone from the live pills — so gone from the next command's argv
			// and from the time cursor — and on screen only as a spent one.
			// Counted at the landing itself: the spent pill lives what is left
			// of GHOST_MS after it, which can be milliseconds (da-ohoei).
			const { pills, spent } = await pressLanded(app);
			assert.equal(pills, 0);
			assert.equal(spent, 1);
			await page.waitForFunction(() => !document.querySelector('.prompt-bar .spent'));
			assert.deepEqual(app.errors, []);
			// The day page, as above: the press is the fields widget's.
		});
	});
});

/** How long the ghost stays, and so a spent pill with it (#lib/bar.ts). */
const GHOST_MS = 1000;

/**
 * Load an unlocked time pill, press the coffee widget and wait for the
 * press to LAND: the pill leaving the live set, which is the moment it is
 * spent (da-jrozq). A spent pill joins the ghost's clock, so a press that
 * lands more than GHOST_MS after it was pressed leaves none to see, and a
 * test waiting for one would time out 30s later reading as a missing
 * feature (da-wjz42). A loaded CI runner took 1032ms once (da-d7ndh): that
 * is the runner, not the bar, so a press that slow is pressed again with a
 * fresh pill — at a time of its own, so no two presses write one entry —
 * and five of them fail, with every timing.
 *
 * Each press waits for nothing to be in flight (da-8ep3q): a press lands
 * after the write's own bar reads, and one pressed at once after the last
 * landed queued its write behind that landing's pane refreshes — 2 to 8 of
 * them in flight at every retry, measured — so on a loaded runner every
 * retry ran slow for the press before it (1028, 1066, 1151ms).
 *
 * Answers the bar's live and spent pill counts read in the landing's own
 * evaluation, since a press landing just under GHOST_MS leaves the spent
 * pill milliseconds to live.
 */
const pressLanded = async (app) => {
	const { page } = app;
	const took = [];
	while (took.length < 5) {
		await setPill(page, { text: `-t 14:3${took.length}` });
		await page.locator(`${BAR} .pill`).waitFor();
		await page.evaluate(() => new Promise((done) => requestAnimationFrame(() => requestAnimationFrame(done))));
		await app.idle();
		await page.evaluate(() => (window.__pressedAt = performance.now()));
		await app.pressWidget('coffee');
		const landed = await page
			.waitForFunction(
				(bar) =>
					window.datum.bar.pills().length === 0 && {
						took: performance.now() - window.__pressedAt,
						pills: document.querySelectorAll(`${bar} .pill`).length,
						spent: document.querySelectorAll(`${bar} .spent`).length
					},
				BAR
			)
			.then((h) => h.jsonValue());
		took.push(landed.took);
		if (landed.took < GHOST_MS) return landed;
	}
	assert.fail(`no press landed inside the ghost's ${GHOST_MS}ms: ${took.map(Math.round).join(', ')}ms`);
};

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
		await desktop(async (app) => {
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
		});
	});

	test('a press anywhere but the lock removes it — text, ×, padding and Enter alike', async () => {
		await desktop(async (app) => {
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
		});
	});

	test('three pills do not grow the bar', async (t) => {
		await desktop(async (app) => {
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
				//
				// Three pills are wider than a 390px phone's whole row — 435px
				// of them against a 390px screen — so there the second case is
				// the only one available and not every pill CAN be on screen.
				// What is asked of it is the bound that is achievable: the set
				// reaches leftward to the screen's edge and no further, so the
				// leftmost pill is always reachable. (A set wider than the
				// screen still runs off the right; showing all of it would
				// want a scroll or a wrap, which no flow of the app asks for
				// today — the pair a `retime` makes is two, and that pair is
				// held whole by the test above.) Riding after the `❯` is the
				// FIRST case's claim, made where the pills fit.
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
						pillsWidth: pills.width,
						box: at('.field').right,
						boxWidth: at('.field').width,
						screen: document.documentElement.clientWidth
					};
				});
				const fits = after.pillsWidth <= after.boxWidth;
				if (fits)
					assert.ok(after.pill > after.caret, `the pill at ${after.pill}, the ❯ ends at ${after.caret}`);
				else
					assert.ok(
						after.pill >= -0.5,
						`the pills reach to ${after.pill}, past the screen's left edge`
					);
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
		});
	});
});

describe('two pills on a phone', () => {
	// vividn's own flow: a tail row, a dayview tap, one tap on `retime` — which
	// puts a hid pill AND a time pill in the prompt at once. On this bead's
	// row they no longer fit the box, and where they go was settled twice.
	//
	// vremya asked, and I built, pills that reach LEFTWARD to stay on the
	// screen. vividn overruled it (2026-09-18): "About pills overlapping the
	// clock, this is not correct behavior. They should never go over the >
	// at all. They should be to right, and just go off the right edge of the
	// screen. The user can scroll to the right when input text gets long, so
	// it should be equivalently present in this rightwards scroll."
	//
	// So the claim is HIS, and it is the one this file holds now: the `❯` is
	// a wall the pills never cross, they ride after the typed text, and they
	// come back with it when a long line is scrolled right. Going off the
	// right edge is allowed — it is what he asked for — so nothing here
	// asserts they are all on the screen.
	const TWO = [
		{ id: 'h', kind: 'hid', text: '35mtf', locked: true },
		{ id: 't', kind: 'time', text: '-t 14:30', locked: true }
	];

	const settle = settlePills;

	/** Every pill's left edge, the scrollport's box, and where the `❯` ends. */
	const where = (page) =>
		page.evaluate(() => {
			const port = document.querySelector('.prompt-bar .field');
			const box = port.getBoundingClientRect();
			return {
				pills: [...document.querySelectorAll('.prompt-bar .pill')].map((p) =>
					Math.round(p.getBoundingClientRect().left)
				),
				ends: [...document.querySelectorAll('.prompt-bar .pill')].map((p) =>
					Math.round(p.getBoundingClientRect().right)
				),
				// The one wall: nothing of the run is PAINTED outside this,
				// whatever the run's own layout says, because the port clips.
				port: { left: Math.round(box.left), right: Math.round(box.right) },
				left: port.scrollLeft,
				reach: port.scrollWidth - port.clientWidth,
				caret: Math.round(document.querySelector('.prompt-bar .line > .prompt').getBoundingClientRect().right)
			};
		});

	for (const width of [360, 390]) {
		test(`at ${width} the pills stay right of the \u276f, typed line or not`, async (t) => {
			await withApp(async (app) => { // its own: one screen per test
				const { page } = app;
				await page.evaluate((p) => window.datum.bar.setPills(p), TWO);
				await page.locator(`${BAR} .pill`).nth(1).waitFor();
				await settle(page);

				const empty = await where(page);
				for (const left of empty.pills)
					assert.ok(left >= empty.caret, `a pill starts at ${left}, left of the \u276f at ${empty.caret}`);

				// A line long enough to push them: they follow the text, and
				// still never reach the `\u276f`.
				await page.locator(`${BAR} input`).fill('occur coffee beans=ethiopian roast=dark');
				await settle(page);
				const typed = await where(page);
				for (const left of typed.pills)
					assert.ok(left >= typed.caret, `typing put a pill at ${left}, left of the \u276f at ${typed.caret}`);
				assert.ok(
					typed.pills[0] > empty.pills[0],
					`the pills did not follow the text: ${empty.pills[0]} then ${typed.pills[0]}`
				);

				// And scrolling that line right brings them back onto the screen
				// — "equivalently present in this rightwards scroll". The
				// SCROLLPORT is what the `\u276f` is walled off by: the run may
				// lie left of it, and what lies left of it is clipped away.
				assert.ok(
					typed.reach > 0,
					'the pills are off the edge with nothing left to scroll to them'
				);
				assert.ok(
					typed.port.left >= typed.caret,
					`the box starts at ${typed.port.left}, over the \u276f at ${typed.caret}`
				);
				await page.locator(`${BAR} .field`).evaluate((el) => (el.scrollLeft = el.scrollWidth));
				await settle(page);
				const scrolled = await where(page);
				assert.ok(
					scrolled.pills[0] < typed.pills[0],
					`scrolling right did not bring the pills back: ${typed.pills[0]} then ${scrolled.pills[0]}`
				);
				// The END of the run is inside the port, which is what makes the
				// last pill reachable — the pair together is wider than a
				// 360px phone's box, so no scroll can hold both of them at
				// once and asking for that would be asking for the impossible.
				const end = scrolled.ends[scrolled.ends.length - 1];
				assert.ok(
					end <= scrolled.port.right + 1,
					`the last pill ends at ${end}, past the box's ${scrolled.port.right}`
				);
				assert.equal(scrolled.left, scrolled.reach, 'the run did not scroll to its end');
				assert.deepEqual(app.errors, []);
			}, { viewport: { width, height: 720 }, device: { isMobile: true, hasTouch: true }, prepare: withBar });
		});
	}
});

describe('a phone stacks the mark over the clock (da-ovdtt)', () => {
	// vividn, 2026-09-28, from vremya's walk on a Pixel 7: side by side, the
	// mark and the clock took about 40% of the bar, and with `occur tea
	// amount=3 cups=1` composed only `[mount=3] [cups=1]` showed. So on a
	// phone they share one column, the mark over the clock — and the mark
	// stays, because it is the database switcher.
	const PIXEL = { device: devices['Pixel 7'], prepare: withBar };

	/**
	 * The mark, the clock and where the box starts — and where the box
	 * started when the two stood side by side, which is the same page with
	 * the column taken back out of the row (`display: contents`, as a wide
	 * screen has it).
	 */
	const measure = (page) =>
		page.evaluate(async () => {
			const box = (s) => document.querySelector(s).getBoundingClientRect();
			const frame = () => new Promise((r) => requestAnimationFrame(() => requestAnimationFrame(r)));
			const now = { mark: box('.prompt-bar .brand'), clock: box('.prompt-bar .section.input .stamp'), input: box('.prompt-bar .section.input'), field: box('.prompt-bar input').left };
			const head = document.querySelector('.prompt-bar .head');
			head.style.display = 'contents';
			await frame();
			const before = box('.prompt-bar input').left;
			head.style.display = '';
			await frame();
			return { ...now, before };
		});

	test('at a Pixel 7 the clock sits under the mark, and the box starts further left', async () => {
		await withApp(async (app) => {
			const { mark, clock, input, field, before } = await measure(app.page);
			assert.ok(Math.abs(mark.left - clock.left) <= 1, `the mark starts at ${mark.left}, the clock at ${clock.left}`);
			assert.ok(clock.top >= mark.bottom - 1, `the clock starts at ${clock.top}, above the mark's foot at ${mark.bottom}`);
			for (const [what, b] of [['the mark', mark], ['the clock', clock]])
				assert.ok(b.top >= input.top - 1 && b.bottom <= input.bottom + 1, `${what} is ${b.top}–${b.bottom}, the input row ${input.top}–${input.bottom}`);
			assert.ok(field < before - 40, `the box starts at ${field}, side by side it started at ${before}`);
			assert.deepEqual(app.errors, []);
		}, PIXEL);
	});

	test('at a Pixel 7 a tap on the mark still opens the databases', async () => {
		await withApp(async (app) => {
			const { page } = app;
			await page.locator(`${BAR} .brand`).tap();
			await page.locator('[role="menu"][aria-label="databases"]').waitFor();
			assert.deepEqual(app.errors, []);
		}, PIXEL);
	});

	// The control: a wide screen was side by side and still is.
	test('a desktop is unchanged: the box starts where it did', async () => {
		await desktop(async (app) => {
			const { mark, clock, field, before } = await measure(app.page);
			assert.ok(clock.left >= mark.right - 1, `the clock starts at ${clock.left}, inside the mark ending at ${mark.right}`);
			assert.equal(field, before);
			assert.deepEqual(app.errors, []);
		});
	});
});

describe('the clock', () => {
	// It was UNDER the logo while the bar was two tiers with the mark
	// spanning both (da-xfa4). da-ckw3 put the four things of the input on
	// ONE row, so the claim moves with it: the clock is BESIDE the mark, on
	// the row, and still on the box's own line.
	test('is beside the logo, on the input row’s line', async () => {
		await desktop(async (app) => {
			const { page } = app;
			const boxes = await page.evaluate(() => {
				const rect = (s) => {
					const r = document.querySelector(s).getBoundingClientRect();
					return { top: r.top, bottom: r.bottom, left: r.left, mid: r.top + r.height / 2 };
				};
				return {
					brand: rect('.prompt-bar .brand'),
					stamp: rect('.prompt-bar .section.input .stamp'),
					input: rect('.prompt-bar input'),
					clock: document.querySelector('.prompt-bar .section.input .stamp').textContent
				};
			});
			assert.match(boxes.clock, /^\d\d:\d\d:\d\d$/, `the clock reads ${boxes.clock}`);
			assert.ok(boxes.stamp.left > boxes.brand.left, 'the clock is not past the logo');
			assert.ok(
				Math.abs(boxes.stamp.mid - boxes.brand.mid) < 6,
				`the clock's middle at ${boxes.stamp.mid}, the mark's at ${boxes.brand.mid} — not one row`
			);
			assert.ok(
				Math.abs(boxes.stamp.mid - boxes.input.mid) < 12,
				`the clock's middle at ${boxes.stamp.mid}, the prompt's at ${boxes.input.mid}`
			);
			assert.deepEqual(app.errors, []);
		});
	});
});
