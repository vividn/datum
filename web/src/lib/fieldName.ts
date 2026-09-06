// What a field name may be.
//
// Mirrors `crates/datum-parse/src/field_name.rs`, which is what actually
// enforces the rule — `spec set`'s parser refuses a bad name, and the (+)
// form's declaration goes through that parser like everything else. This
// module exists so the form can say *before* the press what the parser
// would say after it, and offer the nearest name that works: a first-time
// user typing "my coffee" should not have to read an error to learn that a
// field name is one token.
//
// Because it is a second copy of a rule, it is held to the fixture the way
// `shellSplit` is: `tests/fixtures/field-name.json` is the case list both
// this module's tests and `crates/datum-parse/tests/field_name_fixture.rs`
// read. Add a case there, not here.
//
// datum's whitespace class is `DATUM_SPACE` in `./shellSplit.js` — the same
// list the tokenizer splits on, stated once. A name that contains one of
// those characters is more than one argv token when typed, which is exactly
// why it cannot be a field.

import { DATUM_SPACE } from './shellSplit.js';

/**
 * Characters a field name may not contain: `=` opens a data key, `,` a list
 * or an entry chain, `:` is the slug's own boundary.
 */
const FORBIDDEN = /[=,:]/;

/** The rule, as a sentence — `field_name::RULE` in Rust. */
const RULE =
	'a field name is one word: no spaces, equals, commas or colons, and it cannot start with a dash';

/** Is `name` a field name that can be logged to? */
export function fieldNameOk(name: string): boolean {
	if (name === '') return false;
	if (name.startsWith('-')) return false;
	return ![...name].some((c) => DATUM_SPACE.test(c) || FORBIDDEN.test(c));
}

/**
 * The nearest usable name, or null when nothing usable is left.
 *
 * Every run of forbidden characters becomes a single `_`, and leading
 * `-`/`_` and trailing `_` are dropped: `my coffee` → `my_coffee`,
 * `cups=2` → `cups_2`, `-weird` → `weird`.
 */
export function suggestFieldName(name: string): string | null {
	let out = '';
	for (const c of name) {
		if (DATUM_SPACE.test(c) || FORBIDDEN.test(c)) {
			if (!out.endsWith('_')) out += '_';
		} else {
			out += c;
		}
	}
	const fixed = out.replace(/_+$/, '').replace(/^[-_]+/, '');
	return fixed === '' ? null : fixed;
}

/**
 * What to tell someone who typed `name` into the (+) form: null when it is
 * fine, otherwise the sentence to show them.
 */
export function fieldNameHint(name: string): string | null {
	if (fieldNameOk(name)) return null;
	const fixed = suggestFieldName(name);
	return fixed === null ? RULE : `${RULE}. Try ${fixed}`;
}
