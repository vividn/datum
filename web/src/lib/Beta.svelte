<!--
	The beta mark, the ⓘ beside it and the note it opens (da-0a5o) — in the
	ROOT BAR, just before the gear, since da-y992.

	vividn, on PR #25: "Let's move the beta marker up to the top bar near the
	gear logo, make the db name fill the space". The tag and the ⓘ move as
	one, because the ⓘ is what explains the tag. The logo they leave behind
	is the database's name, with the brand block to itself (Brand.svelte).

	The note hangs DOWN from the top bar now, right-aligned to the BAR's
	edge rather than to the pair: the gear stands between the pair and the
	edge, and a note as wide as a phone allows, hung from the pair, ran off
	the screen's left side by that much. So the pair is not a containing
	block, and the note is placed against the root bar, which is.
-->
<script lang="ts">
	import { storageSentence } from '$lib/storageState.js';

	let open = false;

	function onKey(event: KeyboardEvent): void {
		if (event.key !== 'Escape' || !open) return;
		open = false;
		event.preventDefault();
	}
</script>

<svelte:window on:keydown={onKey} />

<span class="beta">
	<span class="tag">beta</span>
	<button
		type="button"
		class="hint-button"
		aria-label="about datum.day"
		aria-expanded={open}
		title="about datum.day"
		on:click={() => (open = !open)}>ⓘ</button
	>
	{#if open}
		<aside class="beta-hint" role="dialog" aria-label="about datum.day">
			<p>datum.day is early software. {$storageSentence}</p>
		</aside>
	{/if}
</span>

<style>
	.beta {
		display: flex;
		align-items: center;
		gap: 0.1rem;
		flex: none;
	}

	/* The beta mark, the ⓘ beside it and the note it opens are an amber of
	   their own — #f0b429 and the four shades under it — and not in the
	   palette: they say "this software is early", which is a thing the app
	   says about ITSELF, said once, in a corner. None of them is `--warn`,
	   which means unsaved work. Nor do they take the database's hue
	   (da-y992): what they say is not about the database. */
	.tag {
		text-transform: uppercase;
		letter-spacing: 0.08em;
		font-size: 0.75rem;
		color: var(--page);
		background: #f0b429;
		border-radius: 3px;
		padding: 0 0.3rem;
	}

	/* A fingertip's 24px (conventions #12, reachable.test.mjs): the glyph
	   is small, the box it sits in is not. */
	.hint-button {
		display: grid;
		place-items: center;
		inline-size: 24px;
		block-size: 24px;
		background: none;
		border: none;
		padding: 0;
		font: inherit;
		font-size: 0.95rem;
		line-height: 1;
		color: #c0a060;
		cursor: pointer;
	}

	.hint-button:hover,
	.hint-button:focus-visible,
	.hint-button[aria-expanded='true'] {
		color: #f0b429;
	}

	/* Below the bar, over the page: the root bar is the top edge, and the
	   box this is placed in (see the note above). */
	.beta-hint {
		position: absolute;
		top: calc(100% + 0.4rem);
		right: 0.5rem;
		z-index: 20;
		width: max-content;
		max-width: min(28rem, calc(100vw - 2rem));
		padding: 0.5rem 0.75rem;
		background: #2a2010;
		border: 1px solid #5a4418;
		border-radius: 6px;
		box-shadow: 0 4px 16px #00000088;
		font-size: 0.8rem;
		line-height: 1.35;
	}

	.beta-hint p {
		margin: 0;
		color: #e8d5a8;
	}
</style>
