<!--
	The first field's tour (da-ii8lo, $lib/tour.ts): the stop in front
	ringed — a pulse, or a still ring under prefers-reduced-motion — and its
	sentence beside it. A tap anywhere is the next stop, as «next» is;
	«skip» and Escape end it. The page under it takes no tap meanwhile, so a
	tap meant as «next» never logs the field the ring is round.
-->
<script lang="ts">
	import { onDestroy } from 'svelte';
	import { shown, tour } from '$lib/tour.js';

	/** Where the stop's element is drawn, read every frame: a pane may still be settling. */
	let box: DOMRect | null = null;
	let frame = 0;
	let height = 0;
	let next: HTMLButtonElement | undefined;

	$: stop = $tour[0];
	$: follow(stop);
	$: if (stop && next) next.focus();
	/** The sentence goes under the ring where it fits, else over it, else inside its foot. */
	$: place = !box ? '' : box.bottom + 150 < height ? `top: ${box.bottom + 10}px` : box.top > 150 ? `bottom: ${height - box.top + 10}px` : `bottom: ${height - box.bottom + 10}px`;

	function follow(on: typeof stop): void {
		cancelAnimationFrame(frame);
		box = null;
		const step = () => {
			const el = on && shown(on.at);
			if (el) box = el.getBoundingClientRect();
			if (on) frame = requestAnimationFrame(step);
		};
		step();
	}
	onDestroy(() => cancelAnimationFrame(frame));

	const onward = () => tour.update((t) => t.slice(1));
	const end = () => tour.set([]);

	function key(e: KeyboardEvent): void {
		if (!stop || e.key !== 'Escape') return;
		e.preventDefault();
		e.stopPropagation();
		end();
	}
</script>

<svelte:window bind:innerHeight={height} on:keydown|capture={key} />

{#if stop && box}
	<!-- svelte-ignore a11y-click-events-have-key-events a11y-no-static-element-interactions -->
	<div class="tour" on:click={onward}>
		<div
			class="ring"
			style:left="{box.left - 4}px"
			style:top="{box.top - 4}px"
			style:width="{box.width + 8}px"
			style:height="{box.height + 8}px"
		></div>
		<div class="say" role="dialog" aria-label="tour" style={place}>
			<p aria-live="polite">{stop.say}</p>
			<div class="acts">
				<button type="button" class="skip" on:click|stopPropagation={end}>skip</button>
				<button type="button" class="next" bind:this={next} on:click|stopPropagation={onward}
					>{$tour.length === 1 ? 'done' : 'next'}</button
				>
			</div>
		</div>
	</div>
{/if}

<style>
	.tour {
		position: fixed;
		inset: 0;
		z-index: 25;
	}

	/* The page dimmed round the stop, and a ring that pulses outward from it. */
	.ring {
		position: fixed;
		box-sizing: border-box;
		border: 2px solid var(--accent);
		border-radius: 8px;
		box-shadow: 0 0 0 100vmax rgb(0 0 0 / 0.45);
		pointer-events: none;
	}

	.ring::after {
		content: '';
		position: absolute;
		inset: -2px;
		border-radius: inherit;
		box-shadow: 0 0 0 0 var(--accent);
		animation: pulse 1.4s ease-out infinite;
	}

	@keyframes pulse {
		to {
			box-shadow: 0 0 0 14px transparent;
		}
	}

	@media (prefers-reduced-motion: reduce) {
		.ring::after {
			animation: none;
			box-shadow: 0 0 0 4px color-mix(in srgb, var(--accent) 40%, transparent);
		}
	}

	.say {
		position: fixed;
		left: 16px;
		right: 16px;
		max-inline-size: 26rem;
		margin-inline: auto;
		padding: 0.6rem 0.8rem;
		background: var(--tint);
		border: 1px solid var(--accent);
		border-radius: 8px;
		color: var(--ink);
	}

	.say p {
		margin: 0 0 0.5rem;
	}

	.acts {
		display: flex;
		justify-content: flex-end;
		gap: 0.5rem;
	}

	.acts button {
		min-block-size: 32px;
		padding: 0.2rem 0.9rem;
		background: var(--raised);
		border: 1px solid var(--rule);
		border-radius: 999px;
		color: var(--ink);
		font: inherit;
		cursor: pointer;
	}

	.acts .next {
		border-color: var(--accent);
		color: var(--ink-bright);
	}
</style>
