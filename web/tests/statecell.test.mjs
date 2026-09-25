// The tail's state cell: three values, drawn so a person can read them
// (da-fknn8).
//
// vividn, 2026-09-21, looking at datum.day:
//
//   "if you transition state a -> b -> c -> a, the state changes are
//    otherwise unrendered"
//
// On a terminal a state cell is `[LEAD][body][TRAIL]`, and the two edges
// are ONE CHARACTER EACH painted as a background colour. The page drew
// exactly that, which is why the transitions were not there to read: a
// coloured space butted against the word it brackets is padding, and where
// the state continues forward the TRAIL is the BODY'S OWN COLOUR.
//
// The first version drew the edges as WORDS. vividn ruled otherwise
// (2026-09-22): "The previous and following states should just have the
// colors, not the full text — just like the CLI. Really mirror the CLI here.
// The rerendering in the web is just for some niceities and being able to
// select. Hover over previous state could show the value."
//
// ⚑ Which dissolves the cost the words carried: a colour block is ONE
// CHARACTER WIDE whatever state it stands for, so every chip starts at the
// same x and the column scans again.
//
// So what is asserted here is that the page draws the CLI'S OWN SLOTS — the
// painted `previous_state_cell` / `following_state_cell`, byte for byte —
// and adds exactly one thing a terminal cannot: the value on HOVER.

import test, { before, after, describe } from 'node:test';
import assert from 'node:assert/strict';
import { startApp, stopApp, sharedApp, todayAt } from './support/app.mjs';
import { widgetPane } from './support/layout.mjs';

before(startApp);
after(stopApp);

/** The `main` tab as one tail pane, so the pane is the viewport less its frame. */
const ONLY_TAIL = JSON.stringify({ version: 1, name: 'main', root: widgetPane('only', 'tail') });

/**
 * The app, held at noon, showing nothing but the tail: ONE app for the file
 * (sharedApp in support/app.mjs), `main` opened as a draft of the tail alone
 * and the database emptied for every test. The clock is pinned because
 * every seed below is a bare `HH:MM`, which means "today" against whatever
 * the app is holding — and an unpinned one seeds the future for half the
 * day (app.mjs, da-ldct).
 */
const shared = sharedApp({ viewport: { width: 1000, height: 700 }, clock: todayAt(12, 0) });
const onTail = (fn) => shared.solo(fn, ONLY_TAIL);

/** Store `lines` through the seam's quiet door, then redraw: no REPL here to type into. */
async function seedLines(app, lines) {
	for (const line of lines) {
		const out = await app.runLine(line);
		assert.equal(out.code, 0, `${line}: ${out.stderr}`);
	}
	await app.redraw();
}

const TAIL = 'section.pane[data-title="tail"]';

/** Each drawn row's field and its state cell: the whole text, and the edge words alone. */
const cells = (page) =>
	page.evaluate((sel) => {
		const text = (el) => (el?.textContent ?? '').replace(/\u00a0/g, ' ');
		return [...document.querySelectorAll(`${sel} tbody tr`)].map((tr) => ({
			hid: tr.dataset.hid,
			field: text(tr.querySelector('td.field')).trim(),
			state: text(tr.querySelector('td.state')).trim(),
			// The chip alone: the cell's text less its edge words — what the
			// CLI paints as `state_body`.
			body: [...(tr.querySelector('td.state')?.childNodes ?? [])]
				.filter((n) => !(n instanceof Element) || !n.classList.contains('edge'))
				.map((n) => text(n))
				.join('')
				.trim(),
			/** Where the chip starts, which is what the raggedness was about. */
			chipAt: (() => {
				const td = tr.querySelector('td.state');
				const nodes = [...(td?.childNodes ?? [])].filter(
					(n) => !(n instanceof Element) || !n.classList.contains('edge')
				);
				const el = nodes.find((n) => n instanceof Element);
				return el ? Math.round(el.getBoundingClientRect().left - td.getBoundingClientRect().left) : null;
			})(),
			edges: [...tr.querySelectorAll('td.state .edge')].map((e) => ({
				text: text(e),
				title: e.title,
				where: e.classList.contains('from') ? 'from' : 'to',
				// The slot's PAINT, which is a background on a terminal.
				background: getComputedStyle(e.querySelector('span') ?? e).backgroundColor,
				width: Math.round(e.getBoundingClientRect().width)
			}))
		}));
	}, TAIL);

/**
 * Wait for the pane to be at rest with `n` rows drawn.
 *
 * No `timeout:` — §16. Playwright's own 30 s default IS the suite's one tier,
 * and a shorter cap asserts speed by accident: it fails a test that had
 * seconds left to run, and names a bare `TimeoutError` instead of whatever
 * the test was actually about. `tailcolumns.test.mjs` waits on this same
 * condition the same way.
 */
const settled = (page, n) =>
	page.waitForFunction(
		([sel, n]) => window.__datumInflight === 0 && document.querySelectorAll(`${sel} tbody tr`).length >= n,
		[TAIL, n]
	);

describe('a state transition is three values, and the page draws them', () => {
	test('a → b → c → a: every row says where it came from', async () => {
		await onTail(async (app) => {
			const { page } = app;
			await seedLines(app, [
				'switch mood a -t 0900',
				'switch mood b -t 0910',
				'switch mood c -t 0920',
				'switch mood a -t 0930'
			]);
			await settled(page, 4);
			// Four rows, not five: da-gvcen (#68) made a first log ONE
			// entry that declares its own field, so there is no separate
			// TRACK row ahead of them any more.
			const rows = (await cells(page)).filter((r) => r.field === 'mood');
			assert.equal(rows.length, 4, `four switches: ${JSON.stringify(rows)}`);

			// The three transitions. Each row's LEAD carries the previous
			// state — as COLOUR, with the value on hover (vividn: "just
			// have the colors, not the full text … hover could show the
			// value"). Before this bead not one of these rows said
			// anything at all about where it came from.
			const froms = rows.map((r) => r.edges.filter((e) => e.where === 'from').map((e) => e.title));
			assert.deepEqual(
				froms,
				[['∅'], ['a'], ['b'], ['c']],
				`the previous state of each row, on hover: ${JSON.stringify(rows)}`
			);

			// …and the state itself is still there, whole, beside its edge.
			assert.deepEqual(
				rows.map((r) => r.body),
				['a', 'b', 'c', 'a'],
				`the chips: ${JSON.stringify(rows)}`
			);

			// The lead is painted in the PREVIOUS state's colour, not the
			// row's: `b`'s lead is `a`'s hue and `c`'s is `b`'s, and the
			// two differ, so the colour really is carrying the value.
			const [, bRow, cRow] = rows;
			assert.notEqual(bRow.edges[0].background, cRow.edges[0].background, 'a and b painted the same');
			assert.ok(/^rgb/.test(bRow.edges[0].background), `unpainted lead: ${bRow.edges[0].background}`);

			// ⚑ AND THE THING THE COLOUR BOUGHT: every chip starts at the
			// same x. With words this was +85.0px against +14.9px and the
			// column stopped scanning; a colour block is one character
			// wide whatever it stands for.
			const starts = [...new Set(rows.map((r) => r.chipAt))];
			assert.equal(
				starts.length,
				1,
				`the chips no longer line up: ${JSON.stringify(rows.map((r) => [r.body, r.chipAt]))}`
			);

			assert.deepEqual(app.errors, []);
		});
	});

	test('a row that crosses nothing draws no edge, and is no wider', async () => {
		await onTail(async (app) => {
			const { page } = app;
			// Three occurs of a field already tracked: each sits inside one
			// state, so there is no transition to draw on any of them.
			await seedLines(app, ['switch coffee drinking -t 0900', 'occur coffee -t 0910', 'occur coffee -t 0920']);
			await settled(page, 3);
			const rows = await cells(page);
			const occurs = rows.filter((r) => r.field === 'coffee' && r.state.includes('●'));
			assert.equal(occurs.length, 2, `two occur rows: ${JSON.stringify(rows)}`);
			for (const row of occurs) {
				assert.deepEqual(row.edges, [], `an ordinary occur grew an edge: ${JSON.stringify(row)}`);
				assert.equal(row.body, '●', `the dot alone: ${JSON.stringify(row)}`);
			}
			assert.deepEqual(app.errors, []);
		});
	});

	test('the null sign comes off the PREVIOUS state, not the current one', async () => {
		await onTail(async (app) => {
			const { page } = app;
			// The first entry of a field: nothing was tracked before it.
			await seedLines(app, ['occur tea -t 0900']);
			await settled(page, 1);
			const rows = await cells(page);
			const occur = rows.find((r) => r.field === 'tea' && r.state.includes('●'));
			assert.ok(occur, `no occur row: ${JSON.stringify(rows)}`);
			// The `∅` is an EDGE — the state before this row, drawn in the
			// place the CLI's LEAD slot occupies — and NOT part of the dot,
			// which is the whole of the claim here. (It also declares the
			// field, so it has a trailing edge too; that is its own test.)
			assert.equal(occur.body, '●', `the ∅ is in the body: ${JSON.stringify(occur)}`);
			const from = occur.edges.filter((e) => e.where === 'from');
			assert.deepEqual(
				from.map((e) => e.text.trim()),
				['∅'],
				`the null sign is not the previous state: ${JSON.stringify(occur)}`
			);
			assert.deepEqual(app.errors, []);
		});
	});

	test('a switch that went nowhere draws the CLI\'s `!`, not a word', async () => {
		await onTail(async (app) => {
			const { page } = app;
			// The same state twice: the field's compiled state did not
			// move, which the CLI marks with a bold `!` in the LEAD slot
			// (da-qfv0) — the signal that an intermediate transition was
			// forgotten. Drawing the edges must not lose it.
			await seedLines(app, ['switch mood calm -t 0900', 'switch mood calm -t 0910']);
			await settled(page, 2);
			const rows = (await cells(page)).filter((r) => r.field === 'mood');
			const repeat = rows.at(-1);
			// A switch draws BOTH slots, as it does on a terminal — the
			// lead carries the `!` instead of the prior state's plain
			// colour, and the trail is unchanged.
			assert.deepEqual(
				repeat.edges.filter((e) => e.where === 'from').map((e) => e.text.trim()),
				['!'],
				`the repeat marker: ${JSON.stringify(rows)}`
			);
			// It comes from the CLI's own lead slot, not from a rule
			// here — and the hover still names the state it marks.
			assert.equal(repeat.edges[0].title, 'calm');
			assert.equal(repeat.body, 'calm');
			// The row before it moved from nothing to `calm`, so it says so.
			assert.deepEqual(
				rows.at(-2).edges.filter((e) => e.where === 'from').map((e) => e.title),
				['∅']
			);
			assert.deepEqual(app.errors, []);
		});
	});

	test('a timeless row has no transition, so it draws no edge', async () => {
		await onTail(async (app) => {
			const { page } = app;
			// `-T`: a row with no occur time sits at no instant, so there
			// is no state before it. The terminal gives it `¢` and no
			// slots; the page must not invent a null sign for it.
			await seedLines(app, ['add thing -T']);
			await settled(page, 1);
			const rows = await cells(page);
			const timeless = rows.find((r) => r.field === 'thing' && r.body.includes('¢'));
			assert.ok(timeless, `no timeless row: ${JSON.stringify(rows)}`);
			assert.deepEqual(timeless.edges, [], `an edge on a timeless row: ${JSON.stringify(timeless)}`);
			assert.deepEqual(app.errors, []);
		});
	});

	test('a long previous state costs ONE character, and the pane still shows its rows', async () => {
		await onTail(async (app) => {
			const { page } = app;
			// This fixture is why the words had to go. MEASURED when the
			// edges were words: two 31-character states in one cell grew
			// the rows taller than a 243px pane's box and its count came
			// down to ZERO — it showed nothing. A terminal never has to
			// draw that, because its slot is one character however long
			// the state is, and now neither does the page.
			// The narrow screen is this test's alone; `home` puts it back.
			await page.setViewportSize({ width: 480, height: 700 });
			await seedLines(app, [
				'switch mood contemplative-about-the-weather -t 0700',
				'switch mood restless-in-the-afternoon-light -t 0900'
			]);
			await settled(page, 1);
			const rows = await cells(page);
			assert.ok(rows.length > 0, 'the pane shows no rows at all');

			const long = rows.find((r) =>
				r.edges.some((e) => e.where === 'from' && e.title?.startsWith('contemplative'))
			);
			assert.ok(long, `no row carries the long previous state: ${JSON.stringify(rows)}`);
			const edge = long.edges.find((e) => e.where === 'from');

			// The WHOLE value is on the hover — nothing is cut, because
			// nothing is written.
			assert.equal(edge.title, 'contemplative-about-the-weather');
			// And the slot is one character of paint. Measured against
			// the chip's own advance rather than a number written here:
			// a 31-character state may not be wider than a 1-character
			// one, which is the whole of what the colour bought.
			const short = rows
				.flatMap((r) => r.edges)
				.find((e) => e.where === 'from' && e.title === '∅');
			assert.ok(short, `no short-titled edge to compare against: ${JSON.stringify(rows)}`);
			assert.ok(edge.width > 0, `the slot collapsed to nothing: ${JSON.stringify(edge)}`);
			assert.equal(
				edge.width,
				short.width,
				`a long state made a wider slot: ${JSON.stringify([edge, short])}`
			);
			assert.deepEqual(app.errors, []);
		});
	});

	test('a compound switch draws the state it switches TO, and the row says the same', async () => {
		await onTail(async (app) => {
			const { page } = app;
			// `switch f end <dur> , switch f <state>` is ONE row carrying
			// its own `following_state` — the state it switches to. The
			// terminal's TRAIL slot paints THAT, not the state the block
			// INTERRUPTED (da-qxmqe: it used to, because the slot probed
			// the field's history at the block's far edge excluding the
			// row itself).
			// NB no trailing `-t` on the second line: a flag after the `,`
			// rides into the second side and the comma becomes a
			// parallel-state comma instead of a side separator — the
			// native CLI does the same, so it is the seed that would be
			// wrong, not the page. The row lands at the pinned noon.
			await seedLines(app, [
				'switch activity writing -t 0900',
				'switch activity end 14m , switch activity researching'
			]);
			await settled(page, 2);
			const rows = (await cells(page)).filter((r) => r.field === 'activity');
			const compound = rows.at(-1);
			assert.equal(compound.body, 'end', `the chip: ${JSON.stringify(compound)}`);
			assert.deepEqual(
				compound.edges.map((e) => [e.where, e.title]),
				[
					['from', 'writing'],
					['to', 'researching']
				],
				`it came from writing and goes to researching: ${JSON.stringify(compound)}`
			);

			// ⚑ AND THE COLOUR THAT TOOLTIP SITS ON, beside what the ROW
			// says. The JSON and the paint AGREEING is the whole of
			// da-qxmqe: the row's `following_state` and the painted
			// `following_state_cell` both come from `StateTransition::of`,
			// and asserting only one would leave them free to drift apart.
			const trail = compound.edges.find((e) => e.where === 'to');
			const out = await app.run(['tail', 'activity', '--json']);
			const row = JSON.parse(out.stdout).rows.find((r) => r.verb === 'block-switch');
			assert.equal(row.previous_state, 'writing', `the row: ${JSON.stringify(row)}`);
			assert.equal(row.following_state, 'researching', `the row: ${JSON.stringify(row)}`);
			const asRgb = (hex) =>
				`rgb(${[1, 3, 5].map((i) => parseInt(hex.slice(i, i + 2), 16)).join(', ')})`;
			// Against the ROW's own following_state_color, not against the
			// lead's — `trail !== lead` would still pass if a change moved
			// BOTH edges, and this cannot pass for the wrong reason
			// (vremya, #74).
			assert.equal(
				trail.background,
				asRgb(row.following_state_color),
				`the trail must paint researching, the state the row switches TO: ` +
					JSON.stringify([trail, row.following_state_color])
			);
			assert.deepEqual(app.errors, []);
		});
	});

	test('a first log declares its field: the ROW says so, and the cell mirrors the CLI', async () => {
		await onTail(async (app) => {
			const { page } = app;
			// da-gvcen (#68): a first log of an untracked field is ONE
			// `point-switch` carrying `MOMENT` — it draws the DOT and
			// declares the field in its own `following_state`.
			//
			// ⚑ I once made the page draw that trailing state, reasoning
			// that this dot really does move the field. vividn's ruling
			// overrode it: "really mirror the CLI here". A terminal draws
			// a dot NO trailing slot, so neither does the page — the
			// declaration lives in the DATA, which is where he asked for
			// it in the first place, and the cell stays `∅●`.
			await seedLines(app, ['occur brandnew -t 0900']);
			await settled(page, 1);
			const rows = await cells(page);
			const first = rows.find((r) => r.field === 'brandnew');
			assert.ok(first, `no row: ${JSON.stringify(rows)}`);
			assert.equal(first.body, '●', `the dot alone: ${JSON.stringify(first)}`);
			assert.deepEqual(
				first.edges.map((e) => [e.where, e.title]),
				[['from', '∅']],
				`the cell mirrors the CLI's ∅● : ${JSON.stringify(first)}`
			);
			// …and the declaration is on the row, which is vividn's
			// original ask: "it should come through as previous:
			// IGNORED, current: state, following: …".
			const out = await app.run(['tail', 'brandnew', '--json']);
			const [r] = JSON.parse(out.stdout).rows;
			assert.equal(r.previous_state, 'IGNORED');
			assert.equal(r.state, null);
			assert.equal(r.following_state, 'TRACKED');
			assert.deepEqual(app.errors, []);
		});
	});

	test('the row reports the transition, so nothing is read back out of colours', async () => {
		await onTail(async (app) => {
			// The data half of the bead, at the seam: `tail --json` carries
			// the three values. `following_state` used to be the entry's own
			// column — `null` on every plain switch — and there was no
			// previous state at all.
			await app.run(['switch', 'posture', 'sitting', '-t', '0900']);
			await app.run(['switch', 'posture', 'standing', '-t', '0930']);
			const out = await app.run(['tail', 'posture', '--json']);
			assert.equal(out.code, 0, out.stderr);
			// Two rows: da-gvcen leaves no separate TRACK row to skip.
			const rows = JSON.parse(out.stdout).rows;
			assert.equal(rows.length, 2, out.stdout);
			assert.equal(rows[0].previous_state, 'IGNORED');
			assert.equal(rows[0].following_state, 'sitting');
			assert.equal(rows[1].previous_state, 'sitting');
			assert.equal(rows[1].state, 'standing');
			assert.equal(rows[1].following_state, 'standing');
			// The colours are the CLI's, so the page paints no state itself.
			assert.match(rows[1].previous_state_color, /^#[0-9a-f]{6}$/);
			// And the body is the chip alone: the slots the edges replace
			// are not in it, so nothing is drawn twice.
			assert.ok(rows[1].state_body.includes('standing'), rows[1].state_body);
			assert.ok(!rows[1].state_body.includes('sitting'), rows[1].state_body);
			// Whether the state moved is the CLI's answer, not a
			// comparison the page makes (set-aware, da-g1b8).
			assert.equal(rows[1].state_repeat, false);
		});
	});
});
