<!--
	What a field's entries will look like drawn, on the setup form's Basics
	(da-i9upr), vividn, 2026-10-19: «show what the data looks like displayed.
	So for occurence data show multiple dots interspersed, for on/off data
	show on and off states. And for states, show different states happening
	(including false if false is not disabled)».

	A made-up stretch of a day in the field's colours, symbol and states as
	the form holds them, so it follows every edit before a save. A card has
	no time to draw on, and no sample.
-->
<script lang="ts">
	import type { Verb } from './track.js';

	export let verb: Verb;
	/** The field's colour, or what stands in for one. */
	export let color: string;
	/** The symbol its dots draw, or none for the round one. */
	export let glyph = '';
	/** A switch's states, each with the colour it is drawn in. */
	export let states: { name: string; color: string }[] = [];
	/** Whether off is one of a switch's states (not `never_false`). */
	export let off = true;

	/** Where the dots fall along the stretch, as fractions of it. */
	const DOTS = [0.06, 0.19, 0.25, 0.43, 0.61, 0.68, 0.86, 0.94];
	/** How long each turn lasts, in shares of the stretch, in turn. */
	const TURNS = [3, 2, 4, 2, 3, 2, 3];

	/** One stretch of a block shape: a state in its colour, or off, drawn as nothing. */
	$: turns =
		verb === 'start'
			? [{ name: 'off', color: '' }, { name: 'on', color }]
			: [...states, ...(off || states.length === 0 ? [{ name: 'off', color: '' }] : [])];
	$: blocks = TURNS.map((grow, i) => ({ grow, ...turns[i % turns.length] }));
</script>

<div class="sample" data-sample={verb} aria-label="what its entries look like" role="img">
	{#if verb === 'occur'}
		{#each DOTS as at (at)}
			<span class="dot" style:left="{at * 100}%" style:color>{glyph || '●'}</span>
		{/each}
	{:else}
		{#each blocks as block, i (i)}
			<span class="block" class:off={!block.color} data-state={block.name} style:flex-grow={block.grow} style:--c={block.color}
				>{block.name}</span
			>
		{/each}
	{/if}
</div>

<style>
	/* Its own height in the form's scrolling panel, which would squeeze it to its border. */
	.sample {
		flex: none;
		position: relative;
		display: flex;
		block-size: 1.6rem;
		border: 1px solid var(--rule);
		border-radius: 4px;
		overflow: hidden;
	}

	.dot {
		position: absolute;
		top: 50%;
		transform: translate(-50%, -50%);
		font-size: 1rem;
		line-height: 1;
	}

	/* A block's name sits under the colour, readable whatever the colour is. */
	.block {
		flex-basis: 0;
		min-inline-size: 0;
		overflow: hidden;
		padding-inline: 0.2rem;
		background: color-mix(in srgb, var(--c) 80%, transparent);
		color: var(--ink-bright);
		font-size: 0.65rem;
		line-height: 1.6rem;
		text-shadow: 0 0 2px var(--ground), 0 0 2px var(--ground);
		white-space: nowrap;
		text-overflow: ellipsis;
	}

	.block.off {
		background: none;
		color: var(--muted);
		text-shadow: none;
	}
</style>
