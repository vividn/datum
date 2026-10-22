// What a field name may be.
//
// Mirrors `crates/datum-parse/src/field_name.rs`, which is what actually
// enforces the rule — `setup`'s parser refuses a bad name, and the setup
// widget's declaration goes through that parser like everything else. This
// module exists so the form can say *before* the press what the parser
// would say after it, and offer the nearest name that works: a first-time
// user typing "cups=2" should not have to read an error to learn what a
// field name may be.
//
// Because it is a second copy of a rule, it is held to the fixture the way
// `shellSplit` is: `tests/fixtures/field-name.json` is the case list both
// this module's tests and `crates/datum-parse/tests/field_name_fixture.rs`
// read. Add a case there, not here.
//
// A space inside a name is fine (da-7h73h): every typed line that carries a
// field quotes it with `shellJoin`. One at either end is not — ` coffee`
// reads as `coffee` and is another field. datum's whitespace class is
// `DATUM_SPACE` in `./shellSplit.js`, stated once.

import { DATUM_SPACE } from './shellSplit.js';

/**
 * Characters a field name may not contain: `=` opens a data key, `,` a list
 * or an entry chain, `:` is the slug's own boundary.
 */
const FORBIDDEN = /[=,:]/;

/** The rule, as a sentence — `field_name::RULE` in Rust. */
const RULE =
	'a field name has no equals, commas or colons, and cannot start with a dash or start or end with a space';

const space = (c: string | undefined): boolean => c !== undefined && DATUM_SPACE.test(c);

/** Is `name` a field name that can be logged to? */
export function fieldNameOk(name: string): boolean {
	const chars = [...name];
	return name !== '' && !name.startsWith('-') && !space(chars[0]) && !space(chars.at(-1)) && !FORBIDDEN.test(name);
}

/**
 * The nearest usable name, or null when nothing usable is left.
 *
 * Every run of forbidden characters becomes a single `_`, and spaces and
 * `_` at either end and a leading `-` are dropped: `cups=2` → `cups_2`,
 * `-weird` → `weird`, ` my coffee ` → `my coffee`.
 */
export function suggestFieldName(name: string): string | null {
	const chars: string[] = [];
	for (const c of name) {
		if (!FORBIDDEN.test(c)) chars.push(c);
		else if (chars.at(-1) !== '_') chars.push('_');
	}
	while (chars.length && (space(chars[0]) || chars[0] === '-' || chars[0] === '_')) chars.shift();
	while (chars.length && (space(chars.at(-1)) || chars.at(-1) === '_')) chars.pop();
	return chars.length ? chars.join('') : null;
}

/**
 * What to tell someone who typed `name` into the setup widget: null when it is
 * fine, otherwise the sentence to show them.
 */
export function fieldNameHint(name: string): string | null {
	if (fieldNameOk(name)) return null;
	const fixed = suggestFieldName(name);
	return fixed === null ? RULE : `${RULE}. Try ${fixed}`;
}
