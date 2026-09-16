// One `<AnsiText>` for every span on screen (da-mv90).
//
// `$lib/ansi.ts` turns the CLI's escapes into spans, and the markup that
// PAINTS them used to be written out three times — `CliOutput` once,
// `TailTable` twice. The copies had already drifted: the tail's header cell
// set neither underline nor italic, its body cell no italic, so the same
// escape said different things in the REPL and in the table. One component
// now holds the union.
//
// What is asserted is what a reader would see, on both surfaces at once:
// the text a cell carries is the CLI's own cell with nothing added — no
// space between the spans, none before or after them, which is the way a
// shared component goes wrong — and the colour and weight a run is painted
// is the same number in the REPL's block as in the tail's table. The
// cross-surface comparison is the point: a component that lost an attribute
// would lose it in one place and the two would stop agreeing.

import test, { before, after, describe } from 'node:test';
import assert from 'node:assert/strict';
import { startApp, stopApp, withApp, todayAt } from './support/app.mjs';
import { stripAnsi } from './support/tail.mjs';

before(startApp);
after(stopApp);

const OK = 0;

/** Every painted run in the tail pane's table, by column, with its cell. */
const tailSpans = (page) =>
	page.evaluate(() => {
		const read = (el) => ({
			text: el.textContent,
			spans: [...el.querySelectorAll('span')].map((s) => {
				const style = getComputedStyle(s);
				return {
					text: s.textContent,
					color: style.color,
					background: style.backgroundColor,
					weight: style.fontWeight,
					decoration: style.textDecorationLine
				};
			})
		});
		const pane = document.querySelector('section.pane[data-title="tail"]');
		return {
			headers: [...pane.querySelectorAll('thead th')].map((th) => ({ key: th.dataset.key, ...read(th) })),
			rows: [...pane.querySelectorAll('tbody tr')].map((tr) =>
				[...tr.querySelectorAll('td')].map((td) => ({ key: td.dataset.key, ...read(td) }))
			)
		};
	});

/** The REPL's newest output block: its text, and every run in it. */
const replSpans = (page) =>
	page.evaluate(() => {
		const pre = [...document.querySelectorAll('.repl pre')].at(-1);
		return {
			text: pre.textContent,
			spans: [...pre.querySelectorAll('span')].map((s) => {
				const style = getComputedStyle(s);
				return {
					text: s.textContent,
					color: style.color,
					background: style.backgroundColor,
					weight: style.fontWeight,
					decoration: style.textDecorationLine
				};
			})
		};
	});

/** The backgrounds a set of runs paints, in order, transparent dropped. */
const painted = (spans) => spans.map((s) => s.background).filter((c) => c !== 'rgba(0, 0, 0, 0)');

/**
 * Three entries and a `tail` typed at the prompt, with the pane showing the
 * same rows: one row set, two surfaces, both on screen at once.
 *
 * Chosen so the listing carries every attribute datum actually writes.
 * The last entry is half an hour ahead, and a stamp in the future is the
 * one thing datum underlines (`render/action.rs`) — the attribute the
 * tail's header copy of the loop had already lost. It is also `mood happy`
 * a second time, a state repeat, whose lead slot is the bold `!` marker
 * (da-qfv0) — the only bold the CLI writes. Nothing datum prints is italic
 * or SGR-dim, so those two live in the component unexercised.
 */
/**
 * Mid-afternoon: 07:00 and 08:00 are behind the app's clock and only the
 * `+30` below is ahead of it. Unpinned, before 08:00 local all three
 * stamps are in the future and all three are underlined — the surfaces
 * agree with each other and the test still passes, having quietly stopped
 * being about the one underline it names (da-ldct).
 */
const AFTERNOON = todayAt(15);

async function twoSurfaces(app) {
	assert.equal((await app.run(['occur', 'coffee', '-t', '0700'])).code, OK);
	assert.equal((await app.run(['switch', 'mood', 'happy', '-t', '0800'])).code, OK);
	assert.equal((await app.run(['switch', 'mood', 'happy', '-t', '+30'])).code, OK);
	const listed = await app.run(['tail']);
	assert.equal(listed.code, OK, listed.stderr);
	await app.submit('tail');
	await app.idle();
	return {
		listed,
		tail: await tailSpans(app.page),
		repl: await replSpans(app.page),
		cli: await app.tailRows()
	};
}

describe('the spans one component paints', () => {
	test("carry the CLI's own cell text, and not one space more", async () => {
		await withApp(async (app) => {
			const { tail, cli } = await twoSurfaces(app);
			const column = (key) => cli.columns.find((c) => c.key === key);

			assert.ok(tail.rows.length > 0, 'the tail pane is showing no rows');
			for (const cell of [...tail.headers, ...tail.rows.flat()]) {
				// The spans and nothing else: a newline left between them in
				// the markup would arrive as a space in the DOM, and the
				// tail's columns and the REPL's output are monospace.
				assert.equal(
					cell.text,
					cell.spans.map((s) => s.text).join(''),
					`the ${cell.key} cell carries text outside its spans: ${JSON.stringify(cell.text)}`
				);
			}

			// And the text itself is the CLI's, stripped of its escapes. A
			// bare `trim()` would be the wrong test: the state chip's cell
			// really is ` happy ` — the LEAD and TRAIL slots are painted
			// spaces, the chip's own padding. So the claim is exactly
			// "the cell's whitespace is datum's whitespace", measured
			// against what datum said and not against a rule written here.
			assert.deepEqual(
				tail.headers.map((th) => th.text),
				tail.headers.map((th) => stripAnsi(column(th.key).header))
			);
			// The pane shows the rows its box holds — the newest page of the
			// listing — so the cells it is held to are the listing's last N.
			const offset = cli.rows.length - tail.rows.length;
			for (const [i, row] of tail.rows.entries()) {
				assert.deepEqual(
					row.map((td) => td.text),
					row.map((td) => stripAnsi(column(td.key).cells[offset + i]))
				);
			}
		}, { clock: AFTERNOON });
	});

	test("are the REPL's block, character for character", async () => {
		await withApp(async (app) => {
			const { repl, listed } = await twoSurfaces(app);
			assert.equal(repl.text, repl.spans.map((s) => s.text).join(''));
			assert.equal(repl.text, stripAnsi(listed.stdout));
		}, { clock: AFTERNOON });
	});

	test('paint one escape the same colour on both surfaces', async () => {
		await withApp(async (app) => {
			const { tail, repl } = await twoSurfaces(app);
			const badges = painted(repl.spans);
			assert.ok(badges.length > 0, 'the REPL painted no background at all');

			// Every background the tail pane paints is one the REPL painted
			// for the same listing: the field badge, the state chip's three
			// slots. Not "some colour" — the set, so a component that
			// dropped `style:background` on one surface fails here.
			const cells = [...tail.headers, ...tail.rows.flat()];
			const shown = new Set(cells.flatMap((cell) => painted(cell.spans)));
			assert.ok(shown.size > 0, 'the tail pane painted no background at all');
			for (const colour of shown) {
				assert.ok(badges.includes(colour), `the tail pane paints ${colour}, the REPL does not`);
			}
		}, { clock: AFTERNOON });
	});

	test('paint bold and underline, on both surfaces, for the escapes datum writes', async () => {
		await withApp(async (app) => {
			const { tail, repl } = await twoSurfaces(app);
			const bold = (spans) => spans.filter((s) => Number(s.weight) >= 700 && s.text.trim());
			const under = (spans) => spans.filter((s) => s.decoration.includes('underline') && s.text.trim());

			// `\x1b[1m` — the `!` a repeated state is marked with.
			const cells = tail.rows.flat();
			assert.deepEqual(
				bold(repl.spans).map((s) => s.text.trim()),
				['!'],
				'the REPL did not paint the repeat marker bold'
			);
			assert.deepEqual(
				bold(cells.flatMap((c) => c.spans)).map((s) => s.text.trim()),
				['!'],
				'the tail pane did not paint the repeat marker bold'
			);

			// `\x1b[4m` — a stamp in the future, which the second entry is.
			// The tail's header copy of this loop never set it; the shared
			// component does, so the surfaces agree.
			const ahead = under(repl.spans).map((s) => s.text.trim());
			assert.ok(ahead.length > 0, 'the REPL did not underline the stamp in the future');
			assert.deepEqual(
				under(cells.flatMap((c) => c.spans)).map((s) => s.text.trim()),
				ahead,
				'the tail pane underlines a different set of stamps than the REPL'
			);
		}, { clock: AFTERNOON });
	});
});
