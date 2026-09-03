import type { ParsedInput, InputVerb } from './types.js';

/** The input verbs the local runner accepts, in either argv position. */
export const LOG_VERBS: readonly InputVerb[] = ['occur', 'switch', 'start', 'stop', 'add', 'note'];

export function isVerb(s: string): s is InputVerb {
	return (LOG_VERBS as readonly string[]).includes(s);
}

function parseValue(s: string): unknown {
	const num = Number(s);
	if (!isNaN(num) && s.trim() !== '') return num;
	if (s === 'true') return true;
	if (s === 'false') return false;
	return s;
}

function parseKV(s: string): { key: string; val: unknown } | null {
	const idx = s.indexOf('=');
	if (idx < 0) return null;
	const key = s.slice(0, idx);
	const val = parseValue(s.slice(idx + 1));
	return { key, val };
}

/**
 * Parse a log invocation's argv into a [`ParsedInput`].
 *
 * Returns null when argv is not a log invocation at all — no verb in either
 * of the two positions the CLI accepts one in. The CLI has no bare-field
 * form (`maybe_swap_field_verb` in crates/datum-parse/src/alias/expand.rs
 * only swaps when one of the first two tokens is a known subcommand), so
 * neither does this: `tail` is a command this parser doesn't own, not a
 * field named "tail".
 */
export function parseArgv(tokens: string[]): ParsedInput | null {
	if (tokens.length === 0) return null;

	let verb: InputVerb;
	let field = '';
	const data: Record<string, unknown> = {};
	const stateTokens: string[] = [];
	let unit: string | undefined;
	let timeOffset: number | undefined;
	let timeAbsolute: string | undefined;
	let i = 0;

	// datum CLI accepts both verb-first ("switch mood happy") and field-first
	// ("mood switch happy") syntax; resolve which order this input uses before
	// consuming the rest.
	if (isVerb(tokens[0])) {
		verb = tokens[0];
		field = tokens[1] ?? '';
		i = 2;
	} else if (tokens.length > 1 && isVerb(tokens[1])) {
		field = tokens[0];
		verb = tokens[1];
		i = 2;
	} else {
		return null;
	}

	if (!field) return null;

	while (i < tokens.length) {
		const tok = tokens[i];

		if (tok === '-q') {
			timeOffset = -15;
			i++;
			continue;
		}
		if (tok === '-qq') {
			timeOffset = -30;
			i++;
			continue;
		}
		if (tok === '-yy') {
			timeOffset = -60;
			i++;
			continue;
		}
		if (tok === '-d' && i + 1 < tokens.length) {
			const days = parseInt(tokens[i + 1]);
			if (!isNaN(days)) {
				timeOffset = days * 24 * 60;
				i += 2;
				continue;
			}
		}
		if (tok === '-t' && i + 1 < tokens.length) {
			timeAbsolute = tokens[i + 1];
			i += 2;
			continue;
		}

		const kv = parseKV(tok);
		if (kv) {
			data[kv.key] = kv.val;
			i++;
			continue;
		}

		stateTokens.push(tok);
		i++;
	}

	let state: unknown;
	if (stateTokens.length === 0) {
		state = undefined;
	} else if (stateTokens.length === 1) {
		state = parseValue(stateTokens[0]);
	} else {
		// Treat exactly two trailing tokens as a (number, unit) pair (e.g.
		// "weight 75kg" → state=75, unit="kg"). Anything else collapses to a
		// space-joined free-text state — units only attach to numeric values.
		const maybeNum = parseValue(stateTokens[0]);
		if (typeof maybeNum === 'number' && stateTokens.length === 2) {
			state = maybeNum;
			unit = stateTokens[1];
		} else {
			state = stateTokens.join(' ');
		}
	}

	return {
		verb,
		field,
		state,
		unit,
		data: Object.keys(data).length > 0 ? data : undefined,
		timeOffset,
		timeAbsolute
	};
}
