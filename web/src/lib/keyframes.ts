// The input line's KEY FRAMES (da-31d1d), pure and import-light so node
// tests it as it is. vividn, 09-26: «have the input bar show some helpers for
// what the keys are, so a small frame with the name of the key, space
// advances (unless there are quotes), show the default value in ghost text
// for optional keys».
//
// A frame is how a key's token on the line is DRAWN, never a second copy of
// it: the line holds exactly what runs. Since da-9s98k the CLI binds a
// field's keys to positionals, so a frame holds the BARE value — `occur tea
// 3 2` — and the line's other words bind to the keys still unwritten, in
// order, as the CLI binds them. A key a positional cannot reach, because
// one before it is left out, is written `key=`.

import type { Key } from './fields.js';
import { DATUM_SPACE, shellSpans } from './shellSplit.js';

/**
 * The verbs a field's keys are typed after — the ones the CLI gives them
 * (`takes_spec_keys`). Not `end` (da-o4jaa), vividn, 09-27: «generally
 * speaking I don't save any extra_data in this one, so I think that should
 * be the default. But allowing it might also be nice??» — so `end <field>`
 * draws no frames, and a `key=value` typed on it by hand still runs as
 * typed.
 */
const VERBS = ['occur', 'start', 'add', 'switch'];

/** A field and the keys a log of it takes (`FieldButton`'s). */
type Keyed = { field: string; keys: Key[]; stateKeys?: Record<string, Key[]> };

/** One declared key, and where its token stands on the line if it is there. */
export interface Frame extends Key {
	/** The token's UTF-16 span in the line; absent while the key is unwritten. */
	from?: number;
	to?: number;
	/** What the reader has typed into it. */
	value?: string;
}

/** The text a frame puts on the line when a positional cannot reach it. */
export const writes = (key: Key): string => `${key.name}=`;

/**
 * The frames for a line — one per key the field declares, in its order —
 * or null when the line is not `occur|start|add <field>`, or `switch
 * <field> <state>`, for a field with keys. A key is written when a token
 * after the field names it (`key=value`), or when a bare word falls to it:
 * the bare words fill the unnamed keys in order, as the CLI binds them.
 * A switch's first bare word is its state, whose own keys it takes where
 * it declares some (da-cej7q). A flag is no key's, and neither is the word
 * after one of the `valued` flags (`-t 1400`, `valuedFlags`).
 */
export function framesOf(line: string, fields: Keyed[], valued: string[] = []): Frame[] | null {
	const [verb, field, ...rest] = shellSpans(line).tokens;
	const button = VERBS.includes(verb?.text) ? fields.find((b) => b.field === field?.text) : undefined;
	const bare = rest.filter((t, i) => !t.text.includes('=') && !t.text.startsWith('-') && !valued.includes(rest[i - 1]?.text));
	const state = verb?.text === 'switch' ? bare.shift()?.text : undefined;
	const keys = state === undefined ? (verb?.text === 'switch' ? undefined : button?.keys) : (button?.stateKeys?.[state] ?? button?.keys);
	if (!keys?.length) return null;
	const frames: Frame[] = keys.map((key) => {
		const token = rest.find((t) => t.text.startsWith(writes(key)));
		return token ? { ...key, from: token.from, to: token.to, value: token.text.slice(writes(key).length) } : { ...key };
	});
	frames
		.filter((f) => f.from === undefined)
		.forEach((f, i) => bare[i] && Object.assign(f, { from: bare[i].from, to: bare[i].to, value: bare[i].text }));
	return frames;
}

/**
 * The line after a SPACE typed at its end, or null when the space is only a
 * space — which it mostly is: the next bare word falls to the next key.
 *
 * A space in a frame still EMPTY skips it (vividn, 09-26: «space advances»):
 * the key a bare word would fill next, when the line already ends in a
 * space, or a `key=` with nothing in it, which comes off. The key after it
 * is written `key=`, since a positional would fall to the one skipped —
 * bare only when the skip came round to the first unwritten key again. A
 * lone empty key stays put; inside quotes a space is literal.
 */
export function advance(line: string, fields: Keyed[], valued: string[] = []): string | null {
	const frames = framesOf(line, fields, valued);
	if (!frames || shellSpans(line).quoted) return null;
	const unwritten = frames.filter((f) => f.from === undefined);
	const skipped =
		frames.find((f) => f.value === '' && f.to === line.length) ??
		(DATUM_SPACE.test(line.slice(-1)) ? unwritten[0] : undefined);
	if (!skipped) return null;
	const open = frames.filter((f) => f === skipped || f.from === undefined);
	const at = open.indexOf(skipped);
	const next = [...open.slice(at + 1), ...open.slice(0, at)][0];
	if (!next) return null;
	const kept = skipped.from === undefined ? line : line.slice(0, skipped.from);
	return `${kept.trimEnd()} ${next === open[0] ? '' : writes(next)}`;
}

/**
 * The line as it runs: a `key=` left empty comes off, so the CLI fills a
 * default and leaves an optional absent — it applies the defaults itself
 * (da-9s98k), where an empty `key=` would clear one. Not inside an open
 * quote, where the line runs as typed.
 */
export function complete(line: string, fields: Keyed[], valued: string[] = []): string {
	const frames = framesOf(line, fields, valued);
	if (!frames || shellSpans(line).quoted) return line;
	const empty = frames.filter((f) => f.from !== undefined && line.slice(f.from, f.to) === writes(f));
	// From the right, so each span is still where it was.
	const out = empty.sort((a, b) => b.from! - a.from!).reduce((l, f) => l.slice(0, f.from) + l.slice(f.to), line);
	return empty.length ? out.trimEnd() : line;
}
