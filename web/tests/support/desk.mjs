// The desktop `day` the suite is written against (da-ncsyn): strip /
// [REPL | nowview / tail / custom] / fields — the shipped default until
// da-ncsyn moved it to the mobile shape.
//
// WHY IT IS PINNED. The suite finds panes by the ids this tree gives them
// (`repl`, `custom`, `side`, …), and `app.submit`, `pressWidget`,
// `echoes()` and `outputs()` (support/app.mjs) — and `type` in
// support/fields.mjs — wait on or read `.repl .echo` / `.repl pre`: the
// REPL PANE. The shipped desktop tree has no REPL pane (the REPL is the log
// under the bottom bar, which mounts on the first gesture and skips the
// newest entry), so against it every one of those waits times out — 183
// tests in 43 files, measured when the default moved.
//
// WHAT WOULD LET IT GO. A completion signal the harness can wait on that
// does not depend on the layout — the scrollback itself, say, read through
// `window.datum` rather than off a pane — and the pane-id lookups moved to
// panes the test opens for itself. Until then the tree is pinned here
// rather than following whatever a fresh device gets.
//
// HOW. `openApp` hands it to the page before the app loads, as
// `window.__datumDesktopLayout` (`defaultLayoutFor` in layout/defaults.ts),
// so `day` is this tree for every test that does not ask for `desk: null`.
// The shipped default's own tests (layout-desktop.test.mjs) are those that
// do.

const bar = { edge: 'top', align: 'start' };

const widgetPane = (id, kind, settings = {}) => ({
	id,
	tabs: [{ id: `${id}-tab`, content: { id: `${id}-widget`, kind, settings } }],
	active: 0,
	bar
});

const holder = (id, split) => ({ id, tabs: [{ id: `${id}-tab`, content: split }], active: 0, bar });

const split = (id, direction, ratios, children) => ({ id, direction, ratios, children });

const legibly = (heights) => heights.map((h) => h / heights.reduce((a, b) => a + b, 0));

/** `STRIP` in layout/defaults.ts: DEFAULT_LABEL_WIDTH, HOUR_FROM, HOUR_TO. */
const STRIP = { labelWidth: 54, freezeToday: true, hourFrom: 0, hourTo: 24 };

export const DESK = {
	version: 1, // LAYOUT_VERSION
	name: 'default',
	designedFor: 'desktop',
	root: holder(
		'root',
		split('main', 'column', legibly([85, 370, 63]), [
			widgetPane('strip', 'daystrip', STRIP),
			holder(
				'middle',
				split('across', 'row', [2 / 3, 1 / 3], [
					widgetPane('repl', 'repl'),
					holder(
						'side',
						split('aside', 'column', legibly([122, 116, 116]), [
							widgetPane('nowview', 'nowview'),
							widgetPane('tail', 'tail'),
							widgetPane('custom', 'custom')
						])
					)
				])
			),
			widgetPane('bar', 'widgetbar')
		])
	)
};
