<!--
	The glyph control (da-ogyxa): what an occur dot draws in place of the
	circle (da-gzowt), picked the way a colour is — a tile showing it, and a
	tap opens the set under it. vividn, 2026-09-23: «let's get a nice glyph
	picker for the web interface».

	The set is a curated grid in groups (`GLYPHS` in $lib/track.ts), and a box that takes any one
	character — one, because the CLI takes one (`check_glyph` in its
	spec.rs). `none` stores nothing and draws the ordinary circle.

	A glyph two columns wide (most emoji) draws as itself in the dayview and
	as `●` in `now`'s band, where every cell is one slice of time. It is
	marked where it is picked, and on the tile once it is.

	The wrapper is `display: contents`, so the set is an item of whatever
	row holds the tile: it spans a grid row or wraps a flex one to its own
	line, rather than squeezing whatever stands beside the tile.
-->
<script lang="ts">
	import { GLYPHS, wide } from './track.js';

	/** The glyph, one character; empty for the circle. */
	export let value = '';
	/** What it is the glyph of, for a reader that cannot see it. */
	export let label: string;
	/** The colour the dot is drawn in, so the tile looks like the dot. */
	export let color = '';

	let open = false;

	function pick(next: string) {
		value = next;
		open = false;
	}

	/** The first character typed, whatever else came with it. */
	function typed(e: Event) {
		const first = [...(e.currentTarget as HTMLInputElement).value.trim()][0];
		if (first) pick(first);
	}
</script>

<span class="glyph-control">
	<button
		type="button"
		class="tile"
		class:none={!value}
		data-glyph={value}
		data-wide={wide(value) || null}
		style:color={color || null}
		aria-label="{label}: {value || 'none'}"
		aria-expanded={open}
		title={wide(value) ? `${value} — two columns wide: the band draws it ●` : null}
		on:click={() => (open = !open)}>{value || '●'}</button
	>
	{#if open}
		<span class="picker" role="listbox" aria-label="{label}, choose">
			<button
				type="button"
				class="tile none"
				role="option"
				aria-selected={!value}
				aria-label="none"
				data-glyph=""
				on:click={() => pick('')}>●</button
			>
			{#each GLYPHS as { group, glyphs } (group)}
				<span class="group">{group}</span>
				{#each glyphs as glyph (glyph)}
					<button
						type="button"
						class="tile"
						role="option"
						aria-selected={value === glyph}
						aria-label={glyph}
						data-glyph={glyph}
						data-wide={wide(glyph) || null}
						title={wide(glyph) ? `${glyph} — two columns wide: the band draws it ●` : null}
						on:click={() => pick(glyph)}>{glyph}</button
					>
				{/each}
			{/each}
			<input
				class="any"
				placeholder="any character"
				aria-label="{label}, any character"
				autocomplete="off"
				on:input={typed}
			/>
			<span class="legend"><span class="mark">●</span> two columns wide: now's band draws it as ●</span>
		</span>
	{/if}
</span>

<style>
	.glyph-control {
		display: contents;
	}

	.tile {
		position: relative;
		box-sizing: border-box;
		inline-size: 28px;
		block-size: 28px;
		padding: 0;
		background: #1a1a30;
		border: 1px solid var(--rule);
		border-radius: 4px;
		color: var(--ink-bright);
		font: inherit;
		font-size: 1rem;
		line-height: 1;
		cursor: pointer;
	}

	/* No glyph stored: the circle it will draw, quieter than a chosen one. */
	.tile.none {
		color: var(--muted);
	}

	/* Two columns wide: a small dot in the corner, the `●` the band draws. */
	.tile[data-wide]::after,
	.mark {
		color: var(--accent);
		font-size: 0.45rem;
	}

	.tile[data-wide]::after {
		content: '●';
		position: absolute;
		inset-block-start: 1px;
		inset-inline-end: 2px;
	}

	.tile:focus-visible,
	.tile[aria-selected='true'] {
		outline: 2px solid var(--accent);
		outline-offset: 1px;
	}

	/* Its own line in a flex row, the whole row in a grid one. */
	.picker {
		flex: 1 0 100%;
		grid-column: 1 / -1;
		order: 1;
		display: flex;
		flex-wrap: wrap;
		align-items: center;
		gap: 0.25rem;
	}

	.group,
	.legend {
		flex-basis: 100%;
		color: var(--muted);
		font-size: 0.7rem;
		letter-spacing: 0.06em;
	}

	.any {
		box-sizing: border-box;
		flex-basis: 100%;
		min-block-size: 24px;
		background: #1a1a30;
		border: 1px solid var(--rule);
		border-radius: 4px;
		padding: 0.25rem 0.4rem;
		color: var(--ink-bright);
		font: inherit;
	}
</style>
