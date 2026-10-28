<script lang="ts" module>
	/*
	 * A row's state as the three values it is made of (da-fknn8) — what the
	 * field was in before it, what it says, and what continues forward —
	 * DRAWN THE WAY THE CLI DRAWS THEM.
	 *
	 * The first version of this rendered the two edges as WORDS, reasoning
	 * that a coloured sliver works on a character grid and cannot be read in
	 * a browser. vividn ruled otherwise, 2026-09-22:
	 *
	 *   "maybe use monospace just like the CLI (I think it is already
	 *    monospace)? The previous and following states should just have the
	 *    colors, not the full text — just like the CLI. Really mirror the
	 *    CLI here. The rerendering in the web is just for some niceities and
	 *    being able to select. Hover over previous state could show the
	 *    value, but don't need to render the full text."
	 *
	 * ⚑ AND IT DISSOLVES THE PROBLEM THE WORDS CREATED rather than solving
	 * it. A word is as wide as its state is long, so a row that transitioned
	 * started its chip 70px right of a row that did not, and the column
	 * stopped scanning vertically. A COLOUR BLOCK IS ONE CHARACTER WIDE
	 * whatever it stands for, so every chip starts at the same x and the
	 * raggedness has nowhere to come from. No subgrid, no reserved width,
	 * nothing padded.
	 *
	 * So this draws the CLI's OWN painted slots — `previous_state_cell` and
	 * `following_state_cell`, one character each, already resolved by
	 * `render_slot` for every shape the terminal has: a state's colour, the
	 * grey of an untracked field, a parallel set's two-colour `▞`, the
	 * unpainted space of a field that has ended, the `∅` a first entry shows
	 * and the `!` of a switch that changed nothing (da-qfv0). None of those
	 * rules is re-derived here, so none of them can drift.
	 *
	 * What the page adds is only what a terminal cannot:
	 *
	 *   • the VALUE ON HOVER, since a colour does not name its state
	 *   • a box that can be selected, which is what the re-render is for
	 *
	 * The cell is therefore lead + body + trail, byte for byte what
	 * `format_state_group` prints — asserted over every verb and duration in
	 * `render::tail_rows::tests`.
	 */

	/** The `∅` the CLI paints where a state is not tracked. */
	const NULL_SET = '∅';

	/** The sentinel for "the field was not tracked here" — the CLI's own. */
	const ignored = (state: unknown) => state === 'IGNORED';
</script>

<script lang="ts">
	import { ansiSpans } from './ansi.js';
	import AnsiText from './AnsiText.svelte';
	import type { TailRow } from './tail.js';

	interface Props {
		/** The row, as `datum tail --json` reports it, never changed in place (immutable). */
		row: TailRow;
		/** The row's painted `state` cell, for a listing with no parts to draw. */
		painted?: string;
	}

	let { row, painted = '' }: Props = $props();

	/**
	 * The word a slot stands for, for the hover. A colour names no state on
	 * its own, and this is the whole of what vividn asked the page to add:
	 * "Hover over previous state could show the value." An untracked state
	 * has no word, so it gets the null sign the CLI would have drawn.
	 *
	 * The word and the colour under it come from the same trio
	 * (`StateTransition::of`), so they agree on every row — a compound
	 * switch's TRAIL included, which paints the state it switches TO
	 * (da-qxmqe) and names it.
	 */
	function hover(state: unknown, text: string | null | undefined): string | undefined {
		if (state === null || state === undefined) return undefined;
		return ignored(state) ? NULL_SET : (text ?? NULL_SET);
	}

	const lead = $derived(typeof row.previous_state_cell === 'string' ? row.previous_state_cell : '');
	const trail = $derived(typeof row.following_state_cell === 'string' ? row.following_state_cell : '');
	/**
	 * A listing from before these keys existed — or any row the CLI did not
	 * paint parts for — falls back to the whole painted cell, which is what
	 * was drawn before and is the same pixels.
	 */
	const body = $derived(typeof row.state_body === 'string' ? row.state_body : painted);
	/** An occur dot's declared keys, plain, after the cell as the CLI prints them (da-545c9). */
	const keys = $derived(typeof row.keys === 'string' ? row.keys : '');

	/*
	 * A NOTE ON THE MARKUP BELOW, which lives here rather than in an HTML
	 * comment because a comment quoting Svelte's own block syntax is parsed
	 * as that syntax.
	 *
	 * The three parts are SIBLINGS in the cell, with no wrapper: a wrapper
	 * would be one span around other spans, and every test that walks a
	 * cell's runs (`tests/ansitext.test.mjs`) would read the cell's text
	 * once for the wrapper and again for each run inside it. Nothing may
	 * come between them either — the tag hugging below is load-bearing for
	 * the same reason it is in AnsiText: a newline left between two elements
	 * arrives in the DOM as a space, and this is a monospace cell whose
	 * three parts must sit flush as they do on a terminal.
	 */
</script>

{#if lead}<span class="edge from" title={hover(row.previous_state, row.previous_state_label)}
		><AnsiText spans={ansiSpans(lead)} /></span
	>{/if}<AnsiText spans={ansiSpans(body)} />{#if trail}<span
		class="edge to"
		title={hover(row.following_state, row.following_state_label)}
		><AnsiText spans={ansiSpans(trail)} /></span
	>{/if}{#if keys}<span class="keys">{` ${keys}`}</span>{/if}

<style>
	/*
	 * ⛔ `white-space: pre` IS LOAD-BEARING AND NOT TIDINESS. The CLI's slot
	 * is a SPACE painted with a background colour, and a lone space in a
	 * table cell COLLAPSES — measured: the span reported `width: 0` with its
	 * background correctly set, so the colour block was there and drew
	 * nothing. The CLI widget's surface never hit this because `CliOutput` is
	 * already `white-space: pre`; a `<td>` is not. Without this line vividn's
	 * ruling ships as an invisible edge.
	 *
	 * Scoped to the one-character edge, so a long state in the body still
	 * wraps under the cell's own `overflow-wrap: anywhere`.
	 *
	 * `cursor: help` is the rest of it: the span is a HOVER TARGET, and the
	 * cursor is the only thing telling a reader that a block of colour has a
	 * name behind it.
	 */
	.edge {
		white-space: pre;
		cursor: help;
	}
</style>
