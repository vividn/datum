// Duration parsing: a datum duration argument → an ISO-8601 string.
//
// `entries.dur` stores ISO-8601 (schema.sql), but nobody types that. The
// CLI accepts `30m`, `1h30m`, `1:30`, a bare `5` (minutes) and ISO itself
// — see `parse_duration` in crates/datum-parse/src/time/dur.rs, which this
// mirrors. The widget bar's `dur` bubble and a hand-typed `dur=1:30` land
// in the same place, so they have to read the same way.
//
// The colon form is anchored by part count and the two-part case is
// HH:MM, not MM:SS: `1:30` is an hour and a half, matching the Rust
// contract exactly (it is a deliberate divergence from datum-v2).

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

/** Parse a duration argument to whole seconds, or null if it isn't one. */
export function durationSeconds(input: string): number | null {
	const s = input.trim();
	if (s === '') return null;

	if (s.startsWith('P') || s.startsWith('-P')) return isoSeconds(s);
	if (s.includes(':')) return colonSeconds(s);
	return compoundSeconds(s);
}

/**
 * Normalize a duration argument to the ISO-8601 string the entry stores.
 * Returns null when the argument isn't a duration at all, so a caller can
 * fall back to treating the token as something else.
 */
export function toIsoDuration(input: string): string | null {
	const secs = durationSeconds(input);
	return secs === null ? null : isoFromSeconds(secs);
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

/** Seconds → the canonical ISO-8601 form (`PT1H30M`). */
export function isoFromSeconds(total: number): string {
	const sign = total < 0 ? '-' : '';
	let rest = Math.abs(total);
	const days = Math.floor(rest / 86400);
	rest -= days * 86400;
	const hours = Math.floor(rest / 3600);
	rest -= hours * 3600;
	const minutes = Math.floor(rest / 60);
	const seconds = Number((rest - minutes * 60).toFixed(3));

	const time =
		(hours ? `${hours}H` : '') + (minutes ? `${minutes}M` : '') + (seconds ? `${seconds}S` : '');
	// PT0S rather than a bare "P": a zero duration is still a duration, and
	// `deriveStoredVerb` distinguishes zero from absent.
	if (!days && !time) return `${sign}PT0S`;
	return `${sign}P${days ? `${days}D` : ''}${time ? `T${time}` : ''}`;
}
