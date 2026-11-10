// The bar's output as a strip of commands, the newest at the right.
// One of 3 files split from bar.test.mjs by topic, so each stays well inside
// test.mjs's 300 s file cap (da-5rpcr); the helpers they share are
// support/bar.mjs.

import test, { before, after, describe } from 'node:test';
import assert from 'node:assert/strict';
import { sharedApp, startApp, stopApp, withApp } from './support/app.mjs';
import { DESKTOP, PHONE, BAR, OUT, DOC, repl, section, cliDoc } from './support/bar.mjs';

before(startApp);
after(stopApp);

const desktop = repl(sharedApp(DESKTOP));

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
					// The top of the command's head, on its frame's border (da-hbpkw).
					head: g.querySelector('.echo').getBoundingClientRect().top - view.top,
					docs: [...g.querySelectorAll('.doc')].map(span),
					frame: span(g.querySelector('.frame')),
					rule: parseFloat(getComputedStyle(g.querySelector('.frame')).borderLeftWidth)
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
	test('one entry tall: the section holds an entry’s lines, its frames and a line of its data, in type smaller than the input’s', async () => {
		await withApp(async (app) => {
			const { page } = app;
			await app.submit('occur coffee cups=2');
			await page.locator(`${DOC} .rows`).waitFor();
			const m = await page.locator(`${BAR} .section.output`).evaluate((out) => {
				const roll = out.querySelector('.roll');
				const group = out.querySelector('.group:last-child');
				const box = (s) => group.querySelector(s).getBoundingClientRect();
				const style = getComputedStyle(roll);
				return {
					line: parseFloat(getComputedStyle(group.querySelector('.info')).lineHeight),
					room: roll.clientHeight - parseFloat(style.paddingTop) - parseFloat(style.paddingBottom),
					// Its lines: the time, the field and state (da-hbpkw).
					lines: box('.doc .what').height + box('.doc .when').height,
					// What is drawn, from the top of the command on the frame's
					// border to the foot of the time on its bottom one, and
					// where the entry's frame cuts what it holds.
					drawn: box('.stamp').bottom - box('.echo').top,
					data: box('.doc .rows').bottom,
					cut: box('.docs').bottom,
					font: parseFloat(getComputedStyle(group.querySelector('.doc')).fontSize),
					input: parseFloat(getComputedStyle(document.querySelector('.prompt-bar input')).fontSize)
				};
			});
			assert.ok(Math.abs(m.lines - 2 * m.line) <= 1, `the entry's lines are ${m.lines}px, not two of ${m.line}px`);
			assert.ok(m.data <= m.cut + 0.5, `the line of data is cut: ${JSON.stringify(m)}`);
			// And not much more: less than a line spare.
			assert.ok(m.room - m.drawn < m.line, `the section holds ${m.room}px for a ${m.drawn}px entry`);
			assert.ok(m.font < m.input, `the output's type is ${m.font}px against the input's ${m.input}px`);
			assert.deepEqual(app.errors, []);
		}, PHONE);
	});

	test('an entry’s time is a line of its own, over its field and state, as the CLI cut it', async () => {
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
			// vividn, 2026-10-31: «let's have the time be above the data again, just like in the CLI» (da-hbpkw).
			assert.ok(parts.when.bottom <= parts.what.top + 0.5, `the time is not over the field: ${JSON.stringify(parts)}`);
			assert.ok(Math.abs(parts.when.left - parts.what.left) <= 0.5, `the two lines do not start together: ${JSON.stringify(parts)}`);
			assert.deepEqual(app.errors, []);
		}, PHONE);
	});

	// da-nafql. vividn, 2026-10-30: «For other actions besides CREATE, keep using
	// the output actions that were there before», and «Let's have UNDONE actually
	// show what is changing». Each undo kind by what it draws, none like another.
	test('only a new entry is labelled with its shape, and each UNDONE and REVERT draws what it changes', async () => {
		await desktop(async (app) => {
			const { page } = app;
			const run = async (line) => {
				await app.submit(line);
				return page
					.locator(DOC)
					.last()
					.evaluate((el) => {
						const struck = (q) => getComputedStyle(el.querySelector(q)).textDecorationLine === 'line-through';
						return {
							hid: el.dataset.hid,
							word: el.querySelector('.action').innerText,
							time: el.querySelector('.when').textContent,
							data: el.querySelector('.rows .row').innerText,
							struck: [struck('.info'), struck('.rows')]
						};
					});
			};
			const plain = [false, false];

			// Not the field's first entry, whose bare spec goes and comes back with it.
			await app.submit('occur tea -t -50');
			let got = await run('occur tea cups=2 shop=corner');
			assert.deepEqual([got.word, got.struck], ['MOMENT', plain]);

			got = await run('update _LAST cups=3');
			assert.deepEqual([got.word, got.struck], ['UPDATE', plain]);
			assert.match(got.data, /^cups: 2 → 3/);

			got = await run('undo');
			assert.deepEqual([got.word, got.struck], ['REVERT', plain]);
			assert.match(got.data, /^cups: 2 ← 3/);

			await app.submit('redo');
			got = await run('delete _LAST');
			assert.deepEqual([got.word, got.struck], ['DELETE', plain]);

			got = await run('undo');
			assert.deepEqual([got.word, got.struck], ['REVERT', plain]);
			assert.match(got.time, /^\+ /);
			assert.equal(got.data, '+ cups: 3, + shop: "corner"');

			await app.submit('undo');
			got = await run('undo');
			assert.deepEqual([got.word, got.struck], ['UNDONE', [true, true]]);
			assert.match(got.time, /^− /);
			assert.equal(got.data, '− cups: 2, − shop: "corner"');
			// The log has every key on its own row.
			assert.deepEqual(await page.locator(`.cli .doc.gone[data-hid="${got.hid}"] .row`).allInnerTexts(), ['− cups: 2', '− shop: "corner"']);
		});
	});

	// da-hxjrk. vividn, 2026-10-29: «for each entry, give it a smaller frame
	// inside the cli commands frame. Label this frame with the action and the
	// hid … then put field/state indicator on the first line, time on the
	// second line, extra data below that.» The label is the shape the entry
	// STORES, which the CLI names (`shape`), so `end` is an ON/OFF as `start` is.
	test('each entry is a frame labelled with the shape it stores and its hid, its time over its field and state over its data, in the strip and the log', async () => {
		await desktop(async (app) => {
			const { page } = app;
			const shapes = [
				['occur coffee cups=2 -t -50', 'MOMENT'],
				['start work -t -40', 'ON/OFF'],
				['end work -t -30', 'ON/OFF'],
				['switch mood happy -t -20', 'CHANGE'],
				['switch mood calm dur=0 -t -10', 'MOMENT'],
				['switch mood sad 5m -t -5', 'PERIOD'],
				['add book dune title=x', 'RECORD']
			];
			const read = (doc) =>
				doc.evaluate((el) => {
					const box = (s) => el.querySelector(s)?.getBoundingClientRect();
					const frame = el.getBoundingClientRect();
					const label = box('.receipt');
					return {
						hid: el.dataset.hid,
						label: el.querySelector('.receipt').textContent.trim(),
						onBorder: label.top < frame.top && label.bottom > frame.top,
						lines: ['.when', '.what', '.rows'].map(box).filter(Boolean).map((r) => [r.top, r.bottom])
					};
				});
			const strip = [];
			for (const [line] of shapes) {
				await app.submit(line);
				const group = page.locator(OUT).filter({ has: page.locator('.echo', { hasText: line }) });
				strip.push(await read(group.locator('.doc')));
			}
			const log = await page.locator('.cli .doc').evaluateAll((els) => els.map((el) => el.dataset.hid));
			for (const [i, [line, shape]] of shapes.entries()) {
				const logged = await read(page.locator(`.cli .doc[data-hid="${strip[i].hid}"]`));
				assert.ok(log.includes(strip[i].hid), `${line}: not in the log`);
				for (const [where, got] of [['strip', strip[i]], ['log', logged]]) {
					assert.equal(got.label, `${shape} · ${got.hid}`, `${where}: ${line}`);
					assert.ok(got.onBorder, `${where}: ${line}'s label is not on its frame's border`);
					for (const [above, below] of got.lines.slice(1).map((r, j) => [got.lines[j], r])) {
						assert.ok(above[1] <= below[0] + 0.5, `${where}: ${line}'s lines are out of order: ${JSON.stringify(got.lines)}`);
					}
				}
			}
			assert.deepEqual(app.errors, []);
		});
	});

	// vividn, 2026-10-31: «move the time and the undo button to the bottom,
	// and move the cli command into the frame on the top, with a smaller font
	// size, then that gives more rooms for the entry blocks. The ACTION verb
	// needs to be bigger so that it is clearly visible. let's have the time be
	// above the data again, just like in the CLI.» (da-hbpkw)
	for (const [name, screen] of [['a phone', PHONE], ['a desktop', DESKTOP]]) test(`on ${name}, a block on either surface: the command on the top border in smaller type, the entry’s time over its data, the run time on the bottom border, the verb as big as the data`, async () => {
		await withApp(async (app) => {
			const { page } = app;
			await app.submit('occur coffee cups=2');
			await page.locator(`${OUT} .step button`).waitFor();
			await page.locator('.cli .entry:last-child .step button').waitFor();
			for (const surface of [OUT, '.cli .entry:last-child']) {
				const m = await page.locator(surface).evaluate((el) => {
					const r = (s) => el.querySelector(s).getBoundingClientRect();
					const size = (s) => parseFloat(getComputedStyle(el.querySelector(s)).fontSize);
					const box = (b) => ({ top: b.top, bottom: b.bottom, left: b.left, right: b.right });
					return {
						frame: box(r('.frame')),
						echo: box(r('.echo')),
						stamp: box(r('.stamp')),
						doc: box(r('.doc')),
						when: box(r('.doc .when')),
						what: box(r('.doc .what')),
						data: box(r('.doc .rows')),
						echoFont: size('.echo'),
						dataFont: size('.doc .what'),
						verbFont: size('.action'),
						hidFont: size('.hid')
					};
				});
				const say = `${surface}: ${JSON.stringify(m)}`;
				assert.ok(m.echo.top < m.frame.top && m.echo.bottom > m.frame.top, `the command is not on the top border: ${say}`);
				assert.ok(m.echo.left >= m.frame.left && m.echo.right <= m.frame.right, `the command is out of its frame: ${say}`);
				assert.ok(m.echo.bottom <= m.doc.top, `the command is not above the entry: ${say}`);
				assert.ok(m.echoFont < m.dataFont, `the command's type is not smaller than the data's: ${say}`);
				assert.ok(m.stamp.top < m.frame.bottom && m.stamp.bottom > m.frame.bottom, `the run time is not on the bottom border: ${say}`);
				assert.ok(m.stamp.top >= m.doc.bottom - 0.5, `the run time is not below the data: ${say}`);
				assert.ok(m.when.bottom <= m.what.top + 0.5 && m.what.bottom <= m.data.top + 0.5, `the entry's lines are out of order: ${say}`);
				assert.ok(m.verbFont >= m.dataFont && m.verbFont > m.hidFont, `the verb is small: ${say}`);
			}
			assert.deepEqual(app.errors, []);
		}, screen);
	});

	// vividn, on #1517: «move the undo button inside the outer frame on the
	// left … push the entry frames slightly to the right» (da-hbpkw). The hit
	// box is read off the page, by what a press at each point lands on.
	for (const [name, screen] of [['a phone', PHONE], ['a desktop', DESKTOP]]) test(`on ${name}, the undo stands in the frame left of the entries, as tall as a finger and over no word of the entry or the command`, async () => {
		await withApp(async (app) => {
			const { page } = app;
			await app.submit('occur coffee beans=ethiopian grind=fine water=94C');
			await page.locator(`${OUT} .step button`).waitFor();
			await page.locator('.cli .entry:last-child .step button').waitFor();
			for (const surface of [OUT, '.cli .entry:last-child']) {
				const m = await page.locator(surface).evaluate(async (el) => {
					const r = (s) => el.querySelector(s).getBoundingClientRect();
					const button = el.querySelector('.step button');
					const near = r('.frame');
					const hits = [];
					for (let y = Math.floor(near.top) - 10; y <= near.bottom + 10; y++) {
						for (let x = Math.floor(near.left) - 10; x <= near.right + 10; x++) {
							if (button.contains(document.elementFromPoint(x, y))) hits.push([x, y]);
						}
					}
					const xs = hits.map(([x]) => x);
					const ys = hits.map(([, y]) => y);
					const words = ['.echo', '.doc .when', '.doc .what', '.doc .rows', '.receipt'].map((s) => {
						const range = document.createRange();
						range.selectNodeContents(el.querySelector(s));
						const b = range.getBoundingClientRect();
						return { s, left: b.left, right: b.right, top: b.top, bottom: b.bottom };
					});
					// The full id, the entry's label at its widest: a press on it is the hid's.
					const hid = el.querySelector('.doc .hid');
					hid.click();
					await new Promise((done) => requestAnimationFrame(() => requestAnimationFrame(done)));
					const h = hid.getBoundingClientRect();
					const pressed = [h.left + h.width / 2, h.right - 2].map((x) => hid.contains(document.elementFromPoint(x, h.top + h.height / 2)));
					hid.click();
					return {
						frame: { left: near.left, top: near.top, bottom: near.bottom },
						button: { left: r('.step button').left, right: r('.step button').right },
						doc: r('.doc').left,
						hit: { left: Math.min(...xs), right: Math.max(...xs) + 1, top: Math.min(...ys), bottom: Math.max(...ys) + 1 },
						words,
						pressed
					};
				});
				const say = `${surface}: ${JSON.stringify(m)}`;
				assert.ok(m.button.left >= m.frame.left - 2 && m.button.right <= m.doc, `the undo is not in the frame, left of the entry: ${say}`);
				assert.ok(m.hit.right - m.hit.left >= 24 && m.hit.bottom - m.hit.top >= 44, `the undo's hit box is too small: ${say}`);
				const over = m.words.filter((t) => t.left < m.hit.right && t.right > m.hit.left && t.top < m.hit.bottom && t.bottom > m.hit.top);
				assert.deepEqual(over, [], `the undo's hit box lies over words: ${say}`);
				assert.deepEqual(m.pressed, [true, true], `a press on the full id is not the hid's: ${say}`);
			}
			assert.deepEqual(app.errors, []);
		}, screen);
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
				const animate = (window.__animate = Element.prototype.animate);
				Element.prototype.animate = function (frames, options) {
					const slide = animate.call(this, frames, { ...options, duration: 60000 });
					if (this.classList.contains('track')) window.__played.push(slide);
					return slide;
				};
			});
			let state;
			try {
				await app.submit('occur bread');
				await page.waitForFunction(() => window.__played.length === 1);
				await page.waitForTimeout(500);
				state = await page.evaluate(() => window.__played[0].playState);
			} finally {
				// The app is shared: left stretched, the next test's slides
				// each took a minute, and its `strip` waited them out (da-5rpcr),
				// so this runs however the test above it ends (da-roixa).
				await page.evaluate(() => {
					for (const slide of window.__played) slide.cancel();
					Element.prototype.animate = window.__animate;
				});
			}
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

	test('a command’s documents stand side by side in one frame, apart from the next, whose time is at the top', async () => {
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
			for (const doc of both.docs) assert.ok(doc.left >= both.frame.left && doc.right <= both.frame.right, 'a document is outside its frame');
			assert.ok(both.docs[1].left >= both.docs[0].right, 'the documents are not side by side');
			// The boundary: the next command's frame, past the last of this
			// one's documents, and its time at the top of the row beside it.
			assert.equal(next.line, 'occur water');
			assert.ok(next.rule >= 1, 'no frame around the next command');
			assert.ok(next.frame.left > both.frame.right, 'the two frames touch');
			assert.ok(next.left > both.docs[1].right, 'the next group begins inside this one');
			assert.ok(next.head >= 0 && next.head < 8, `the next command's head is not at the top: ${next.head}`);
			assert.deepEqual(app.errors, []);
		});
	});

	// vividn, 2026-11-09: «logging something or doing something that causes a
	// new CLI entry to be made should scroll the output section over to the far
	// right (if you've panned back into the past) so that you can see the new
	// entry come in» (da-4fy4c).
	test('scrolled back, the older commands mount and the reader stays put; a new command takes the strip to its end, with its slide', async () => {
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

			// Back to the start of what is mounted, as a finger or a wheel would scroll: more
			// mount in front of it, and the group the reader was at stays where it was.
			const roll = page.locator(`${BAR} .roll`);
			await strip(page); // at rest: the last arrival's slide is a transform
			const held = await roll.evaluate(async (el) => {
				const count = () => el.querySelectorAll('.group').length;
				const was = count();
				el.scrollTo({ left: 0 });
				const first = el.querySelector('.group');
				const left = first.getBoundingClientRect().left;
				for (let i = 0; i < 120 && count() === was; i += 1) await new Promise((r) => requestAnimationFrame(r));
				await new Promise((r) => requestAnimationFrame(() => requestAnimationFrame(r)));
				return { grew: count() - was, moved: first.getBoundingClientRect().left - left };
			});
			assert.ok(held.grew > 0, 'scrolling back mounted nothing');
			assert.ok(Math.abs(held.moved) <= 1, `the reader was moved ${held.moved}px as older commands mounted`);
			for (let i = 0; i < 10 && (await strip(page)).groups[0].line !== 'occur coffee'; i += 1) {
				await roll.evaluate((el) => el.scrollTo({ left: 0 }));
				await page.waitForTimeout(100);
			}
			await roll.evaluate((el) => el.scrollTo({ left: 0 }));
			await page.waitForFunction(() => document.querySelector('.prompt-bar .roll').scrollLeft === 0);
			const back = await strip(page);
			assert.deepEqual(back.groups.map((g) => g.line), lines);
			assert.ok(inView(back, back.groups[0]) && !back.atEnd, 'scrolling back did not reach the first command');

			await watchSlides(page);
			await app.submit('occur milk');
			await page.waitForFunction(() => document.querySelector('.prompt-bar .group:last-child .echo').innerText.includes('milk'));
			const s = await strip(page);
			assert.equal(s.groups.at(-1).line, 'occur milk');
			assert.ok(s.atEnd && inView(s, s.groups.at(-1)), 'scrolled back, the new command is out of view');
			assert.equal((await page.evaluate(() => window.__slides)).length, 1, 'the arrival did not slide in');

			// And it is pinned there: the next command lands in view too.
			await app.submit('occur egg');
			await page.waitForFunction(() => document.querySelector('.prompt-bar .group:last-child .echo').innerText.includes('egg'));
			const next = await strip(page);
			assert.ok(next.atEnd && inView(next, next.groups.at(-1)), 'after the jump, the next command is out of view');
			assert.deepEqual(app.errors, []);
		}, PHONE);
	});

	// vividn, 2026-10-28: «dragging the collapsed view further to the left
	// should eventually show an x icon with clear written under it which
	// clears the scrollback» (da-3eczg), and 2026-10-29: «It should be similar
	// to the browser pull down to refresh action, where an X in a circle slowly
	// comes out until a certain threshold after which the circle becomes bright
	// and then it clears on let go» (da-uqx7n). A real finger, through CDP: the
	// strip scrolls natively, and only a real touch is what the browser pans with.
	test('a pull from rest at the newest brings out an × in a circle, bright past 200px, and a lift clears only then; a swipe that arrives at the end clears nothing', async () => {
		await withApp(async (app) => {
			const { page } = app;
			const commands = async () => {
				for (const line of ['occur coffee', 'occur water', 'occur tea', 'occur toast', 'occur juice']) await app.submit(line);
				assert.ok((await strip(page)).atEnd, 'the strip is not at its newest end');
			};
			await commands();
			const tail = (await app.run(['tail'])).stdout;
			const echoes = await page.locator('.echo').count();
			assert.ok(echoes >= 5, `only ${echoes} commands on the page`);

			const roll = page.locator(`${BAR} .roll`);
			const box = await roll.boundingBox();
			const [x, y] = [box.x + box.width - 30, box.y + box.height / 2];
			const cdp = await page.context().newCDPSession(page);
			const touch = (type, dx) =>
				cdp.send('Input.dispatchTouchEvent', { type, touchPoints: dx === undefined ? [] : [{ id: 0, x: x + dx, y }] });
			/** One finger down at x, then moved left in ten steps to x + dx. */
			const drag = async (dx) => {
				await touch('touchStart', 0);
				for (let i = 1; i <= 10; i += 1) await touch('touchMove', (dx * i) / 10);
			};
			/** Whether the strip has come off any of the ×, whether the × stands whole beside it, and its circle,
			 *  two frames on: a touch reaches the page a frame after CDP has sent it. */
			const shown = () =>
				page.evaluate(async () => {
					await new Promise((done) => requestAnimationFrame(() => requestAnimationFrame(done)));
					const button = document.querySelector('.prompt-bar .clear-strip');
					const section = button.closest('.section').getBoundingClientRect();
					const b = button.getBoundingClientRect();
					const roll = button.parentElement.querySelector('.roll').getBoundingClientRect();
					return {
						pulled: roll.right < button.parentElement.getBoundingClientRect().right - 0.5,
						whole: b.left >= section.left && b.right <= section.right + 0.5 && roll.right <= b.left + 0.5,
						text: button.innerText,
						bright: button.classList.contains('bright'),
						opacity: Number(getComputedStyle(button).opacity),
						size: button.querySelector('span').getBoundingClientRect().width
					};
				});
			const sprungBack = () =>
				page.waitForFunction(() => getComputedStyle(document.querySelector('.prompt-bar .roll')).transform === 'none');

			assert.equal((await shown()).pulled, false, 'at rest, part of the × shows past the strip');

			// 1. One swipe from short of the newest that reaches the end and goes on far past the threshold: nothing.
			await roll.evaluate((el) => el.scrollTo({ left: el.scrollWidth - el.clientWidth - 100 }));
			await page.waitForFunction(() => {
				const el = document.querySelector('.prompt-bar .roll');
				return el.scrollLeft + el.clientWidth < el.scrollWidth - 50;
			});
			await drag(-340);
			assert.ok((await strip(page)).atEnd, 'the swipe did not reach the newest end');
			assert.equal((await shown()).bright, false, 'a swipe that arrived at the end armed the pull');
			await touch('touchEnd');
			await sprungBack();
			assert.equal(await page.locator('.echo').count(), echoes, 'a swipe that only arrived at the end cleared the scrollback');

			// 4. From rest at the end, the circle grows and fades in with the pull, and turns bright exactly at 200px.
			await touch('touchStart', 0);
			const at = {};
			for (const dx of [-40, -100, -160, -199, -200]) {
				await touch('touchMove', dx);
				at[dx] = await shown();
			}
			assert.ok(at[-40].pulled && !at[-40].whole, `a short pull: ${JSON.stringify(at[-40])}`);
			assert.ok(at[-40].opacity < at[-100].opacity && at[-100].opacity < at[-160].opacity, `the circle does not fade in: ${JSON.stringify(at)}`);
			assert.ok(at[-40].size < at[-100].size && at[-100].size < at[-160].size, `the circle does not grow: ${JSON.stringify(at)}`);
			assert.equal(at[-199].bright, false, 'bright short of the threshold');
			assert.ok(at[-200].bright && at[-200].whole && at[-200].opacity === 1, `not bright and whole at the threshold: ${JSON.stringify(at[-200])}`);
			assert.match(at[-200].text, /×\s*clear/);

			// 3. Brought back under the threshold before the lift, it dims, and the lift clears nothing.
			await touch('touchMove', -260);
			assert.ok((await shown()).bright, 'not bright past the threshold');
			await touch('touchMove', -150);
			assert.equal((await shown()).bright, false, 'still bright back under the threshold');
			await touch('touchEnd');
			await sprungBack();
			assert.equal(await page.locator('.echo').count(), echoes, 'a pull let go under the threshold cleared something');

			// A second finger landing mid-pull calls the gesture off, however far it had gone (canary on #1350).
			await drag(-240);
			const both = (type, dx) =>
				cdp.send('Input.dispatchTouchEvent', { type, touchPoints: [{ id: 0, x: x - 240 + dx, y }, { id: 1, x: x - 100 + dx, y: y - 10 }] });
			await both('touchStart', 0);
			await both('touchMove', -10);
			await touch('touchEnd');
			await sprungBack();
			assert.equal(await page.locator('.echo').count(), echoes, 'a pull with a second finger on it cleared the scrollback');

			// 2. From rest at the end, past the threshold: bright, and the lift clears the scrollback and not the data.
			// The two fingers scrolled the strip off its end; a pull starts only from rest there.
			await roll.evaluate((el) => el.scrollTo({ left: el.scrollWidth }));
			await page.waitForFunction(() => {
				const el = document.querySelector('.prompt-bar .roll');
				return el.scrollLeft + el.clientWidth >= el.scrollWidth - 2;
			});
			await drag(-240);
			assert.ok((await shown()).bright, 'not bright past the threshold');
			await touch('touchEnd');
			await page.waitForFunction(() => !document.querySelector('.echo'));
			assert.equal(await page.locator(`${BAR} .group`).count(), 0, 'the strip still holds commands');
			assert.equal((await app.run(['tail'])).stdout, tail, 'clearing the scrollback changed the data');
			await sprungBack();

			// Reduced motion: no spring, and the × is there, bright, at the threshold or not at all.
			await page.emulateMedia({ reducedMotion: 'reduce' });
			await commands();
			await drag(-160);
			assert.equal((await shown()).pulled, false, 'reduced motion pulled the strip short of the threshold');
			await touch('touchEnd');
			await drag(-240);
			const still = await shown();
			assert.ok(still.whole && still.bright, `reduced motion: the × is not shown bright past the threshold: ${JSON.stringify(still)}`);
			await touch('touchEnd');
			await page.waitForFunction(() => !document.querySelector('.echo'));

			// A keyboard: focused, the × is shown, and pressed, it clears.
			await commands();
			await page.locator(`${BAR} .clear-strip`).focus();
			assert.ok((await shown()).whole, 'focused, the × is not shown');
			await page.keyboard.press('Enter');
			await page.waitForFunction(() => !document.querySelector('.echo'));
			await sprungBack();
			await cdp.detach();
			assert.deepEqual(app.errors, []);
		}, PHONE);
	});

	// vividn, 2026-10-28: «put undo buttons on the entries that cause changes in
	// the database. And when something is undone, rather than displaying the
	// delete/change as a new action, just grey/hash out the command that was
	// undone … put a redo button where the undo button was», and «disable the
	// undo button if future writes bury the change … "cannot undo. more recent
	// changes" more recent changes is a link, which takes you to the latest
	// entry that buries it» (da-25q3k).
	test('a track and an ignore wrote, so each has an undo that takes its rows back; a get of a TRACK row wrote nothing', async () => {
		await withApp(async (app) => {
			const { page } = app;
			const group = (line) => page.locator(`${BAR} .group`).filter({ has: page.locator('.echo', { hasText: line }) });
			const held = async () => (await app.run(['tail', 'tea', '--json'])).stdout;

			await app.submit('ignore tea');
			await group('ignore tea').getByRole('button', { name: 'undo' }).waitFor();
			await app.clock.forward(1000);
			await app.submit('track tea --verb occur');
			const undo = group('track tea').getByRole('button', { name: 'undo' });
			await undo.waitFor();
			await app.clock.forward(1000);
			await app.submit('get tea');
			await group('get tea').locator('.doc').first().waitFor();
			const got = JSON.parse((await page.evaluate(() => window.datum.bar.last().result.stdout)).trim().split('\n').at(-1)).docs;
			assert.equal(got.at(-1).action, 'TRACK●', 'nothing here tells a get of a TRACK row from its track');
			assert.equal(await group('get tea').locator('.step').count(), 0, 'a get of a TRACK row has an undo button');

			assert.match(await held(), /TRACKED/);
			// Once on CI the struck head never came, and nothing said why (da-2cuyi). Each way it
			// can fail leaves its own trace: the click landing off the button (Playwright checks
			// the target at mousedown alone), the step refused (`ours` read no history as ours),
			// or the revert run with no mark (the bar's last command).
			const clicked = await page.evaluateHandle(() => {
				const on = [];
				addEventListener('click', (e) => on.push(e.target.outerHTML.slice(0, 80)), true);
				return on;
			});
			await undo.click();
			await group('track tea').locator('.head.struck').waitFor().catch(async (e) => {
				const why = await group('track tea').locator('.step').evaluate((step, clicked) => ({
					clicked,
					step: step.textContent,
					disabled: step.querySelector('button').ariaDisabled,
					last: window.datum.bar.last().line
				}), clicked);
				throw new Error(`undone, the head is not struck: ${JSON.stringify(why)}`, { cause: e });
			});
			assert.doesNotMatch(await held(), /TRACKED/, 'undone, the TRACK row is still there');
			assert.doesNotMatch((await app.run(['setup', 'tea', '--json'])).stdout, /"verb"/, 'undone, the spec is still there');
			assert.deepEqual(app.errors, []);
		});
	});

	test('undo hashes a command out where it stands and redo puts it back; a buried one says why and goes to what buried it; a reload keeps the marks', async () => {
		await withApp(async (app) => {
			const { page } = app;
			const group = (line) => page.locator(`${BAR} .group`).filter({ has: page.locator('.echo', { hasText: line }) });
			const hidOf = async () =>
				JSON.parse((await page.evaluate(() => window.datum.bar.last().result.stdout)).trim().split('\n').at(-1)).docs[0].hid;
			const listed = async (hid) => (await app.run(['ids'])).stdout.includes(hid);
			const struck = (line) => group(line).locator('.head.struck');

			await app.submit('occur coffee');
			const coffee = await hidOf();
			await app.submit('tail');
			assert.equal(await group('❯ tail').locator('.step').count(), 0, 'a read has an undo button');
			const groups = await page.locator(`${BAR} .group`).count();

			const arrow = await group('occur coffee').getByRole('button', { name: 'undo' }).innerText();
			await group('occur coffee').getByRole('button', { name: 'undo' }).tap();
			await struck('occur coffee').waitFor();
			assert.equal(await listed(coffee), false, 'undone, the entry is still in the database');
			// The row shows what the undo answered, struck through, under its other arrow (da-hbpkw).
			const row = group('occur coffee');
			await row.locator('.doc .action[title="UNDONE"]').waitFor();
			assert.equal(await row.locator('.doc.gone .what').evaluate((el) => getComputedStyle(el.parentElement).textDecorationLine), 'line-through');
			assert.equal(await row.locator('.doc').getAttribute('data-hid'), coffee);
			assert.equal(await page.locator(`${BAR} .group`).count(), groups, 'the undo is a command of its own in the strip');
			// The CLI widget is the log's own component: there, the undo is what the CLI printed, and the command it undid is struck too.
			await page.locator('.host.cli .echo', { hasText: `❯ revert ${coffee}` }).waitFor();
			// What it did (da-nafql).
			await page.locator('.host.cli .doc .action[title="UNDONE"]').first().waitFor();
			assert.equal(await page.locator('.host.cli .head.struck').count(), 1);

			assert.notEqual(await row.getByRole('button', { name: 'redo' }).innerText(), arrow, 'redo wears undo\'s arrow');
			await group('occur coffee').getByRole('button', { name: 'redo' }).tap();
			await page.waitForFunction(() => !document.querySelector('.prompt-bar .head.struck'));
			assert.ok(await listed(coffee), 'redone, the entry is not back');
			assert.equal(await row.locator('.doc .action').getAttribute('title'), 'MOMENT', 'redone, the row still answers as undone');
			assert.equal(await group('occur coffee').getByRole('button', { name: 'undo' }).count(), 1);

			// Buried: a later command wrote the same entry, far enough along the strip to be out of view.
			await app.submit('occur tea');
			const tea = await hidOf();
			await app.submit('occur toast');
			await app.submit('occur juice');
			const juice = await hidOf();
			for (const line of ['occur water', `update ${tea} cups=2`, 'tail']) await app.submit(line);
			await page.locator(`${BAR} .roll`).evaluate(async (roll) => {
				// The strip mounts its older groups as it is scrolled to them.
				for (let i = 0; i < 20 && !/occur tea/.test(roll.innerText); i += 1) {
					roll.scrollLeft = 0;
					await new Promise((r) => requestAnimationFrame(() => requestAnimationFrame(r)));
				}
			});
			await group('occur tea').evaluate((g) => g.scrollIntoView({ inline: 'start' }));
			const inView = (line) =>
				group(line).evaluate((g) => {
					const roll = g.closest('.roll').getBoundingClientRect();
					const box = g.getBoundingClientRect();
					return box.left >= roll.left - 1 && box.right <= roll.right + 1;
				});
			assert.equal(await inView(`update ${tea}`), false, 'the burying command is in view already');
			const buried = group('occur tea').getByRole('button', { name: 'undo' });
			assert.equal(await buried.getAttribute('aria-disabled'), 'true');
			// Playwright refuses an `aria-disabled` control; the page does not, and says why.
			// Once on CI the note was never told, and nothing said why (da-jlw07). Each way it can
			// fail leaves its own trace: the tap landing off the button or making no click (a
			// forced tap checks no target), a second click telling it away again, the step no
			// longer refused (it ran, `last`), or the strip moved off the group (`inView`).
			const pressed = await page.evaluateHandle(() => {
				const on = [];
				for (const type of ['touchstart', 'click'])
					addEventListener(type, (e) => on.push(`${type} ${e.pointerType ?? ''} ${e.target.outerHTML.slice(0, 60)}`), true);
				return on;
			});
			await buried.tap({ force: true });
			const note = group('occur tea').locator('.buried.told');
			await note.waitFor().catch(async (e) => {
				const why = await group('occur tea').locator('.step').evaluate((step, pressed) => ({
					pressed,
					step: step.textContent,
					disabled: step.querySelector('button').ariaDisabled,
					note: step.querySelector('.buried')?.className ?? null,
					last: window.datum.bar.last().line
				}), pressed);
				throw new Error(`pressed, the buried note is not told: ${JSON.stringify({ ...why, inView: await inView('occur tea') })}`, { cause: e });
			});
			assert.match(await note.innerText(), /^cannot undo\. more recent changes$/);
			assert.ok(await listed(tea), 'a buried undo undid');
			await note.getByRole('button', { name: 'more recent changes' }).tap();
			await page.waitForFunction(
				(line) => {
					const g = [...document.querySelectorAll('.prompt-bar .group')].find((g) => g.querySelector('.echo').textContent.includes(line));
					const roll = g.closest('.roll').getBoundingClientRect();
					const box = g.getBoundingClientRect();
					return box.left >= roll.left - 1 && box.right <= roll.right + 1;
				},
				`update ${tea}`
			);

			// Changed where the page's scrollback cannot see — another tab, the terminal, a synced device:
			// the press reads the entry's trail, runs nothing, and says so, with nothing to link to (vremya on #1362).
			await app.run(['update', juice, 'cups=3']);
			const commands = await page.locator('.host.cli .echo').count();
			await group('occur juice').getByRole('button', { name: 'undo' }).tap();
			const elsewhere = group('occur juice').locator('.buried.told');
			await elsewhere.waitFor();
			assert.equal(await elsewhere.innerText(), 'cannot undo. a change made elsewhere');
			assert.equal(await group('occur juice').getByRole('button', { name: 'undo' }).getAttribute('aria-disabled'), 'true');
			assert.equal(await page.locator('.host.cli .echo').count(), commands, 'a refused undo ran a command');
			assert.match(JSON.parse((await app.run(['history', juice, '--json'])).stdout)[0].raw, /cups=3/, 'the change made elsewhere was undone');

			// The marks are the scrollback's: undone, reloaded, still hashed out with redo, and the buried one still refused.
			await group('occur water').getByRole('button', { name: 'undo' }).tap();
			await struck('occur water').waitFor();
			await app.reload();
			await page.locator(`${BAR} .roll`).evaluate(async (roll) => {
				for (let i = 0; i < 20 && !/occur tea/.test(roll.innerText); i += 1) {
					roll.scrollLeft = 0;
					await new Promise((r) => requestAnimationFrame(() => requestAnimationFrame(r)));
				}
			});
			await struck('occur water').waitFor();
			assert.equal(await group('occur water').getByRole('button', { name: 'redo' }).count(), 1);
			assert.equal(await group('occur tea').getByRole('button', { name: 'undo' }).getAttribute('aria-disabled'), 'true');
			assert.equal(await page.locator(`${BAR} .group .echo`, { hasText: /❯ (undo|redo|revert) / }).count(), 0, 'a step is a command in the strip');
			assert.deepEqual(app.errors, []);
		}, PHONE);
	});
});
