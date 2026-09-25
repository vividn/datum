<!--
	The spec modal's Placement tab (da-btyvj): where a field's lane sits in
	the day, chosen by dragging it on today's REAL band — the CLI's own
	`dayview`, so the field is placed against its actual neighbours.

	The model was already there (`PresentationSpec`: `y`/`height` as
	fractions of the day, or the anchor triple); this is its face. Without
	one, a field's `y` is a hash of its NAME — a lane nobody chose — and the
	readout says so until someone does.

	Drag the lane to move it, an edge to resize it. Tied to a neighbour
	(⛓), the same drag writes the RATIOS to that neighbour and not `y` /
	`height`, so the lane moves when its anchor does. Nothing is written
	here: `place` is bound to the modal, which saves it with the rest.
-->
<script lang="ts">
	import { onMount } from 'svelte';
	import { inlineSvg } from './hover.js';
	import { bandArgv } from './panes.js';
	import { run } from './run.js';
	import type { Place } from './track.js';

	export let field: string;
	export let place: Place;

	/** The day's height in pixels: tall enough to take a lane's edge with a finger. */
	const H = 160;
	/** The thinnest lane a drag leaves. */
	const MIN = 0.02;

	interface Lane {
		y: number;
		height: number;
		spec: { relative_to?: string | null } | null;
	}

	let band: HTMLDivElement;
	let svg: SVGSVGElement | null = null;
	/** Every field's lane as the pictures resolve it (`fields --json`), anchors included. */
	let lanes: Record<string, Lane> = {};
	/** The fields today's band draws, whose names label their lanes. */
	let named: string[] = [];
	let drag: { mode: 'move' | 'top' | 'bottom'; at: number; y: number; h: number } | null = null;

	onMount(async () => {
		const [picture, listing] = await Promise.all([
			run(bandArgv(Math.floor(band.clientWidth), H)),
			run(['fields', '--json'])
		]);
		if (listing.code === 0) {
			lanes = Object.fromEntries(
				(JSON.parse(listing.stdout) as ({ field: string } & Lane)[]).map((f) => [f.field, f])
			);
		}
		svg = picture.code === 0 ? inlineSvg(picture.stdout) : null;
		if (!svg) return;
		band.prepend(svg);
		named = [...svg.querySelectorAll('g.field')].map((g) => g.getAttribute('data-field') ?? '');
	});

	const round = (n: number) => Math.round(n * 1000) / 1000;
	const clamp = (n: number, lo: number, hi: number) => Math.min(hi, Math.max(lo, n));

	/** The lane as the band drew it, from the stored spec. */
	$: drawn = lanes[field];
	// The resolver holds a field only by an anchor with a spec; without one it draws it free.
	$: anchor = place.relative_to && lanes[place.relative_to]?.spec ? lanes[place.relative_to] : undefined;
	/** The lane as it stands now, in fractions of the day. */
	$: lane = anchor
		? {
				y: anchor.y + anchor.height * (place.anchor_pos_ratio ?? 0),
				h: anchor.height * (place.anchor_size_ratio ?? 1)
			}
		: { y: place.y ?? drawn?.y ?? 0, h: place.height ?? drawn?.height ?? 0.12 };

	/** Would tying to `name` close a loop back to this field? */
	function loops(name: string, seen = new Set<string>()): boolean {
		if (name === field) return true;
		if (seen.has(name)) return false;
		seen.add(name);
		const next = lanes[name]?.spec?.relative_to;
		return !!next && loops(next, seen);
	}
	// Only a field with a spec can anchor: the resolver has nothing else to hold it by.
	$: anchors = Object.keys(lanes).filter((n) => lanes[n].spec && lanes[n].height > 0 && !loops(n));

	/**
	 * Put the lane at `y`/`h`: as ratios to `to` when tied, as absolutes when
	 * free — and free when `to` is gone or has no height to be a ratio of.
	 */
	function put(y: number, h: number, to = place.relative_to) {
		const a = to ? lanes[to] : undefined;
		place =
			to && a?.spec && a.height > 0
				? {
						relative_to: to,
						anchor_pos_ratio: round((y - a.y) / a.height),
						anchor_size_ratio: round(h / a.height)
					}
				: { y: round(y), height: round(h) };
	}

	function grab(mode: 'move' | 'top' | 'bottom', e: PointerEvent) {
		e.preventDefault();
		e.stopPropagation();
		drag = { mode, at: e.clientY, y: lane.y, h: lane.h };
	}

	function slide(e: PointerEvent) {
		if (!drag) return;
		const d = (e.clientY - drag.at) / H;
		const { y, h } = drag;
		if (drag.mode === 'move') put(clamp(y + d, 0, 1 - h), h);
		else if (drag.mode === 'bottom') put(y, clamp(h + d, MIN, 1 - y));
		else {
			const top = clamp(y + d, 0, y + h - MIN);
			put(top, y + h - top);
		}
	}

	// The field's own marks follow the lane while it moves: the band was drawn
	// at the stored place, so its group is carried from there to here.
	$: own = svg?.querySelector(`g.field[data-field="${CSS.escape(field)}"]`);
	$: if (own && drawn && drawn.height > 0) {
		const k = lane.h / drawn.height;
		own.setAttribute('transform', `matrix(1 0 0 ${k} 0 ${(lane.y - drawn.y * k) * H})`);
	}

	$: relation = !anchor
		? ''
		: (place.anchor_pos_ratio ?? 0) >= 1
			? 'below'
			: (place.anchor_pos_ratio ?? 0) + (place.anchor_size_ratio ?? 1) <= 0
				? 'above'
				: 'on';
</script>

<svelte:window on:pointermove={slide} on:pointerup={() => (drag = null)} on:pointercancel={() => (drag = null)} />

<div class="placement">
	<label class="tie">
		<select
			aria-label="anchor to a field"
			value={place.relative_to ?? ''}
			on:change={(e) => put(lane.y, lane.h, e.currentTarget.value)}
		>
			<option value="">⛓ free</option>
			{#each anchors as name (name)}
				<option value={name}>⛓ {name}</option>
			{/each}
			<!-- A saved anchor the list cannot offer is still what the spec says. -->
			{#if place.relative_to && !anchors.includes(place.relative_to)}
				<option value={place.relative_to}>⛓ {place.relative_to} (missing)</option>
			{/if}
		</select>
	</label>

	<div class="band" bind:this={band} style:height="{H}px">
		{#each named.filter((n) => n !== field && lanes[n]) as name (name)}
			<span class="name" style:top="{lanes[name].y * H}px">{name}</span>
		{/each}
		<!-- svelte-ignore a11y-no-static-element-interactions -->
		<div
			class="lane"
			class:dragging={drag}
			data-field={field}
			style:top="{lane.y * H}px"
			style:height="{lane.h * H}px"
			on:pointerdown={(e) => grab('move', e)}
		>
			<span class="edge top" on:pointerdown={(e) => grab('top', e)}></span>
			<span class="own">{field}</span>
			<span class="edge bottom" on:pointerdown={(e) => grab('bottom', e)}></span>
		</div>
	</div>

	<p class="readout">
		{#if anchor}
			{relation} {place.relative_to} · ×{place.anchor_size_ratio}
		{:else if place.relative_to}
			tied to {place.relative_to}, which has no spec — drawn free at y {lane.y.toFixed(2)}
		{:else}
			y {lane.y.toFixed(2)} &nbsp; h {lane.h.toFixed(2)}
			{#if place.y === undefined}<span class="unchosen">· placed by its name's hash — nobody chose this</span>{/if}
		{/if}
	</p>
</div>

<style>
	.placement {
		display: flex;
		flex-direction: column;
		gap: 0.4rem;
	}

	.tie {
		align-self: flex-end;
	}

	select {
		min-block-size: 28px;
		background: #1a1a30;
		border: 1px solid var(--rule);
		border-radius: 4px;
		color: var(--ink-bright);
		font: inherit;
	}

	.band {
		position: relative;
		overflow: hidden;
		border: 1px solid var(--rule);
	}

	.band :global(svg) {
		display: block;
	}

	.name,
	.own {
		position: absolute;
		left: 0.25rem;
		font-size: 0.65rem;
		line-height: 1;
		color: var(--muted);
		pointer-events: none;
	}

	/* The lane being placed: its whole box moves it, its edges size it.
	   `touch-action: none` or a finger scrolls the dialog instead. */
	.lane {
		position: absolute;
		inset-inline: 0;
		box-sizing: border-box;
		min-block-size: 6px;
		border: 1px solid var(--accent);
		background: color-mix(in srgb, var(--accent) 12%, transparent);
		cursor: grab;
		touch-action: none;
	}

	.lane.dragging {
		cursor: grabbing;
	}

	.own {
		top: 50%;
		left: auto;
		right: 0.25rem;
		transform: translateY(-50%);
		color: var(--accent);
	}

	.edge {
		position: absolute;
		inset-inline: 0;
		block-size: 8px;
		cursor: ns-resize;
		touch-action: none;
	}

	.edge.top {
		top: -4px;
	}

	.edge.bottom {
		bottom: -4px;
	}

	.readout {
		margin: 0;
		color: var(--ink);
		font-size: 0.75rem;
	}

	.unchosen {
		color: var(--quiet);
		font-style: italic;
	}
</style>
