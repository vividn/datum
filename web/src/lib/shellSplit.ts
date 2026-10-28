// Shell-style argv splitting for a typed line.
//
// Mirrors `crates/datum-parse/src/alias/expand.rs::shell_split`, which is
// what the native REPL feeds to the parser. datum.day has to tokenize
// the same way or `note word correr "to run"` means something different in
// the browser than it does in the terminal. Stored as its own module so the
// rule has one source per language (TS/Rust) — update in lockstep, and
// keep them honest with `tests/fixtures/shell-split.json`, the case list
// both sides' tests read.
//
// Single quotes are literal; double quotes and a bare backslash let the
// next character through unchanged. An unterminated quote is not an error:
// the run so far becomes the final token, so a half-typed line still
// tokenizes while the user is mid-keystroke.

// datum's whitespace class: the codepoints that separate one argv token from
// the next. Stated here rather than inherited from the host language, because
// that is what went wrong. This module and `shell_split` disagreed at exactly
// two codepoints: `/\s/` counts U+FEFF, which Rust's Unicode `White_Space`
// does not, and `White_Space` counts U+0085 NEL, which `/\s/` does not
// (da-rx63). Neither errored — the same pasted line simply became different
// argv in the browser than in the terminal.
//
// The set is Unicode `White_Space` plus U+FEFF, the byte-order mark, which
// rides in on text pasted from a file or a web page and is never part of a
// token. U+200B zero-width space is deliberately absent: it is whitespace to
// neither language, and it stays inside its token. The same list, character
// for character, is `is_datum_space` in expand.rs — which is also where the
// field-name rule reads it from, so `./fieldName.js` imports it here rather
// than restating it.
export const DATUM_SPACE =
	/[\u0009-\u000d\u0020\u0085\u00a0\u1680\u2000-\u200a\u2028\u2029\u202f\u205f\u3000\ufeff]/;

export function shellSplit(line: string): string[] {
	return shellSpans(line).tokens.map((t) => t.text);
}

/** One token of a line and where it stands in it, in UTF-16 offsets. */
export interface Span {
	text: string;
	from: number;
	to: number;
}

/**
 * `shellSplit` with each token's place in the line, and whether the line
 * ends inside an open quote — what the key frames (`keyframes.ts`) draw
 * around, and how they know a space is literal.
 */
export function shellSpans(line: string): { tokens: Span[]; quoted: boolean } {
	const tokens: Span[] = [];
	let cur = '';
	let inSingle = false;
	let inDouble = false;
	let hasTok = false;
	let from = 0;

	const chars = [...line];
	// Where each character starts, and the line's end after the last.
	const at = [0];
	for (const c of chars) at.push(at[at.length - 1] + c.length);

	for (let i = 0; i < chars.length; i++) {
		const c = chars[i];
		if (!hasTok && !DATUM_SPACE.test(c)) from = at[i];
		if (inSingle) {
			if (c === "'") inSingle = false;
			else cur += c;
		} else if (inDouble) {
			if (c === '"') inDouble = false;
			else if (c === '\\') {
				const next = chars[++i];
				if (next !== undefined) cur += next;
			} else cur += c;
		} else if (c === "'") {
			inSingle = true;
			hasTok = true;
		} else if (c === '"') {
			inDouble = true;
			hasTok = true;
		} else if (c === '\\') {
			const next = chars[++i];
			if (next !== undefined) {
				cur += next;
				hasTok = true;
			}
		} else if (DATUM_SPACE.test(c)) {
			if (hasTok) {
				tokens.push({ text: cur, from, to: at[i] });
				cur = '';
				hasTok = false;
			}
		} else {
			cur += c;
			hasTok = true;
		}
	}

	if (hasTok) tokens.push({ text: cur, from, to: line.length });
	return { tokens, quoted: inSingle || inDouble };
}

/**
 * The inverse of `shellSplit` — `shell_join` in expand.rs, the same quoting
 * character for character: a token holding a space, a quote or a shell
 * metacharacter is double-quoted with `"` and `\` escaped, the rest are
 * bare. What writes a field into a typed line goes through this, so a
 * spaced field (`my coffee`, da-7h73h) stays one token.
 */
export const shellJoin = (tokens: string[]): string => tokens.map(shellQuote).join(' ');

function shellQuote(token: string): string {
	if (token === '') return '""';
	if (![...token].some((c) => DATUM_SPACE.test(c) || `'"\\$\`*?#|&;<>()`.includes(c))) return token;
	return `"${token.replace(/["\\]/g, '\\$&')}"`;
}
