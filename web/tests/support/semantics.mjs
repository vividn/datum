// What "the same entry" means when two implementations are compared.
//
// Deliberately not the whole row. `hid`, `content_hash`, `client_id` and
// the timestamps of the write itself are per-write facts that could not
// match across two implementations and should not have to. What is left is
// what the user actually asked for — the field, its verb, its value, its
// duration, its data, when it happened, the identity it was filed under,
// and the provenance of the line that wrote it — which is exactly what has
// to agree if datum.day IS datum rather than a lookalike.
//
// The slug carries a clock in the common case (`coffee:2026-03-16T09:15…`),
// and a clock is a per-write fact, so the time inside it is masked. What
// survives the mask is the part a case can actually pin: `tx:electricity`
// when an `--id %comment%` template put the data there (da-5kbj).
//
// Both sides normalize through here, and from the same shape: the capture
// script reads the native CLI's JSONL dump, and the parity test reads the
// browser's — `window.datum.entries()` is `db backup -` too.

/**
 * @param {object} raw
 * @param {string} raw.field
 * @param {string} raw.verb
 * @param {unknown} [raw.state]
 * @param {string} [raw.dur]        ISO-8601, or the CLI's `MOMENT` sentinel
 * @param {unknown} [raw.data]
 * @param {string} raw.slug         the identity the row was filed under
 * @param {string} [raw.rawInput]   the line the user typed, pre-expansion
 * @param {unknown} [raw.aliasChain] the alias expansions it went through
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
		slug: maskClock(raw.slug),
		rawInput: raw.rawInput ?? null,
		aliasChain: raw.aliasChain ?? null,
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

/**
 * A slug with any timestamp in it replaced by `<t>`.
 *
 * The default id template is `%occur_time%`, so most slugs are a clock —
 * `coffee:2026-03-16T09:15:00.000Z`, or the bare local date for a
 * whole-day row. Neither is comparable across two writes, and the case's
 * `time` block already checks the instant. What the mask leaves behind is
 * whether the slug's body came from somewhere else.
 */
function maskClock(slug) {
	return String(slug ?? '').replace(/c?\d{4}-\d{2}-\d{2}(T[\d:.]+Z?)?/g, '<t>');
}

/** The CLI's backup JSONL row → [`semantics`]. */
export function fromBackupRow(row) {
	return semantics({
		field: row.field,
		verb: row.verb,
		state: row.state,
		dur: row.dur,
		data: row.data,
		slug: row.slug,
		rawInput: row.raw_input,
		aliasChain: row.alias_chain,
		occurUtc: row.occur_time.utc,
		createUtc: row.create_time
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
