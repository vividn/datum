// Shell-style argv splitting for a REPL line.
//
// Mirrors `crates/datum-parse/src/alias/expand.rs::shell_split`, which is
// what the native REPL feeds to the parser. The web REPL has to tokenize
// the same way or `note word correr "to run"` means something different in
// the browser than it does in the terminal. Stored as its own module so the
// rule has one source per language (TS/Rust) — update in lockstep.
//
// Single quotes are literal; double quotes and a bare backslash let the
// next character through unchanged. An unterminated quote is not an error:
// the run so far becomes the final token, so a half-typed line still
// tokenizes while the user is mid-keystroke.
export function shellSplit(line: string): string[] {
	const out: string[] = [];
	let cur = '';
	let inSingle = false;
	let inDouble = false;
	let hasTok = false;

	const chars = [...line];
	for (let i = 0; i < chars.length; i++) {
		const c = chars[i];
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
		} else if (/\s/.test(c)) {
			if (hasTok) {
				out.push(cur);
				cur = '';
				hasTok = false;
			}
		} else {
			cur += c;
			hasTok = true;
		}
	}

	if (hasTok) out.push(cur);
	return out;
}
