// What the three CLI-render panes run, and how wide they claim to be.
//
// Each pane is one datum command rendered as text (da-1x72). Deliberately
// CLI renders for now: the moment a pane becomes an interactive table it
// stops being free, and that is a later, separate trade.

/** One pane: a title, the command behind it, and what to say when it's quiet. */
export interface PaneSpec {
	title: string;
	argv: string[];
	/** Shown when the command succeeds with no output at all. */
	empty?: string;
}

/**
 * The column count the panes render CLI output at.
 *
 * datum sizes its tables to the terminal (`render/term_layout.rs`: ioctl,
 * then `$COLUMNS`, then 80). A browser pane has no terminal, so it claims
 * a fixed width instead of tracking its own pixels — a table that
 * re-columnised on every window drag would be unreadable. The panes are
 * laid out at least this wide and scroll sideways past it; the wasm
 * runner (da-1vat) exports the same number as `$COLUMNS` so the CLI and
 * the pane agree.
 */
export const PANE_COLS = 80;

export const DAYVIEW: PaneSpec = {
	title: 'dayview',
	argv: ['dayview'],
	empty: 'nothing to draw yet'
};

// `nowview`, not `now`: the CLI splits them by output device — `now`
// paints a band on the terminal, `nowview` writes the SVG — and a browser
// pane is the second of those. The pane shows what the CLI would write to
// a .svg file, which is a better picture than the terminal's.
export const NOWVIEW: PaneSpec = {
	title: 'nowview',
	argv: ['nowview'],
	empty: 'no field has a state yet'
};

export const TAIL: PaneSpec = {
	title: 'tail',
	argv: ['tail'],
	empty: 'nothing logged yet — try `switch mood happy`'
};
