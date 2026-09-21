// TWO ENTRIES SIDE BY SIDE ON A PHONE, AND THE MEASUREMENTS THAT SAY SO
// (da-5iyo).
//
// vividn, 2026-09-21, reviewing PR 46: «ideally, but this can be another PR,
// the output of a single entry should be narrow enough to comfortably fit
// 2 on mobile side by side.»
//
// `$lib/scrollback.ts` answers that with two numbers, and a number in a
// file is only as good as the measurement behind it — a breakpoint chosen
// by eye reads exactly the same. So this file TAKES BOTH MEASUREMENTS
// AGAIN, against the built app, and holds the constants to what it finds:
//
// 1. `MIN` is above the width an entry's output starts being CUT at. The
//    column is narrowed a step at a time until something inside an entry
//    overflows its box, and the width that first does has to be under
//    `MIN` — with room to spare, so that a rewording of what datum echoes
//    cannot land on the number itself.
// 2. `WANT` is at or above the width an entry's lines ASK for. Measured
//    with one column in a wide box, where nothing wraps and a document's
//    lines are exactly as wide as they want to be.
//
// And then the claim itself: at the two real phone sizes the bead names —
// the root's declared minimum of 412×563 and the 390×844 the phone drives
// run at — the REPL draws TWO columns and cuts nothing; at a box too
// narrow for two it draws ONE and cuts nothing there either, which is the
// reading this started from, unchanged.
//
// `query: 'repl'` throughout: the one-pane layout is the REPL at the
// width of the screen, which is what makes a viewport a column count.

import test, { before, after, describe } from 'node:test';
import assert from 'node:assert/strict';
import { registerHooks } from 'node:module';
import { startApp, stopApp, withApp, todayAt } from './support/app.mjs';

registerHooks({
	resolve(specifier, context, next) {
		if (/^\.\/.*\.js$/.test(specifier) && context.parentURL?.includes('/src/lib/'))
			return next(specifier.replace(/\.js$/, '.ts'), context);
		return next(specifier, context);
	}
});

const { GAP, MIN, WANT } = await import('../src/lib/scrollback.ts');

before(startApp);
after(stopApp);

/**
 * The ordinary shapes an entry comes in: a switch with a trailing edge, a
 * bare occur, a block, an occur carrying data for the brace.
 *
 * These are what `WANT` is measured over. A field or a state long enough
 * can make an info line as wide as you like, and a column sized for the
 * longest name anyone might type would be a column nobody has room for —
 * so a long one WRAPS, which in the REPL costs nothing (Doc.svelte).
 */
const ORDINARY = [
	'switch mood happy',
	'occur coffee',
	'start work',
	'occur brew bean=ethiopian grind=fine water=94C'
];

/**
 * The scrollback the layout is read off: the ordinary shapes, a switch
 * whose state is far longer than any column, and a `tail`, whose answer is
 * a table the CLI laid out itself and not an action echo.
 */
const LINES = [...ORDINARY, 'switch project datum-web-columns', 'tail -n 3'];

const PHONES = [
	{ width: 390, height: 844 },
	{ width: 412, height: 563 }
];

/** Everything inside an entry that a narrow column could cut. */
const PIECES = '.receipt, .info, .row, .echo, .lines, .brace, .doc, .docs';

async function seed(app, lines = LINES) {
	for (const line of lines) await app.submit(line);
	await app.idle();
}

async function sized(page, viewport) {
	await page.setViewportSize(viewport);
	// Two frames: the REPL's own ResizeObserver fires on the frame after
	// the layout it observed, and the column count is what it carries.
	await page.evaluate(
		() => new Promise((r) => requestAnimationFrame(() => requestAnimationFrame(r)))
	);
}

/** The columns, the room they have, and anything cut inside one of them. */
const state = (page) =>
	page.evaluate((pieces) => {
		const grid = document.querySelector('.entries');
		const cells = [...document.querySelectorAll('.entry:not(.whole)')];
		const cut = [];
		for (const cell of cells)
			for (const el of cell.querySelectorAll(pieces))
				if (el.scrollWidth > el.clientWidth + 1)
					cut.push(`${el.className.split(' ')[0]} wants ${el.scrollWidth} in ${el.clientWidth}: ${el.textContent.trim().slice(0, 40)}`);
		return {
			columns: Number(grid?.dataset.columns),
			width: grid ? Math.round(grid.getBoundingClientRect().width) : 0,
			cell: cells.length ? Math.round(cells[0].getBoundingClientRect().width) : 0,
			cut
		};
	}, PIECES);

describe('the measurements the column width rests on', () => {
	test('an entry is cut well below the narrowest column it is ever given', async () => {
		await withApp(
			async (app) => {
				await seed(app);
				// A wide box, so that forcing the column count down to a
				// given width is the only thing narrowing an entry.
				await sized(app.page, { width: 1600, height: 900 });

				// The narrowest column that still cuts nothing, and the
				// widest that cuts something: walk the count up until the
				// entries start losing text.
				let whole = null;
				let cutAt = null;
				for (let columns = 5; columns <= 24 && cutAt === null; columns += 1) {
					await app.page.evaluate(
						(n) => document.querySelector('.entries').style.setProperty('--columns', String(n)),
						columns
					);
					const { cell, cut } = await state(app.page);
					if (cut.length) cutAt = cell;
					else whole = cell;
				}
				await app.page.evaluate(() =>
					document.querySelector('.entries').style.removeProperty('--columns')
				);

				assert.ok(cutAt !== null, `nothing was cut down to ${whole}px, so nothing was measured`);
				assert.ok(
					cutAt < MIN,
					`an entry is cut at ${cutAt}px, which MIN (${MIN}) does not clear`
				);
				// Not merely clear of it: a narrower column than `MIN` is
				// never drawn, so what this buys is the room for datum to
				// reword an echo without the floor moving under it.
				assert.ok(
					whole <= MIN,
					`the measurement never reached MIN (${MIN}): the narrowest column tried was ${whole}px`
				);
			},
			{ viewport: PHONES[0], query: 'repl', clock: todayAt(15) }
		);
	});

	test('an ordinary entry’s lines ask for WANT and not much less', async () => {
		await withApp(
			async (app) => {
				await seed(app, ORDINARY);
				await sized(app.page, { width: 1600, height: 900 });
				// One column in a box wider than any of these lines: each
				// one is as wide as it wants to be, so its box IS its
				// natural width (`.lines` grows for nothing).
				const wants = await app.page.evaluate(() => {
					const grid = document.querySelector('.entries');
					grid.style.setProperty('--columns', '1');
					const widths = [...document.querySelectorAll('.entry:not(.whole) .lines')].map((el) =>
						Math.ceil(el.getBoundingClientRect().width)
					);
					grid.style.removeProperty('--columns');
					return widths;
				});

				assert.ok(wants.length >= 4, `only ${wants.length} documents were measured`);
				const widest = Math.max(...wants);
				assert.ok(widest <= WANT, `an ordinary entry's lines want ${widest}px, more than WANT (${WANT})`);
				// And WANT is not merely generous enough: a constant far
				// past what anything asks for is a number nobody measured.
				// 32px is three characters of the monospace these lines are
				// set in, which is as close as a round number gets.
				assert.ok(
					WANT - widest < 32,
					`WANT is ${WANT} and the widest line asks for ${widest}: the number has come loose from the measurement`
				);
			},
			{ viewport: PHONES[0], query: 'repl', clock: todayAt(15) }
		);
	});
});

describe('two entries side by side on a phone', () => {
	for (const viewport of PHONES) {
		test(`${viewport.width}×${viewport.height} draws two columns and cuts nothing`, async () => {
			await withApp(
				async (app) => {
					await seed(app);
					await sized(app.page, viewport);
					const { columns, width, cell, cut } = await state(app.page);

					assert.equal(columns, 2, `a ${viewport.width}px phone drew ${columns} column(s)`);
					// The claim under the claim: the room really is there
					// for two columns of `MIN`, scrollbar and all.
					assert.ok(
						width >= 2 * MIN + GAP,
						`the pane has ${width}px, which is under two columns of ${MIN} and a ${GAP}px gap`
					);
					assert.ok(cell >= MIN, `a column came out ${cell}px, under MIN (${MIN})`);
					assert.deepEqual(cut, [], 'a phone column cut an entry');
					assert.deepEqual(app.errors, []);
				},
				{ viewport, query: 'repl', clock: todayAt(15) }
			);
		});
	}

	test('and really are two abreast: the second entry stands beside the first', async () => {
		await withApp(
			async (app) => {
				await seed(app);
				await sized(app.page, PHONES[0]);
				const [first, second] = await app.page.evaluate(() =>
					[...document.querySelectorAll('.entry:not(.whole)')].slice(0, 2).map((el) => {
						const { top, left } = el.getBoundingClientRect();
						return { top: Math.round(top), left: Math.round(left) };
					})
				);

				assert.equal(second.top, first.top, 'the second entry was drawn under the first');
				assert.ok(second.left > first.left, 'the second entry was not to the right of the first');
			},
			{ viewport: PHONES[0], query: 'repl', clock: todayAt(15) }
		);
	});
});

describe('the single column a narrower box keeps', () => {
	test('is one column, and cuts nothing either', async () => {
		await withApp(
			async (app) => {
				await seed(app);
				await sized(app.page, { width: 320, height: 700 });
				const { columns, width, cut } = await state(app.page);

				assert.ok(
					width < 2 * MIN + GAP,
					`320px gave the pane ${width}px, which is room for two after all`
				);
				assert.equal(columns, 1, `a 320px phone drew ${columns} columns`);
				assert.deepEqual(cut, [], 'the single column cut an entry');
				assert.deepEqual(app.errors, []);
			},
			{ viewport: { width: 320, height: 700 }, query: 'repl', clock: todayAt(15) }
		);
	});
});

describe('what the columns cannot carry', () => {
	test('a table takes the whole row, at the width the CLI printed it for', async () => {
		await withApp(
			async (app) => {
				await seed(app);
				await sized(app.page, PHONES[0]);
				const table = await app.page.evaluate(() => {
					const cells = [...document.querySelectorAll('.entry')];
					const whole = cells.filter((el) => el.classList.contains('whole'));
					const grid = document.querySelector('.entries');
					return {
						wholes: whole.length,
						echoes: whole.map((el) => el.querySelector('.echo')?.textContent.trim()),
						width: whole.length ? Math.round(whole[0].getBoundingClientRect().width) : 0,
						grid: Math.round(grid.getBoundingClientRect().width)
					};
				});

				// `tail -n 3` alone: every other line here answers with an
				// action echo, which is what a column carries.
				assert.equal(table.wholes, 1, `${table.wholes} entries took a whole row`);
				assert.equal(table.echoes[0], '❯ tail -n 3');
				assert.equal(table.width, table.grid, 'the table did not get the whole row');
			},
			{ viewport: PHONES[0], query: 'repl', clock: todayAt(15) }
		);
	});
});

describe('the hairline between commands, once there are columns', () => {
	test('stands between rows and never between two entries side by side', async () => {
		await withApp(
			async (app) => {
				await seed(app);
				await sized(app.page, PHONES[0]);
				const cells = await app.page.evaluate(() =>
					[...document.querySelectorAll('.entry')].map((el) => ({
						top: Math.round(el.getBoundingClientRect().top),
						ruled: getComputedStyle(el).borderTopWidth !== '0px'
					}))
				);

				assert.ok(cells.length >= 4, `only ${cells.length} entries were drawn`);
				// The first row is the pane's own top edge: no rule there
				// (a repl widget draws `skip` 0).
				const first = cells[0].top;
				for (const cell of cells)
					assert.equal(
						cell.ruled,
						cell.top !== first,
						`an entry at y=${cell.top} was ${cell.ruled ? '' : 'not '}ruled`
					);
				// And entries that share a row share the verdict, which is
				// what makes the row read as one rule with the gap in it.
				const rows = new Map();
				for (const cell of cells) rows.set(cell.top, (rows.get(cell.top) ?? 0) + 1);
				assert.ok([...rows.values()].some((n) => n > 1), 'no two entries shared a row');
			},
			{ viewport: PHONES[0], query: 'repl', clock: todayAt(15) }
		);
	});
});
