// What "the same stdout" means when the browser and the native CLI print it.
//
// Since da-g8o8 there is one implementation — the browser runs the CLI
// itself — so the two outputs are the same bytes except where they cannot
// be: the facts a write mints (hids, content hashes, the instant it was
// written). Both sides mask those, in order of appearance, and compare
// everything else — colour escapes included, because painting is part of
// what the CLI prints.
//
// Every entry the script logs is pinned to PINNED_DAY, so any other instant
// in the output is "now": the moment a row was written, which no two runs
// share.
//
// Used by web/scripts/capture-cli-fixtures.mjs (the native half, into
// tests/fixtures/cli-stdout.json) and by tests/stdout.test.mjs.

export const PINNED_DAY = '2026-03-16';

/** Any run of colour escapes, raw or as JSON writes them (`\u001b`). */
const ESC = '(?:(?:\\x1b|\\\\u001b)\\[[0-9;]*m)*';

/** A fresh masker: hids are numbered by first appearance within one run. */
export function masker() {
	const full = new Map();
	const short = new Map();
	const label = (map, prefix) => (id) => {
		if (!map.has(id)) map.set(id, `<${prefix}${map.size}>`);
		return map.get(id);
	};
	const hid = label(full, 'hid');
	const id = label(short, 'id');
	return (text) =>
		text
			.replace(/\b[0-9a-f]{64}\b/g, '<hash>')
			// Over-masking is harmless — both sides get the same function —
			// so every 12-character token is taken for a hid.
			.replace(/\b[0-9a-z]{12}\b/g, hid)
			// The short hid: after an action word, or closing a tail row,
			// where it follows the `_n` column.
			.replace(
				// `TRACK<glyph>` is an action word too (da-c8mh).
				new RegExp(`((?:CREATE|EXISTS|UPDATE|DELETE|TRACK.)${ESC} ${ESC})([0-9a-z]{5})\\b`, 'g'),
				(_, before, m) => before + id(m)
			)
			.replace(new RegExp(`(_\\d+${ESC} +)([0-9a-z]{5})$`, 'gm'), (_, before, m) => before + id(m))
			// And as a JSON string — `tail --json`'s hid column — when it is
			// the head of a hid this run has already seen.
			.replace(/"([0-9a-z]{5})"/g, (m, head) => {
				const whole = [...full.keys()].find((h) => h.startsWith(head));
				return whole ? `"${full.get(whole)}:5"` : m;
			})
			// nowview's clock.
			.replace(/>\d\d:\d\d</g, '><clock><')
			.replace(/c\d{8}T\d{6}(?:\.\d+)?Z/g, 'c<now>')
			.replace(/\d{4}-\d\d-\d\dT\d\d:\d\d(?::\d\d(?:\.\d+)?)?(?:Z|[+-]\d\d:\d\d)?/g, (t) =>
				t.startsWith(PINNED_DAY) ? t : '<now>'
			)
			// A timeless entry's display clock is its create time.
			.replace(new RegExp(`(c${ESC})\\d\\d:\\d\\d:\\d\\d`, 'g'), '$1<clock>')
			.replace(/\n+$/, '');
}
