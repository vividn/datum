<!--
	A tour (#lib/tour.ts): the stop in front ringed — a pulse, or a still ring
	under prefers-reduced-motion — and its sentence beside it. On a stop that
	only shows, a tap anywhere is the next stop, as «next» is, and the page
	under it takes no tap meanwhile, so a tap meant as «next» never logs the
	field the ring is round (da-ii8lo). On a stop that INVITES (da-7dflk) the
	ring is a hole the page is used through, and doing what it says moves on;
	«skip» moves on without it. «leave» and Escape end it; «read more», on a
	widget's last stop, opens its guide in the docs.
-->
<script lang="ts">
	import { onDestroy, untrack } from 'svelte';
	import { invited, shown, tour } from '#lib/tour.ts';
	import { openGuide } from '#lib/help.ts';

	/** Where the stop's element is drawn, read every frame: a pane may still be settling. */
	let box = $state<DOMRect | null>(null);
	let frame = 0;
	let height = $state(0);
	let width = $state(0);
	let next: HTMLButtonElement | undefined = $state();

	const stop = $derived($tour[0]);
	$effect.pre(() => {
		const on = stop;
		untrack(() => follow(on));
	});
	// A stop that invites leaves the focus where the page has it: a box to type in, say.
	$effect.pre(() => {
		if (stop && !stop.do && next) next.focus();
	});
	$effect(() => {
		const on = stop;
		if (on?.do) return invited(on, () => $tour[0] === on && onward());
	});
	/** The sentence goes under the ring where it fits, else over it, else inside its foot. */
	const place = $derived(!box ? '' : box.bottom + 150 < height ? `top: ${box.bottom + 10}px` : box.top > 150 ? `bottom: ${height - box.top + 10}px` : `bottom: ${height - box.bottom + 10}px`);
	/** The page dimmed and taking no tap, but through the ring of a stop that invites. */
	const hole = $derived(
		!box || !stop?.do
			? undefined
			: `path(evenodd, "M0 0H${width}V${height}H0Z M${box.left - 4} ${box.top - 4}H${box.right + 4}V${box.bottom + 4}H${box.left - 4}Z")`
	);

	/** Follow the stop's element, scrolled into view once it is found: a widget's part may be past its pane's fold. */
	function follow(on: typeof stop): void {
		cancelAnimationFrame(frame);
		box = null;
		let found = false;
		const step = () => {
			const el = on && shown(on.at, on.in);
			if (el && !found) el.scrollIntoView({ block: 'nearest', inline: 'nearest' });
			found ||= !!el;
			if (el) box = el.getBoundingClientRect();
			if (on) frame = requestAnimationFrame(step);
		};
		step();
	}
	onDestroy(() => cancelAnimationFrame(frame));

	const onward = () => tour.update((t) => t.slice(1));
	const end = () => tour.set([]);
	function more(guide: string): void {
		end();
		void openGuide(guide);
	}

	/** Shown as a popover: the top layer, above a modal open under it, without making the page inert. */
	function raised(node: HTMLElement): void {
		node.showPopover();
	}

	function key(e: KeyboardEvent): void {
		if (!stop || e.key !== 'Escape') return;
		e.preventDefault();
		e.stopPropagation();
		end();
	}
</script>

<svelte:window bind:innerHeight={height} bind:innerWidth={width} onkeydowncapture={key} />

{#if stop && box}
	<!-- In the top layer, over a modal place too, and modal itself to nothing: a stop that invites is used through it. -->
	<div class="tour" data-do={stop.do} popover="manual" use:raised>
		<!-- svelte-ignore a11y_click_events_have_key_events, a11y_no_static_element_interactions -->
		<div class="block" style:clip-path={hole} onclick={stop.do ? undefined : onward}></div>
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
				<button type="button" class="leave" onclick={end}>leave</button>
				{#if stop.more}
					{@const guide = stop.more}
					<button type="button" class="more" onclick={() => more(guide)}>read more</button>
				{/if}
				<button type="button" class="next" bind:this={next} onclick={onward}
					>{stop.do ? 'skip' : $tour.length === 1 ? 'done' : 'next'}</button
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
		inline-size: auto;
		block-size: auto;
		max-inline-size: none;
		max-block-size: none;
		margin: 0;
		padding: 0;
		border: 0;
		background: transparent;
		overflow: visible;
		pointer-events: none;
	}

	.block {
		position: fixed;
		inset: 0;
		pointer-events: auto;
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
		pointer-events: auto;
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

	.acts .next,
	.acts .more {
		border-color: var(--accent);
		color: var(--ink-bright);
	}
</style>
