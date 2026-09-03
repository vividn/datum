// What "the same entry" means when two implementations are compared.
//
// Deliberately not the whole row. `hid`, `content_hash`, `device_id` and
// the timestamps of the write itself are per-write facts that could not
// match across two implementations and should not have to. What is left is
// what the user actually asked for — the field, its verb, its value, its
// duration, its data, and when it happened — which is exactly what has to
// agree if datum.day IS datum rather than a lookalike.
//
// Both sides normalize through here: the capture script over the CLI's
// JSONL dump, and the parity test over what the browser stored.

/**
 * @param {object} raw
 * @param {string} raw.field
 * @param {string} raw.verb
 * @param {unknown} [raw.state]
 * @param {string} [raw.dur]        ISO-8601, or the CLI's `MOMENT` sentinel
 * @param {unknown} [raw.data]
 * @param {string} raw.occurUtc     when the entry says it happened
 * @param {string} raw.createUtc    when it was written
 */
export function semantics(raw) {
	const occur = new Date(raw.occurUtc);
	const created = new Date(raw.createUtc);
	return {
		field: raw.field,
		verb: raw.verb,
		state: raw.state ?? null,
		// `MOMENT` is the CLI's in-column spelling of "no duration, this is a
		// point". `verb` already carries that and the browser has no such
		// sentinel, so it compares as absent on both sides.
		dur: raw.dur === 'MOMENT' ? null : (raw.dur ?? null),
		data: raw.data ?? null,
		time: {
			/** UTC wall clock, for a case that pinned one with `-t HHMM`. */
			clock: occur.toISOString().slice(11, 16),
			/** Whole minutes between the write and the entry's own time. */
			minutesBefore: Math.round((created - occur) / 60000),
			/** Whole UTC days between the write's date and the entry's. */
			daysBefore: Math.round(
				(Date.parse(created.toISOString().slice(0, 10)) -
					Date.parse(occur.toISOString().slice(0, 10))) /
					86400000
			),
			/** Whether the entry landed on midnight — a whole-day record. */
			midnight: occur.toISOString().slice(11) === '00:00:00.000Z'
		}
	};
}

/** The CLI's backup JSONL row → [`semantics`]. */
export function fromBackupRow(row) {
	return semantics({
		field: row.field,
		verb: row.verb,
		state: row.state,
		dur: row.dur,
		data: row.data,
		occurUtc: row.occur_time.utc,
		createUtc: row.create_time
	});
}

/** A browser `Entry` (src/lib/types.ts) → [`semantics`]. */
export function fromEntry(entry) {
	return semantics({
		field: entry.field,
		verb: entry.verb,
		state: entry.state,
		dur: entry.dur,
		data: entry.data,
		occurUtc: entry.occurTime.utc,
		createUtc: entry.createTime
	});
}

/**
 * The part of an entry's time a case actually compares.
 *
 *   clock     — the case pinned a wall clock with `-t HHMM`
 *   relative  — the case asked for "N minutes ago" (`-t -30`, `-q`, `-qq`)
 *   day       — the case asked for a whole day back (`-y`, `-yy`)
 *
 * Only the mode's own keys are kept, so a fixture holds nothing that
 * changes just because it was captured at a different moment.
 */
export function timeFor(mode, time) {
	switch (mode) {
		case 'clock':
			return { clock: time.clock };
		case 'relative':
			return { minutesBefore: time.minutesBefore };
		case 'day':
			return { daysBefore: time.daysBefore, midnight: time.midnight };
		default:
			throw new Error(`unknown time mode: ${mode}`);
	}
}

/** [`semantics`] with its time narrowed to what `mode` compares. */
export function comparable(entry, mode) {
	return { ...entry, time: timeFor(mode, entry.time) };
}
