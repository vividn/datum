// The TypeScript runner — the seam's default implementation.
//
// It covers the log verbs (`occur`, `switch`, `start`, `stop`, `add`,
// `note`) against the OPFS database, which is what the app needs to be
// useful today. Every other datum command returns
// `EXIT_UNKNOWN_COMMAND`: this is a stand-in for the real CLI, not a second
// implementation of it, and da-1vat retires the whole file by compiling
// `datum-cli` to wasm and installing it via `setRunner`.

import { insertEntry } from '../db.js';
import { parseArgv } from '../parse.js';
import { generateId, buildSlug, contentHash } from '../entryId.js';
import { deriveStoredVerb } from '../verb.js';
import { EXIT_DUPLICATE, EXIT_UNKNOWN_COMMAND, type RunResult } from '../run.js';
import { isAnnotated, type Entry } from '../types.js';

const DEVICE_ID_KEY = 'datum-device-id';

/** This browser's device id, minted on first use and kept in localStorage. */
export function deviceId(): string {
	const existing = localStorage.getItem(DEVICE_ID_KEY);
	if (existing) return existing;
	const minted = 'web-' + Math.random().toString(36).slice(2, 10);
	localStorage.setItem(DEVICE_ID_KEY, minted);
	return minted;
}

export async function localRun(argv: string[]): Promise<RunResult> {
	const parsed = parseArgv(argv);
	if (!parsed) {
		return {
			argv,
			code: EXIT_UNKNOWN_COMMAND,
			stdout: '',
			stderr: `datum: ${argv[0]}: not available yet in the browser (needs the wasm CLI)`
		};
	}

	const entry = await buildEntry(parsed, argv.join(' '));
	try {
		await insertEntry(entry);
	} catch (e) {
		const message = e instanceof Error ? e.message : String(e);
		// The slug is UNIQUE (schema.sql `entries_slug`), so re-logging the
		// same field at the same minute collides. The CLI reports that as
		// exit 11 so idempotent loggers can treat it as a benign skip.
		const duplicate = /UNIQUE|constraint/i.test(message);
		return {
			argv,
			code: duplicate ? EXIT_DUPLICATE : 1,
			stdout: '',
			stderr: duplicate ? `datum: already have ${entry.slug}` : `error: ${message}`
		};
	}

	return { argv, code: 0, stdout: entryLine(entry), stderr: '' };
}

async function buildEntry(
	parsed: NonNullable<ReturnType<typeof parseArgv>>,
	rawInput: string
): Promise<Entry> {
	const now = new Date();
	let entryTime = now;
	if (parsed.timeOffset !== undefined) {
		entryTime = new Date(now.getTime() + parsed.timeOffset * 60 * 1000);
	} else if (parsed.timeAbsolute) {
		const [hStr, mStr] = parsed.timeAbsolute.match(/^(\d{2})(\d{2})$/)?.slice(1) ?? [];
		if (hStr && mStr) {
			entryTime = new Date(now);
			entryTime.setHours(parseInt(hStr), parseInt(mStr), 0, 0);
		}
	}

	// A unit rides inside the value, `{"v": 75.2, "u": "kg"}` — there is no
	// `unit` column in schema.sql. Same annotation `coerce.rs` builds for a
	// `weight=75.2kg` data token on the Rust side.
	const state =
		parsed.unit !== undefined && typeof parsed.state === 'number'
			? { v: parsed.state, u: parsed.unit }
			: parsed.state;

	const occurTime = {
		utc: entryTime.toISOString(),
		offsetMinutes: -now.getTimezoneOffset(),
		timezone: Intl.DateTimeFormat().resolvedOptions().timeZone
	};

	return {
		hid: generateId(),
		slug: buildSlug(parsed.field, occurTime),
		contentHash: await contentHash(
			parsed.field,
			occurTime,
			state,
			undefined,
			undefined,
			parsed.data
		),
		field: parsed.field,
		state,
		data: parsed.data,
		verb: deriveStoredVerb(state, undefined, undefined),
		occurTime,
		createTime: now.toISOString(),
		modifyTime: now.toISOString(),
		deviceId: deviceId(),
		rawInput
	};
}

/**
 * One line of stdout for a logged entry.
 *
 * A plain-text stand-in for `render_entry_line`
 * (crates/datum-commands/src/render/action.rs) — same shape (time, field,
 * state) without the field badges and colour the real renderer brings.
 */
export function entryLine(entry: Entry): string {
	const time = new Date(entry.occurTime.utc).toLocaleTimeString([], {
		hour: '2-digit',
		minute: '2-digit'
	});
	const parts = [time, entry.field];
	if (entry.state !== undefined) parts.push(`= ${formatValue(entry.state)}`);
	if (entry.data) {
		for (const [k, v] of Object.entries(entry.data as Record<string, unknown>)) {
			parts.push(`${k}=${formatValue(v)}`);
		}
	}
	return parts.join(' ');
}

/** Render a stored value, unwrapping the `{v, u}` unit annotation. */
export function formatValue(value: unknown): string {
	if (isAnnotated(value)) return `${value.v}${value.u}`;
	return typeof value === 'string' ? value : JSON.stringify(value);
}
