import type { ParsedInput, Verb } from './types.js';

const VERBS: Verb[] = ['occur', 'start', 'stop', 'switch', 'note'];

function isVerb(s: string): s is Verb {
	return (VERBS as string[]).includes(s);
}

function parseValue(s: string): unknown {
	// Try number first
	const num = Number(s);
	if (!isNaN(num) && s.trim() !== '') return num;
	// Try boolean
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

export function parseInput(input: string): ParsedInput | null {
	const raw = input.trim();
	if (!raw) return null;

	const tokens = raw.split(/\s+/);
	if (tokens.length === 0) return null;

	let verb: Verb = 'occur';
	let field = '';
	const extraData: Record<string, unknown> = {};
	const valueTokens: string[] = [];
	let unit: string | undefined;
	let timeOffset: number | undefined;
	let timeAbsolute: string | undefined;
	let i = 0;

	// Detect verb and field (order-independent)
	if (isVerb(tokens[0])) {
		verb = tokens[0];
		field = tokens[1] ?? '';
		i = 2;
	} else if (tokens.length > 1 && isVerb(tokens[1])) {
		field = tokens[0];
		verb = tokens[1];
		i = 2;
	} else {
		// bare field or "field value..."
		field = tokens[0];
		verb = 'occur';
		i = 1;
	}

	if (!field) return null;

	// Parse remaining tokens
	while (i < tokens.length) {
		const tok = tokens[i];

		// Time flags
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

		// key=value extra data
		const kv = parseKV(tok);
		if (kv) {
			extraData[kv.key] = kv.val;
			i++;
			continue;
		}

		valueTokens.push(tok);
		i++;
	}

	// Parse value and unit from valueTokens
	let value: unknown;
	if (valueTokens.length === 0) {
		value = undefined;
	} else if (valueTokens.length === 1) {
		value = parseValue(valueTokens[0]);
	} else {
		// Last token might be a unit if value is numeric
		const maybeNum = parseValue(valueTokens[0]);
		if (typeof maybeNum === 'number' && valueTokens.length === 2) {
			value = maybeNum;
			unit = valueTokens[1];
		} else {
			value = valueTokens.join(' ');
		}
	}

	return {
		verb,
		field,
		value,
		unit,
		extraData: Object.keys(extraData).length > 0 ? extraData : undefined,
		timeOffset,
		timeAbsolute
	};
}
