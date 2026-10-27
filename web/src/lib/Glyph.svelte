<!--
	The symbol control (da-ogyxa): what an occur dot draws in place of the
	circle (da-gzowt), picked the way a colour is — a tile showing it, and a
	tap opens the set. vividn, 2026-09-23: «let's get a nice glyph picker for
	the web interface»; 2026-10-19 (da-rpfws): it is called a «symbol», and
	it «should pop up rather than push down», so the set is a sheet.

	The set is a curated grid in groups (`GLYPHS` in $lib/track.ts), and a box that takes any one
	character — one, because the CLI takes one (`check_glyph` in its
	spec.rs). `none` stores nothing and draws the ordinary circle.

	Where `icon` is bound (the field's own, not a state's), the set offers
	«image (svg)» too (da-45wgz, da-rpfws — vividn: «what even is icon? …
	put it on the symbol picker»): the file becomes the spec's `icon`, which
	the dayview draws, and the glyph is cleared, so the CLI's band draws the
	dot. A character picked clears the image. The tile shows the image in
	an `<img>`, where nothing in it runs: what the file may hold is the
	CLI's to say on Done.
-->
<script lang="ts">
	import { act } from './act.js';
	import Sheet from './Sheet.svelte';
	import { GLYPHS, wide } from './track.js';

	interface Props {
		/** The glyph, one character; empty for the circle. */
		value?: string;
		/** The `<svg>` drawn instead, or null where an image is not offered. */
		icon?: string | null;
		/** What it is the symbol of, for a reader that cannot see it. */
		label: string;
		/** The colour the dot is drawn in, so the tile looks like the dot. */
		color?: string;
	}

	let {
		value = $bindable(''),
		icon = $bindable(null),
		label,
		color = ''
	}: Props = $props();

	let open = $state(false);

	function pick(next: string) {
		value = next;
		if (icon !== null) icon = '';
		open = false;
	}

	/** The first character typed, whatever else came with it. */
	function typed(e: Event) {
		const first = [...(e.currentTarget as HTMLInputElement).value.trim()][0];
		if (first) pick(first);
	}

	async function image(e: Event) {
		const file = (e.currentTarget as HTMLInputElement).files?.[0];
		if (!file) return;
		icon = await file.text();
		value = '';
		open = false;
	}

	const src = (svg: string) => `data:image/svg+xml;charset=utf-8,${encodeURIComponent(svg)}`;
	const two = (glyph: string) => (wide(glyph) ? `${glyph} — two columns wide: the band draws it ●` : null);
</script>

<button
	type="button"
	class="tile"
	class:none={!value && !icon}
	data-glyph={value}
	data-icon={icon ? '' : null}
	style:color={color || null}
	aria-label="{label}: {icon ? 'image' : value || 'none'}"
	aria-expanded={open}
	aria-haspopup="dialog"
	title={two(value)}
	use:act={'open'}
	onclick={() => (open = true)}
	>{#if icon}<img src={src(icon)} alt="" />{:else}{value || '●'}{/if}</button
>
<Sheet title={label} bind:open>
	<span class="picker" role="listbox" aria-label="{label}, choose">
		<button
			type="button"
			class="tile none"
			role="option"
			aria-selected={!value && !icon}
			aria-label="none"
			data-glyph=""
			onclick={() => pick('')}>●</button
		>
		{#each GLYPHS as { group, glyphs } (group)}
			<span class="group">{group}</span>
			{#each glyphs as glyph (glyph)}
				<button
					type="button"
					class="tile"
					role="option"
					aria-selected={!icon && value === glyph}
					aria-label={glyph}
					data-glyph={glyph}
					title={two(glyph)}
					onclick={() => pick(glyph)}>{glyph}</button
				>
			{/each}
		{/each}
		<input class="any" placeholder="any character" aria-label="{label}, any character" autocomplete="off" oninput={typed} />
		{#if icon !== null}
			<!-- The input covers the box, so a drop lands on it. -->
			<label class="image" class:on={!!icon}>
				<input type="file" accept=".svg,image/svg+xml" aria-label="{label}, image (svg)" onchange={image} />
				image (svg)
			</label>
		{/if}
	</span>
</Sheet>

<style>
	.tile {
		box-sizing: border-box;
		inline-size: 28px;
		block-size: 28px;
		padding: 0;
		background: var(--raised);
		border: 1px solid var(--rule);
		border-radius: 4px;
		color: var(--ink-bright);
		font: inherit;
		font-size: 1rem;
		line-height: 1;
		cursor: pointer;
	}

	/* The `›` app.css draws on an opener, beside the box rather than in it. */
	.tile:global([data-act='open']) {
		position: relative;
		margin-inline-end: 0.6em;
	}

	.tile:global([data-act='open'])::after {
		position: absolute;
		inset-block-start: 50%;
		inset-inline-start: 100%;
		margin-inline-start: 0.15em;
		transform: translateY(-50%);
		color: var(--muted);
		font-size: 1rem;
		line-height: 1;
	}

	.tile img {
		display: block;
		inline-size: 100%;
		block-size: 100%;
		object-fit: contain;
	}

	/* No glyph stored: the circle it will draw, quieter than a chosen one. */
	.tile.none {
		color: var(--muted);
	}

	.tile:focus-visible,
	.tile[aria-selected='true'],
	.image.on {
		outline: 2px solid var(--accent);
		outline-offset: 1px;
	}

	.picker {
		display: flex;
		flex-wrap: wrap;
		align-items: center;
		gap: 0.25rem;
	}

	.group {
		flex-basis: 100%;
		color: var(--muted);
		font-size: 0.7rem;
		letter-spacing: 0.06em;
	}

	.any,
	.image {
		box-sizing: border-box;
		flex-basis: 100%;
		min-block-size: 24px;
		border: 1px solid var(--rule);
		border-radius: 4px;
		padding: 0.25rem 0.4rem;
		font: inherit;
	}

	.any {
		background: var(--raised);
		color: var(--ink-bright);
	}

	.image {
		position: relative;
		border-style: dashed;
		color: var(--muted);
		cursor: pointer;
	}

	.image input {
		position: absolute;
		inset: 0;
		opacity: 0;
		cursor: pointer;
	}
</style>
