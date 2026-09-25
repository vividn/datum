// What "the same tail" means when two implementations are compared.
//
// The browser's `tail` is the CLI's `format_table_with` through wasm, so
// the two tables are the same string except where they cannot be: the hid
// column, which names a row minted on the machine that wrote it. Both
// sides mask that and compare everything else — the header, the column
// order, the alignment, the `_n` ordinals, the clock.

/** A minted hid: 12 characters of base36 (`entry_id.rs`: `hid_from_bytes`). */
const HID = /\b[0-9a-z]{12}\b/g;

/** The short form the table's `hid` column actually prints. */
const SHORT_HID = /\b[0-9a-z]{5}\b/g;

/**
 * A tail table, comparable across machines.
 *
 * Hids go — they are per-write facts, and no two implementations could
 * agree on them. Everything else stays exactly as datum printed it,
 * including the run of spaces that IS the column alignment. Colour is
 * stripped: the CLI writes the fixture through a pipe, where its own gate
 * shuts (`color_enabled`), so painting is asserted separately rather than
 * smuggled into this comparison.
 */
export function comparableTail(table) {
	return stripAnsi(table)
		.replace(HID, '<hid>')
		// The `hid` column prints the SHORT hid, five characters — masked
		// after the long form so a full hid is never half-masked.
		.split('\n')
		.map((line) => line.replace(/(\s)([0-9a-z]{5})(\*?)$/, (_, gap, __, star) => gap + '<hid>' + star))
		.join('\n')
		.replace(/\n+$/, '');
}

/** Strip ANSI SGR escapes — `strip_ansi` in render/colors.rs, in JS. */
export function stripAnsi(text) {
	// eslint-disable-next-line no-control-regex
	return text.replace(/\x1b\[[0-9;]*m/g, '');
}

/**
 * A table's rows as text, one string per row, runs of spaces collapsed.
 *
 * The form in which a terminal grid and an HTML table can be compared at
 * all: the padding that aligns a terminal's columns is not in the DOM,
 * and the cell boundaries the DOM has are not in the text.
 */
export function rowTexts(table) {
	return comparableTail(table)
		.split('\n')
		.map((line) => line.trim().replace(/\s+/g, ' '))
		.filter(Boolean);
}

export { SHORT_HID };
