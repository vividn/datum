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
// and the random bytes a hid is made of.

import { jsonModule } from './wasm.js';
import type { Entry } from './types.js';

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
}

/**
 * Exactly one of these three is present.
 *
 * `unsupported` is a well-formed command the module does not build an entry
 * for — every other datum command, and the log shapes the web app cannot
 * hold yet. `error` is the CLI's own message for an argv that did not
 * parse, so the browser reports what `datum` would have reported.
 */
interface ParseResponse {
	entry?: Entry;
	unsupported?: string;
	error?: string;
}

const call = jsonModule<ParseRequest, ParseResponse>('datum-parse.wasm', 'parse');

/**
 * Parse one argv into the entry to store.
 *
 * Rejects only when the module itself cannot be reached; a command it does
 * not handle and an argv that does not parse both come back as a result the
 * caller renders.
 */
export function parseArgv(argv: string[], deviceId: string): Promise<ParseResponse> {
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
		hid_bytes: Array.from(crypto.getRandomValues(new Uint8Array(12)))
	});
}
