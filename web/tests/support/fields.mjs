// The fields widget's shared fixtures and drivers (da-c7u8, da-lt0k).
//
// Split out when one `fields.test.mjs` outgrew node's 120 s per-FILE
// timeout and started taking main's Web job red with it: node counts a
// FILE as one test, so a file is a budget, and the way to stay under one
// is to stop putting every group in the same file. These are the pieces
// the six `fields*.test.mjs` hold in common — the seeded spec set, its
// colours, and the waits and seeds more than one of them types through. A
// helper only one file uses stays in that file.

// Hex, not CSS colour names: `crates/datum-commands/src/render/colors.rs`'s
// `resolve_rgb` only understands `#rrggbb` (or a legacy ANSI-256 number) —
// the same model both the CLI and the widget now share (da-yiyj) — so a name
// like `orange` would silently fall through to `md5(field)`.
export const FIXTURE = [
	'spec coffee --verb occur --color #ffa500',
	'spec work --verb start --color #0000ff',
	// `--state good=green` would name a state `good=green`: the colours are `--state-color`.
	'spec mood --verb switch --states good,bad --state-color good=#008000 --state-color bad=#ff0000 --color #800080',
	'spec note --verb add',
	'occur untracked'
];

export const ORANGE = 'rgb(255, 165, 0)';
export const BLUE = 'rgb(0, 0, 255)';
export const PURPLE = 'rgb(128, 0, 128)';
export const GREEN = 'rgb(0, 128, 0)';
export const CLEAR = 'rgba(0, 0, 0, 0)';

/** Type `line` into the bottom bar's prompt and wait for the REPL to echo it. */
export async function type(page, line) {
	const before = await page.locator('.repl .echo').count();
	const input = page.locator('.prompt-bar input');
	await input.fill(line);
	await input.press('Enter');
	await page.waitForFunction((n) => document.querySelectorAll('.repl .echo').length > n, before);
}

/** Wait until the bar's `field` button has computed `prop` of `value`. */
export const untilFieldStyle = (page, field, prop, value) =>
	page.waitForFunction(
		([f, p, v]) => {
			const el = document.querySelector(`.bar [data-field="${f}"]`);
			return el && getComputedStyle(el)[p] === v;
		},
		[field, prop, value]
	);

/** Two states seen, the second the one mood is in now. */
export const MOOD = ['switch mood calm -t 0900', 'switch mood happy -t 0930'];

/** Seed `lines`, then wait until the bar has drawn `field`. */
export async function seed(app, lines, field) {
	for (const line of lines) await app.submit(line);
	await app.page.locator(`.bar [data-field="${field}"]`).waitFor();
}

