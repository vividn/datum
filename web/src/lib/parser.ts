// The parser: datum's own, in the browser.
//
// datum.day used to carry a second parser written in TypeScript. In under
// two days it produced `-q`/`-qq` off by a factor of three, `-y`/`-yy`
// reading DAY offsets as MINUTES, and `-y` stored as `state="-y"` — data
// corruption in a tool whose whole purpose is recording when things
// happened. Nothing keeps a hand-written second parser in agreement with
// the first, so it is gone (da-7scm): `crates/datum-parse-wasm` is
// `datum-parse` behind the same C ABI the layout module uses, and this file
// is the other half of it.
//
// argv in, the row to insert out. Everything about *how* an invocation is
// read — the time flags, the `KEY=VALUE` grammar, the duration spellings,
// the verb truth table, the slug and the content hash — stays in Rust,
// where the CLI's copy of the rule already lives. The browser supplies only
// what it alone knows: the clock, the viewer's timezone, this device's id,
// the random bytes a hid is made of — and, since da-5kbj, the rows of its
// own `aliases` table and its own field specs, so that expanding an alias
// is `datum-parse`'s job here too and not a second expander in TypeScript.

import { jsonModule } from './wasm.js';
import { getAliasRows, getFieldSpecs } from './db.js';
// `?url` so Vite emits it content-hashed under /_app/immutable/ — see wasm.ts.
import parseWasm from './wasm/datum-parse.wasm?url';
import type { Entry, FieldSpec } from './types.js';

/** What the module needs from the browser for one `run(argv)`. */
interface ParseRequest {
	/** Command words, without the program name — what the seam passes around. */
	argv: string[];
	now_ms: number;
	/** The viewer's IANA zone, when the browser can name one. */
	timezone: string | null;
	/** The viewer's UTC offset in minutes, east-positive. */
	offset_minutes: number;
	device_id: string;
	/** 12 bytes from `crypto.getRandomValues`. */
	hid_bytes: number[];
	/**
	 * Every row of the `aliases` table, as stored. The expansion itself —
	 * the `%key%` regions, `$N` positionals, the hop cap, the cycle check —
	 * is `datum-parse`'s, so these rows are all the browser contributes.
	 */
	aliases: { name: string; alias_json: string }[];
	/** Every stored field spec, in `rowid` order; the spec-derived aliases
	 *  (`sleep` → `start sleep`) are built from these, Rust-side. */
	field_specs: FieldSpec[];
}

/**
 * The write a `datum alias` command asks for, plus what it says.
 *
 * `save` and `delete` are mutually exclusive and both absent for `alias
 * list` / `alias show`, which answer with `output` alone.
 */
export interface AliasInstruction {
	save?: { name: string; aliasJson: string };
	delete?: string;
	output: string;
	warning?: string | null;
}

/**
 * Exactly one of these four is present.
 *
 * `unsupported` is a well-formed command the module does not build an entry
 * for — every other datum command, and the log shapes the web app cannot
 * hold yet. `output` is a command whose whole answer is text the caller
 * prints with exit 0 — `--help`, `--version`, `--man`; the module decides
 * that on the parser's own variant, so the browser never has to recognise
 * one by its message. `error` is the CLI's own message for an argv that did
 * not parse, so the browser reports what `datum` would have reported.
 */
interface ParseResponse {
	entry?: Entry;
	/** A `datum alias` command: the row to write, and what to print. */
	alias?: AliasInstruction;
	unsupported?: string;
	output?: string;
	error?: string;
}

const call = jsonModule<ParseRequest, ParseResponse>(parseWasm, 'parse');

/**
 * Parse one argv into the entry to store.
 *
 * Rejects only when the module itself cannot be reached; a command it does
 * not handle and an argv that does not parse both come back as a result the
 * caller renders.
 */
export async function parseArgv(argv: string[], deviceId: string): Promise<ParseResponse> {
	// The alias world, read fresh for every command: `alias set` in the
	// REPL must fire on the very next line without a reload, and an alias
	// synced in from another device the same way.
	const [aliases, specs] = await Promise.all([getAliasRows(), getFieldSpecs()]);
	return call({
		argv,
		now_ms: Date.now(),
		// `resolvedOptions().timeZone` is an IANA name in every browser that
		// supports the PWA at all. Null is still handled: the module falls
		// back to the raw offset, which is self-verifying where a name is a
		// claim — it never invents "UTC" (da-2pgd).
		timezone: Intl.DateTimeFormat().resolvedOptions().timeZone ?? null,
		offset_minutes: -new Date().getTimezoneOffset(),
		device_id: deviceId,
		hid_bytes: Array.from(crypto.getRandomValues(new Uint8Array(12))),
		aliases,
		field_specs: [...specs.values()]
	});
}
