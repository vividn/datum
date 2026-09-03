// Which marks an entry makes on a timeline.
//
// This is the browser's copy of one rule and only one: the verb→shape
// dispatch that decides whether an entry is a band, a dot, or nothing at
// all. Mirrors `render_field_visuals` in
// crates/datum-commands/src/commands/field_visuals.rs and the truth table
// it dispatches on (crates/datum-core/src/shape.rs). Update in lockstep,
// the way $lib/verb.ts tracks `derive_verb`.
//
// Everything downstream of the shape — the colours, the geometry, the
// clip, the axis — stays in Rust: the marks built here are handed to
// datum-layout through $lib/layout.ts, so the browser never picks a pixel
// or a hex value of its own (da-6h6p).

import type { BlockReq, FieldReq, FieldStyle, PointReq } from './layout.js';
import type { Entry, FieldSpec, StoredVerb } from './types.js';
import { durationSeconds } from './duration.js';

/** The sentinel that reads as "no state here" (`IGNORED_STATE_VALUE`). */
const IGNORED = 'IGNORED';

/** Verbs whose occur_time updates the field's prevailing state. */
function isSwitchingVerb(verb: StoredVerb): boolean {
	return verb === 'switch' || verb === 'block-switch' || verb === 'point-switch';
}

/** Verbs that draw a band across `[occur - dur, occur]`. */
function isBlockVerb(verb: StoredVerb): boolean {
	return verb === 'block' || verb === 'block-switch';
}

/** Verbs that additionally open a trailing region from `following_state`. */
function opensSecondarySwitchRegion(verb: StoredVerb): boolean {
	return verb === 'block-switch' || verb === 'point-switch';
}

/** A state value that terminates rather than opens a region. */
export function isUnrenderedSwitchValue(v: unknown): boolean {
	return v === undefined || v === null || v === false || v === IGNORED;
}

/** `entry.shape()`: a duration that spans time makes an entry a block. */
function isBlockShaped(entry: Entry): boolean {
	const secs = entry.dur === undefined ? null : durationSeconds(entry.dur);
	return secs !== null && secs > 0;
}

type RenderShape = 'hidden' | 'marker' | 'region' | 'dot';

/** The (verb, shape, state) → mark table. Port of `shape.rs::render_shape`. */
function renderShape(entry: Entry): RenderShape {
	switch (entry.verb) {
		case 'switch':
			return isUnrenderedSwitchValue(entry.state) ? 'hidden' : 'region';
		case 'block':
		case 'block-switch':
			return 'region';
		case 'point-switch':
			return 'dot';
		case 'point':
			return isBlockShaped(entry) ? 'region' : 'dot';
	}
}

function occurMs(entry: Entry): number {
	return Date.parse(entry.occurTime.utc);
}

/**
 * When each switching entry's region ends: at the next switching entry,
 * or at `fallbackEndMs` for the last one. `entries` must be sorted by
 * occur_time ascending. Port of `field_visuals::switch_region_end_ms`.
 */
function switchRegionEnds(entries: Entry[], fallbackEndMs: number): number[] {
	const ends = new Array<number>(entries.length).fill(fallbackEndMs);
	let nextOccur: number | null = null;
	for (let i = entries.length - 1; i >= 0; i--) {
		if (!isSwitchingVerb(entries[i].verb)) continue;
		ends[i] = nextOccur ?? fallbackEndMs;
		nextOccur = occurMs(entries[i]);
	}
	return ends;
}

/**
 * The state one entry leaves in force at `atMs`. Port of
 * `db::now::effective_state`: past the trailing edge of a compound
 * switching verb it is `following_state` that holds, not `state`.
 */
export function effectiveState(entry: Entry, atMs: number): unknown {
	if (opensSecondarySwitchRegion(entry.verb) && entry.followingState !== undefined) {
		const secs = entry.dur === undefined ? null : durationSeconds(entry.dur);
		if (secs !== null && atMs >= occurMs(entry) + secs * 1000) return entry.followingState;
	}
	return entry.state ?? null;
}

/**
 * The field's prevailing state at `atMs`, or undefined when nothing in
 * `entries` has set one. The in-memory form of `Db::state_at`: newest
 * entry at or before the instant, ties broken by slug the way
 * `STATE_ORDER_NEWEST_FIRST` breaks them, then [`effectiveState`].
 *
 * Bounded by what the caller fetched, where the CLI's query is bounded by
 * the whole store: a dot whose prevailing state was set before the fetch
 * window reads as "none" here and paints black. The panel and the nowview
 * leading region, where that difference is visible rather than a shade of
 * one dot's background, ask the database directly instead
 * ($lib/db.ts `getStateEntriesAt`).
 */
export function stateAt(entries: Entry[], atMs: number): unknown {
	let winner: Entry | undefined;
	for (const entry of entries) {
		const ms = occurMs(entry);
		if (!Number.isFinite(ms) || ms > atMs) continue;
		if (
			winner === undefined ||
			ms > occurMs(winner) ||
			(ms === occurMs(winner) && entry.slug > winner.slug)
		) {
			winner = entry;
		}
	}
	return winner === undefined ? undefined : effectiveState(winner, atMs);
}

/** The first switching entry in `entries`, whose region ends the lead-in. */
export function firstSwitchMs(entries: Entry[]): number | undefined {
	for (const entry of entries) {
		if (isSwitchingVerb(entry.verb)) return occurMs(entry);
	}
	return undefined;
}

/** The drawn part of a stored field spec, flattened the way Rust reads it. */
export function styleOf(spec: FieldSpec | undefined): FieldStyle | undefined {
	if (!spec) return undefined;
	const presentation = (spec.presentation ?? {}) as Record<string, unknown>;
	const style: FieldStyle = {};
	if (typeof spec.color === 'string') style.color = spec.color;
	if (spec.state_colors && typeof spec.state_colors === 'object') {
		style.state_colors = spec.state_colors as Record<string, string>;
	}
	if (typeof presentation.y === 'number') style.y = presentation.y;
	if (typeof presentation.height === 'number') style.height = presentation.height;
	if (typeof presentation.z_index === 'number') style.z_index = presentation.z_index;
	return Object.keys(style).length > 0 ? style : undefined;
}

/** A band, clipped away when it falls entirely outside the window. */
function pushRegion(
	blocks: BlockReq[],
	state: unknown,
	startMs: number,
	endMs: number,
	windowStartMs: number,
	windowEndMs: number
): void {
	if (isUnrenderedSwitchValue(state)) return;
	if (endMs <= windowStartMs || startMs >= windowEndMs) return;
	blocks.push({ start_ms: startMs, end_ms: endMs, state });
}

/** A band standing in for a state that was set before the fetch window. */
export interface LeadingRegion {
	startMs: number;
	endMs: number;
	state: unknown;
}

/**
 * One field's marks over `[windowStartMs, windowEndMs]`.
 *
 * `entries` are that field's own, sorted by occur_time ascending, and may
 * reach back before the window — an entry that started earlier still
 * draws the part of its band that lands inside it.
 */
export function fieldMarks(
	field: string,
	entries: Entry[],
	windowStartMs: number,
	windowEndMs: number,
	fallbackEndMs: number,
	lead?: LeadingRegion
): FieldReq {
	const blocks: BlockReq[] = [];
	const points: PointReq[] = [];
	const ends = switchRegionEnds(entries, fallbackEndMs);

	// da-ncoa: a state set before the fetch window has no in-window entry
	// to open its region, so nowview hands one in. First, so it lies under
	// whatever the in-window entries draw over it.
	if (lead) pushRegion(blocks, lead.state, lead.startMs, lead.endMs, windowStartMs, windowEndMs);

	entries.forEach((entry, i) => {
		const at = occurMs(entry);
		if (!Number.isFinite(at)) return;

		switch (renderShape(entry)) {
			case 'hidden':
			case 'marker':
				break;
			case 'region':
				if (isBlockVerb(entry.verb)) {
					// A block is `[occur - dur, occur]`: occur_time is its
					// trailing edge, not its start.
					const secs = entry.dur === undefined ? null : durationSeconds(entry.dur);
					if (secs === null || secs <= 0) break;
					if (at <= windowStartMs || at - secs * 1000 >= windowEndMs) break;
					blocks.push({ start_ms: at - secs * 1000, end_ms: at, state: entry.state });
				} else {
					pushRegion(blocks, entry.state, at, ends[i], windowStartMs, windowEndMs);
				}
				break;
			case 'dot':
				if (at < windowStartMs || at > windowEndMs) break;
				points.push(
					entry.verb === 'point-switch'
						? {
								// A point-switch carries both halves itself: the
								// state it passes through, over the state it leaves
								// behind.
								time_ms: at,
								fg: entry.state,
								fg_fallback: 'field',
								bg: entry.followingState,
								bg_fallback: 'field',
								state_label: entry.state
							}
						: {
								// A plain dot is the field's colour over whatever
								// state was prevailing underneath it — black when
								// none was.
								time_ms: at,
								fg_fallback: 'field',
								bg: stateAt(entries, at),
								bg_fallback: 'black',
								state_label: entry.state
							}
				);
				break;
		}

		// The trailing region a compound switching verb opens: a band from
		// occur_time coloured by `following_state`.
		if (opensSecondarySwitchRegion(entry.verb)) {
			pushRegion(blocks, entry.followingState, at, ends[i], windowStartMs, windowEndMs);
		}
	});

	return { field, blocks, points };
}
