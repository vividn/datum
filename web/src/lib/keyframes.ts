// The input line's KEY FRAMES (da-31d1d), pure and import-light so node
// tests it as it is. vividn, 09-26: «have the input bar show some helpers for
// what the keys are, so a small frame with the name of the key, space
// advances (unless there are quotes), show the default value in ghost text
// for optional keys».
//
// A frame is how a key's token on the line is DRAWN, never a second copy of
// it: the line holds exactly what runs. So a frame writes the text a reader
// would have typed, `key=` — the CLI binds no positional to a spec's keys
// today, and da-9s98k is where that grammar is decided. If it becomes
// `occur tea 3`, `writes` and `valueOf` are the two places that change.

import type { Key } from './fields.js';
import { shellSpans, shellSplit } from './shellSplit.js';

/**
 * The verbs a field's keys are typed after. Not `end` (da-o4jaa), vividn,
 * 09-27: «generally speaking I don't save any extra_data in this one, so I
 * think that should be the default. But allowing it might also be nice??»
 * — so `end <field>` draws no frames and Enter writes it no defaults, and
 * a `key=value` typed on it by hand still runs as typed.
 */
const VERBS = ['occur', 'start'];

/** One declared key, and where its token stands on the line if it is there. */
export interface Frame extends Key {
	/** The token's UTF-16 span in the line; absent while the key is unwritten. */
	from?: number;
	to?: number;
	/** What the reader has typed into it. */
	value?: string;
}

/** The text a frame puts on the line, ready for its value. */
export const writes = (key: Key): string => `${key.name}=`;

/** The value of `key`'s token, or undefined when `token` is not that key's. */
const valueOf = (key: Key, token: string): string | undefined =>
	token.startsWith(writes(key)) ? token.slice(writes(key).length) : undefined;

/**
 * The frames for a line — one per key the field declares, in its order —
 * or null when the line is not `occur|start <field>` for a field with
 * keys. A key is written when a token after the field carries it.
 */
export function framesOf(line: string, fields: { field: string; keys: Key[] }[]): Frame[] | null {
	const [verb, field, ...rest] = shellSpans(line).tokens;
	const keys = VERBS.includes(verb?.text) ? fields.find((b) => b.field === field?.text)?.keys : undefined;
	if (!keys?.length) return null;
	return keys.map((key) => {
		const token = rest.find((t) => valueOf(key, t.text) !== undefined);
		return token ? { ...key, from: token.from, to: token.to, value: valueOf(key, token.text) } : { ...key };
	});
}

/**
 * The line after a SPACE typed at its end, or null when the space is only a
 * space: inside quotes, in a line with no frames, or with every key written.
 *
 * Space advances: it writes the next unwritten key's `key=`. A space in a
 * frame still EMPTY skips it — its `key=` comes off and the one after it
 * goes on, round to the first again — so a reader can step past an
 * optional key, and a lone empty key stays put.
 */
export function advance(line: string, fields: { field: string; keys: Key[] }[]): string | null {
	const frames = framesOf(line, fields);
	const { tokens, quoted } = shellSpans(line);
	if (!frames || quoted || tokens.length < 2) return null;
	const empty = frames.findIndex((f) => f.value === '' && f.to === line.length);
	const order = empty < 0 ? frames : [...frames.slice(empty + 1), ...frames.slice(0, empty)];
	const next = order.find((f) => f.from === undefined);
	if (!next) return null;
	const kept = empty < 0 ? line : line.slice(0, frames[empty].from);
	return `${kept.replace(/\s*$/, '')} ${writes(next)}`;
}

/**
 * The line as it runs: every optional key left empty or unwritten that has
 * a default gets `key=default`, so what runs is what the line shows and the
 * default is stored (the CLI does not apply one itself — da-9s98k). Not
 * inside an open quote, where the defaults would become part of its value.
 */
export function complete(line: string, fields: { field: string; keys: Key[] }[]): string {
	const frames = framesOf(line, fields);
	// An open quote would take the defaults into its value: the line runs as typed.
	if (!frames || shellSpans(line).quoted) return line;
	let out = line;
	// From the right, so each span is still where it was.
	for (const f of [...frames].filter((f) => f.value === '' && f.default !== undefined).sort((a, b) => b.from! - a.from!))
		out = out.slice(0, f.from) + writes(f) + quote(f.default!) + out.slice(f.to);
	const missing = frames.filter((f) => f.from === undefined && f.default !== undefined);
	return missing.length ? `${out.replace(/\s*$/, '')} ${missing.map((f) => writes(f) + quote(f.default!)).join(' ')}` : out;
}

/** A default as a token: quoted when it would split or unquote. */
function quote(value: string): string {
	const tokens = shellSplit(value);
	return tokens.length === 1 && tokens[0] === value ? value : `'${value.replace(/'/g, `'\\''`)}'`;
}
