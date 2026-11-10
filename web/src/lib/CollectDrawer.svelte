<!--
	A field's extra data, a drawer on collect's far edge, opposite the now
	strip (da-igqs2). vividn, 2026-11-05: «a separate little section off on
	the far side, that you can open and add keys, see which keys are defined
	etc. normally it is closed, but if data is required, it swings open or you
	can drag it open your self before logging», the far side being the right
	edge, opposite the now strip.

	Closed, its handle stands on the edge; a tap on it or a drag out of the
	edge opens it, a drag back or a tap closes it. Open, it lists the field's
	keys, a tap putting `key=` on the line, and `+ data` taking the caret
	there. It swings open by itself when the field has a required key: the
	widget mounts one per field, so a drawer opened or closed by hand stays so
	until the field changes.
-->
<script lang="ts">
	import { CLICK_SLOP } from './inspect.js';
	import type { Key } from './fields.js';

	let {
		keys,
		disabled = false,
		/** Which edge it stands at: the one the now strip does not. */
		edge = 'right',
		onkey,
		onmore
	}: { keys: Key[]; disabled?: boolean; edge?: 'left' | 'right'; onkey: (name: string) => void; onmore: () => void } = $props();

	const needed = $derived(keys.some((k) => k.required));
	let open = $state(false);
	// After the first frame, so it is seen to swing open.
	$effect(() => {
		const want = needed;
		const frame = requestAnimationFrame(() => (open = want));
		return () => cancelAnimationFrame(frame);
	});

	/** The panel, whose width a drag reads as it starts: bound, it was read as the drawer mounted, in a tap's flush (da-9f6ob). */
	let panel: HTMLDivElement | undefined;
	/** How far of the panel a drag has it out, 0 closed to 1 open; null while not dragged. */
	let out = $state<number | null>(null);
	/** A drag's own click is no tap. */
	let dragged = false;

	/**
	 * A press on the drawer, followed on the window until it lifts wherever it
	 * lifts (CollectWhen's `closeDown`): past the slop sideways it is a drag,
	 * the drawer under the finger, and it rests open past halfway.
	 */
	function press(down: PointerEvent) {
		const width = panel?.clientWidth ?? 0;
		if (disabled || !width) return;
		const from = open ? 1 : 0;
		const toward = edge === 'right' ? -1 : 1;
		dragged = false;
		const move = (e: PointerEvent) => {
			if (e.pointerId !== down.pointerId) return;
			const dx = (e.clientX - down.clientX) * toward;
			if (!dragged && Math.abs(dx) < CLICK_SLOP) return;
			dragged = true;
			out = Math.min(1, Math.max(0, from + dx / width));
		};
		const end = () => {
			if (out !== null) open = out > 0.5;
			out = null;
			removeEventListener('pointermove', move);
			removeEventListener('pointerup', end);
			removeEventListener('pointercancel', end);
		};
		addEventListener('pointermove', move);
		addEventListener('pointerup', end);
		addEventListener('pointercancel', end);
	}

	function swallow(e: MouseEvent) {
		if (!dragged) return;
		dragged = false;
		e.stopPropagation();
		e.preventDefault();
	}

	const shown = $derived(out ?? (open ? 1 : 0));
</script>

<!-- svelte-ignore a11y_no_static_element_interactions -->
<div
	class="drawer"
	class:left={edge === 'left'}
	class:open
	class:moving={out !== null}
	style:transform="translateX({(1 - shown) * 100 * (edge === 'right' ? 1 : -1)}%)"
	data-drawer={open ? 'open' : 'closed'}
	data-pans="x"
	onpointerdown={press}
	onclickcapture={swallow}
>
	<button class="handle" data-drawer-handle {disabled} aria-expanded={open} title={open ? 'close the data' : 'the data: its keys'} onclick={() => (open = !open)}>
		data
	</button>
	<div class="panel" bind:this={panel} inert={!open && out === null}>
		{#each keys as key (key.name)}
			<button class="key" class:required={key.required} data-key={key.name} {disabled} onclick={() => onkey(key.name)}>{key.name}=</button>
		{/each}
		<button class="more" data-more-data {disabled} onclick={onmore}>+ data</button>
	</div>
</div>

<style>
	.drawer {
		position: absolute;
		top: 0;
		bottom: 0;
		right: 0;
		z-index: 2;
		display: flex;
		width: min(12rem, 60%);
		/* A sideways drag is the drawer's; the keys still scroll up and down. */
		touch-action: pan-y;
		transition: transform 200ms cubic-bezier(0.33, 1, 0.68, 1);
	}

	.drawer.left {
		right: auto;
		left: 0;
		flex-direction: row-reverse;
	}

	.drawer.moving {
		transition: none;
	}

	@media (prefers-reduced-motion: reduce) {
		.drawer {
			transition: none;
		}
	}

	.panel {
		flex: 1;
		min-width: 0;
		display: flex;
		flex-direction: column;
		align-items: stretch;
		gap: 0.4rem;
		padding: 0.5rem;
		overflow: hidden auto;
		background: var(--ground);
		border-left: 1px solid var(--rule);
		box-shadow: -2px 0 6px rgb(0 0 0 / 0.15);
	}

	.left .panel {
		border-left: none;
		border-right: 1px solid var(--rule);
		box-shadow: 2px 0 6px rgb(0 0 0 / 0.15);
	}

	/* Hung off the panel's inner side, so it is on the edge while the panel is out of view. */
	.handle {
		position: absolute;
		top: 50%;
		right: 100%;
		translate: 0 -50%;
		writing-mode: vertical-rl;
		padding: 0.6rem 0.2rem;
		background: var(--ground);
		color: var(--ink);
		border: 1px solid var(--rule);
		border-right: none;
		border-radius: 6px 0 0 6px;
		font: inherit;
		font-size: 0.75rem;
		cursor: pointer;
	}

	.left .handle {
		right: auto;
		left: 100%;
		border-right: 1px solid var(--rule);
		border-left: none;
		border-radius: 0 6px 6px 0;
	}

	.key,
	.more {
		background: none;
		border: 1px dashed var(--rule);
		border-radius: 999px;
		padding: 0.25rem 0.6rem;
		color: var(--ink);
		font: inherit;
		font-size: 0.8rem;
		cursor: pointer;
		overflow-wrap: anywhere;
	}

	.key.required {
		border-style: solid;
	}

	.more {
		border-style: solid;
	}
</style>
