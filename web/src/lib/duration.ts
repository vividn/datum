// A stored `dur` → seconds, for drawing it.
//
// Reading a duration *argument* is the parser's job, and the parser is
// datum's own: `dur=30m`, `dur=1:30` and `dur=PT30M` all reach the entry
// through `crates/datum-parse` compiled to wasm (da-7scm), so nothing here
// interprets what a user typed. What is left is the display side — the
// dayview and tail panes need a block's width, and `entries.dur` holds
// ISO-8601 (schema.sql).
//
// The non-ISO forms are still accepted below because `durationSeconds` is
// also handed values that came from elsewhere in the app; the colon form is
// anchored by part count and its two-part case is HH:MM, not MM:SS, so
// `1:30` is an hour and a half — matching `parse_duration` in
// crates/datum-parse/src/time/dur.rs exactly (a deliberate divergence from
// datum-v2).

const UNIT_SECONDS: Record<string, number> = {
	s: 1,
	sec: 1,
	secs: 1,
	second: 1,
	seconds: 1,
	m: 60,
	min: 60,
	mins: 60,
	minute: 60,
	minutes: 60,
	h: 3600,
	hr: 3600,
	hrs: 3600,
	hour: 3600,
	hours: 3600,
	d: 86400,
	day: 86400,
	days: 86400
};

/** Parse a duration to whole seconds, or null if it isn't one. */
export function durationSeconds(input: string): number | null {
	const s = input.trim();
	if (s === '') return null;

	if (s.startsWith('P') || s.startsWith('-P')) return isoSeconds(s);
	if (s.includes(':')) return colonSeconds(s);
	return compoundSeconds(s);
}

function colonSeconds(s: string): number | null {
	const parts = s.split(':').map((p) => Number(p));
	if (parts.some((n) => !Number.isFinite(n))) return null;
	// The rightmost field is minutes for the 2-part form and seconds for the
	// longer ones — see the Rust contract cited above.
	switch (parts.length) {
		case 2:
			return (parts[0] * 60 + parts[1]) * 60;
		case 3:
			return parts[0] * 3600 + parts[1] * 60 + parts[2];
		case 4:
			return parts[0] * 86400 + parts[1] * 3600 + parts[2] * 60 + parts[3];
		default:
			return null;
	}
}

/** `90` (bare minutes), `30m`, `1h30m`, `-15m`. */
function compoundSeconds(s: string): number | null {
	const match = [...s.matchAll(/(\d+(?:\.\d+)?)\s*([a-z]*)/gi)];
	if (match.length === 0) return null;
	// Reject anything the segments don't fully cover ("2 cups", "happy").
	if (match.reduce((n, m) => n + m[0].length, 0) !== s.replace(/^[+-]/, '').length) return null;

	let total = 0;
	for (const [, num, unit] of match) {
		// A bare number defaults to minutes, matching `parse_duration`'s "5".
		const seconds = unit === '' ? 60 : UNIT_SECONDS[unit.toLowerCase()];
		if (seconds === undefined) return null;
		total += Number(num) * seconds;
	}
	return s.startsWith('-') ? -total : total;
}

function isoSeconds(s: string): number | null {
	const m = /^(-?)P(?:(\d+(?:\.\d+)?)D)?(?:T(?:(\d+(?:\.\d+)?)H)?(?:(\d+(?:\.\d+)?)M)?(?:(\d+(?:\.\d+)?)S)?)?$/.exec(
		s
	);
	if (!m) return null;
	const [, sign, d, h, min, sec] = m;
	if (!d && !h && !min && !sec) return null;
	const total = Number(d ?? 0) * 86400 + Number(h ?? 0) * 3600 + Number(min ?? 0) * 60 + Number(sec ?? 0);
	return sign === '-' ? -total : total;
}
