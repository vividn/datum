// How many entries of the scrollback stand side by side, and where the
// hairlines between them fall (da-5iyo).
//
// vividn, 2026-09-21, reviewing PR 46: «ideally, but this can be another PR,
// the output of a single entry should be narrow enough to comfortably fit
// 2 on mobile side by side.» So the REPL is no longer one column of a
// phone's width — it is as many columns as the box it is in holds.
//
// THE TWO WIDTHS BELOW ARE MEASURED AND NOT CHOSEN. A breakpoint picked
// by eye would say "two columns on a phone" and mean "two columns at
// whatever width I had open"; these say what an entry's output needs
// (`MIN`) and what it wants (`WANT`), and a phone gets two because it can
// give two columns more than the first. `tests/scrollback.test.mjs` takes
// both measurements again against the built app, so a change to how an
// entry is drawn cannot quietly leave the numbers behind.
//
// The rows matter because the hairline between commands is a rule on a
// SEAM (da-qdm7): with one column that was "every entry but the first",
// and with several it is "every entry but the ones in the first row" —
// which is not `i >= columns`, because an entry the columns cannot carry
// (a `tail` table, a drawn day, an error) takes a whole row to itself and
// shifts every entry after it.

/**
 * The narrowest column an entry's output reads in, in px.
 *
 * Two numbers were measured for this, at the sizes the bead names — the
 * root's declared minimum of 412×563 and the 390×844 the phone drives run
 * at — with `switch mood happy`, `occur coffee`, `start work`, `occur
 * brew bean=… grind=… water=…` and a long-stated `switch` in the
 * scrollback:
 *
 * 1. **What an entry's output CUTS at: between 100 and 112px.** Narrowed
 *    a column at a time, nothing inside an entry overflows its box down
 *    to 112px and something does at 100 — the info line's longest
 *    unbreakable run, `14:59:59+0`, is 96px and the brace beside it wants
 *    the 107 that a section of two documents asks for. Above
 *    that floor the REPL loses nothing to a narrow column, because it has
 *    the vertical room to wrap where the bar's fixed band has to clip
 *    (Doc.svelte).
 * 2. **What a 390px phone HAS: 351px** of content in the pane, once the
 *    scrollback is long enough to scroll; 364 before the scrollbar, and a
 *    412px phone has 373.
 *
 * So a column is as wide as it can be and still come two to the narrowest
 * phone: (351 − `GAP`) / 2 is 168, and 160 is that with room for a
 * scrollbar of any browser's width. Three never crowd a phone (3 × 160 +
 * 2 × 14 = 508, and no phone here has 508), and one is what a pane
 * narrower than 334 keeps — the single-column reading this started from,
 * unchanged, because at that width every one of these lines already fits.
 */
export const MIN = 160;

/**
 * The width an entry's output WANTS, in px.
 *
 * The third measurement: the widest the two lines a document draws ask
 * for before anything wraps, over the ORDINARY shapes of that scrollback
 * — 232px for `CREATE 2i84w` / `15:00:00+0  mood  happy` (and `work
 * start`, which ties it), against 184px for the narrowest of them.
 * Ordinary, because a field or a state long enough makes an info line as
 * wide as you like and a column sized for the longest name anyone might
 * type is a column nobody has room for; a long one wraps, which here
 * costs nothing. A column this wide holds the ordinary set unbroken,
 * which is what a box with the room to spare should be spending it on:
 * columns of `MIN` on a 1280px screen are seven strips of a phone's
 * reading, with the width they were supposed to use left empty beside
 * them.
 *
 * ⚠ RE-MEASURED at da-gvcen, and the OLD number says why it had to be.
 * It was 270px, and the line it was measured on was `TRACK● av96x` /
 * `14:59:59+0  coffee  TRACKED` — a field's DECLARATION, which every
 * first log used to echo beside its entry. da-gvcen folds the
 * declaration into the entry's own `following_state`, and `datum track`
 * prints `✓ Tracking …` rather than a document, so no scrollback draws
 * that shape any more. 270 had come loose from anything measurable,
 * which `scrollback.test.mjs` says in as many words.
 *
 * The 28px above the measurement is the headroom `MIN` carries for the
 * same reason — room for datum to reword an echo without the number
 * moving under it — and it is inside the 32px the test allows. It is
 * also what keeps the column COUNTS the layout was tuned for: a 390px
 * phone still gets two and a 1280px screen still four, which a bare 232
 * would have made five.
 */
export const WANT = 260;

/** The air between two columns, in px — the rule's own margin, sideways. */
export const GAP = 14;

/** How many columns of `each` px, plus their gaps, fit in `width`. */
const fit = (width: number, each: number): number => Math.floor((width + GAP) / (each + GAP));

/**
 * How many entries stand side by side in a box `width` px wide.
 *
 * AS MANY AS FIT AT THE WIDTH AN ENTRY WANTS — and never fewer than two
 * where two can be read, which is the whole of the ask. The second half
 * is what a phone gets: 351px cannot give two columns 270px each, but it
 * can give them 169, and 169 is above everything an entry's output needs
 * (`MIN`).
 *
 * `null` is a box not measured yet: one column, which is what the REPL
 * has always drawn, so nothing flashes into place on the first frame.
 */
export function columnsIn(width: number | null): number {
	if (width === null || !Number.isFinite(width)) return 1;
	return Math.max(1, fit(width, WANT), fit(width, MIN) >= 2 ? 2 : 1);
}

/**
 * The row each entry falls on, reading across `columns` columns.
 *
 * `wide[i]` is an entry whose output is not the CLI's action echo but its
 * own printed text — a table, a picture, an error. Those keep the full
 * width: a `tail` squeezed into half a phone is not a table. Such an
 * entry starts a fresh row and ends it.
 */
export function rowsOf(wide: readonly boolean[], columns: number): number[] {
	const rows: number[] = [];
	let row = 0;
	let filled = 0;
	for (const full of wide) {
		if (full && filled > 0) {
			row += 1;
			filled = 0;
		}
		rows.push(row);
		if (full || filled + 1 === columns) {
			row += 1;
			filled = 0;
		} else {
			filled += 1;
		}
	}
	return rows;
}
