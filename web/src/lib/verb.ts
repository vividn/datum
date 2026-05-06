// Verb derivation: pure (state, dur, followingState) → StoredVerb.
//
// Mirrors `crates/datum-core/src/verb.rs::derive_verb`. Stored as its own
// module so the truth table has one source per language (TS/Rust). Update
// in lockstep when the canonical truth table changes.

import type { StoredVerb } from './types.js';

type DurKind = 'none' | 'zero' | 'positive';

function classifyDur(dur: string | undefined): DurKind {
	if (dur === undefined) return 'none';
	const secs = parseIsoDurationSecs(dur);
	if (secs === null) {
		throw new Error(`malformed ISO-8601 duration: ${dur}`);
	}
	if (secs < 0) {
		throw new Error(`negative duration not allowed: ${dur}`);
	}
	return secs === 0 ? 'zero' : 'positive';
}

export function deriveStoredVerb(
	state: unknown,
	dur: string | undefined,
	followingState: unknown
): StoredVerb {
	const stateSet = state !== undefined;
	const fsSet = followingState !== undefined;
	const durKind = classifyDur(dur);

	// Truth table from crates/datum-core/src/verb.rs (da-5ns).
	if (!stateSet && durKind === 'none' && !fsSet) return 'point';
	if (!stateSet && durKind === 'none' && fsSet)
		throw new Error('following_state with no state and no duration is not a valid shape');
	if (!stateSet && (durKind === 'zero' || durKind === 'positive') && !fsSet) return 'block';
	if (!stateSet && (durKind === 'zero' || durKind === 'positive') && fsSet) return 'block-switch';
	if (stateSet && durKind === 'none' && !fsSet) return 'switch';
	if (stateSet && durKind === 'none' && fsSet)
		throw new Error('following_state requires a duration when state is set');
	if (stateSet && durKind === 'zero' && !fsSet) return 'point';
	if (stateSet && durKind === 'zero' && fsSet) return 'point-switch';
	if (stateSet && durKind === 'positive' && !fsSet) return 'block';
	if (stateSet && durKind === 'positive' && fsSet) return 'block-switch';
	throw new Error('unreachable');
}

// Parse ISO-8601 duration to total seconds. Years/months approximated
// (365d / 30d) — only used here for zero/positive classification; the
// canonical `dur` string is stored verbatim, never recomputed from this.
// Returns null for malformed input.
function parseIsoDurationSecs(input: string): number | null {
	let sign = 1;
	let body = input;
	if (body.startsWith('-')) {
		sign = -1;
		body = body.slice(1);
	}
	if (!body.startsWith('P')) return null;
	const rest = body.slice(1);
	if (rest.length === 0) return null;

	// Week form (PnW) is mutually exclusive with the rest of the grammar.
	if (rest.endsWith('W') && !rest.includes('T')) {
		const nStr = rest.slice(0, -1);
		const n = parseDecimal(nStr);
		if (n === null) return null;
		return sign * n * 7 * 86400;
	}

	let total = 0;
	let buf = '';
	let seenT = false;
	let hadAny = false;

	for (const c of rest) {
		if (c === 'T') {
			if (seenT || buf.length > 0) return null;
			seenT = true;
		} else if ((c >= '0' && c <= '9') || c === '.' || c === ',') {
			buf += c === ',' ? '.' : c;
		} else {
			if (buf.length === 0) return null;
			const n = parseDecimal(buf);
			if (n === null) return null;
			buf = '';
			let secs: number;
			if (!seenT && c === 'Y') secs = n * 365 * 86400;
			else if (!seenT && c === 'M') secs = n * 30 * 86400;
			else if (!seenT && c === 'D') secs = n * 86400;
			else if (seenT && c === 'H') secs = n * 3600;
			else if (seenT && c === 'M') secs = n * 60;
			else if (seenT && c === 'S') secs = n;
			else return null;
			total += secs;
			hadAny = true;
		}
	}

	if (buf.length > 0) return null;
	if (!hadAny) return null;
	return sign * total;
}

function parseDecimal(s: string): number | null {
	if (s.length === 0) return null;
	const digitCount = [...s].filter((c) => c >= '0' && c <= '9').length;
	if (digitCount === 0) return null;
	const n = Number(s);
	return isNaN(n) ? null : n;
}
