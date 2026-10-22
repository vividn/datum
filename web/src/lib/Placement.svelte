<!--
	The field form's placement, on its Basics (da-btyvj, da-i9upr): where a field's lane sits in
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

	An occur field's lane is its dot (da-c1b6m): the box is drawn round the
	dot instead, centred on the lane. Its body moves the lane like any
	other; an edge or corner sizes the dot about its centre, writing
	`glyph_scale` (the dot's height over the standard dot's, so a field tied
	to this one stays put) and `glyph_aspect`. «lock aspect» keeps width over
	height, «reset» puts back a round dot of the standard size, or the lane's
	where the lane is thinner (da-rjf34) — which is how tall every view draws
	it, as a fraction of the day. A day with none of the
	field's dots has nothing to size, so the band is then the day of its
	latest one.
-->
<script lang="ts">
	import { onMount } from 'svelte';
	import { inlineSvg } from './hover.js';
	import { bandArgv, fieldsArgv, lastDotsArgv } from './calls.js';
	import { run } from './run.js';
	import type { FieldListing } from './fields.js';
	import type { TailListing } from './tail.js';
	import type { Place } from './track.js';

	export let field: string;
	export let place: Place;
	/** Whether the field is an occur field, the one whose dots are sized here. */
	export let occur = false;

	/** The day's height in pixels: tall enough to take a lane's edge with a finger. */
	const H = 160;
	/** The thinnest lane a drag leaves. */
	const MIN = 0.02;
	/** A dot's standard height, the CLI's `STANDARD_DOT`: a lane with no height. */
	const STANDARD_DOT = 0.12;
	/** What a scale of 1 draws in a lane `h` tall: the standard, or the lane where thinner. */
	const unit = (h: number) => Math.min(h, STANDARD_DOT);

	let band: HTMLDivElement;
	let svg: SVGSVGElement | null = null;
	/** Every field's lane as the pictures resolve it (`fields --json`), anchors included. */
	let lanes: Record<string, FieldListing> = {};
	/** The fields today's band draws, whose names label their lanes. */
	let named: string[] = [];
	let drag: {
		mode: 'move' | 'top' | 'bottom' | 'dot';
		at: number;
		y: number;
		h: number;
		/** For `dot`: the handle's sides (-1, 0, 1 on x and y), the press's x, the box's size in px. */
		sx?: number;
		sy?: number;
		ax?: number;
		b?: number;
		w?: number;
	} | null = null;
	/** Whether sizing the dot keeps its width over its height. */
	let locked = false;
	/** Where the dot the box is drawn round sits along the band, in px: its latest on the band. */
	let dotX = 0;
	/** The day the band draws when it is not today's. */
	let sample = '';

	/** The size the band draws the dots at: the saved one, which `place` starts as. */
	const saved = { scale: place.glyph_scale ?? 1, aspect: place.glyph_aspect ?? 1 };

	/** The day of the field's latest dot, `YYYY-MM-DD` as its own zone reads it. */
	async function lastDot(): Promise<string | undefined> {
		const r = await run(lastDotsArgv(field));
		if (r.code !== 0) return undefined;
		const { rows } = JSON.parse(r.stdout) as TailListing;
		return rows.findLast((row) => row.verb === 'point')?.time?.slice(0, 10);
	}

	onMount(async () => {
		const argv = bandArgv(Math.floor(band.clientWidth), H);
		let [picture, listing] = await Promise.all([run(argv), run(fieldsArgv())]);
		if (listing.code === 0) {
			lanes = Object.fromEntries(
				(JSON.parse(listing.stdout) as FieldListing[]).map((f) => [f.field, f])
			);
		}
		svg = picture.code === 0 ? inlineSvg(picture.stdout) : null;
		if (occur && svg && !svg.querySelector(`[data-field="${CSS.escape(field)}"] .point-fg`)) {
			const day = await lastDot();
			if (day) {
				picture = await run([...argv, '--end-date', day]);
				svg = picture.code === 0 ? inlineSvg(picture.stdout) : null;
				sample = day;
			}
		}
		// Gone while it read: the shape changed, or the tab did.
		if (!svg || !band) return;
		band.prepend(svg);
		const disc = [...svg.querySelectorAll(`g.field[data-field="${CSS.escape(field)}"] .point-bg`)].at(-1);
		dotX = disc ? Number(disc.getAttribute('cx')) : band.clientWidth / 2;
		named = [...svg.querySelectorAll('g.field')].map((g) => g.getAttribute('data-field') ?? '');
	});

	/** The dot box's handles, by the sides each moves: four corners, four edges. */
	const HANDLES = [-1, 0, 1].flatMap((sx) => [-1, 0, 1].map((sy) => [sx, sy])).filter(([x, y]) => x || y);
	/** A corner's cursor, by the sign of its sides' product. */
	const CURSORS: Record<string, string> = { '1': 'nwse-resize', '-1': 'nesw-resize' };

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
		: // A `y` with no `height` is the band's centre, not its top (da-1r506): the drawn lane says where that is.
			{ y: (place.height === undefined ? undefined : place.y) ?? drawn?.y ?? 0, h: place.height ?? drawn?.height ?? 0.12 };

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
		// The dot's size is not the lane's, and goes along as it was.
		const { glyph_scale, glyph_aspect } = place;
		const at =
			to && a?.spec && a.height > 0
				? {
						relative_to: to,
						anchor_pos_ratio: round((y - a.y) / a.height),
						anchor_size_ratio: round(h / a.height)
					}
				: { y: round(y), height: round(h) };
		place = { ...at, glyph_scale, glyph_aspect };
	}

	/** A size factor as stored: 1 is unset. */
	const factor = (n: number) => (round(n) === 1 ? undefined : round(n));
	$: scale = place.glyph_scale ?? 1;
	$: aspect = place.glyph_aspect ?? 1;
	/** The dot's box in px: its unit times its scale, about the lane's centre. */
	$: dotH = unit(lane.h) * scale * H;
	$: dotW = dotH * aspect;

	function grab(mode: 'move' | 'top' | 'bottom', e: PointerEvent) {
		e.preventDefault();
		e.stopPropagation();
		drag = { mode, at: e.clientY, y: lane.y, h: lane.h };
	}

	/** Press a handle of the dot's box: `sx`/`sy` are the sides it moves. */
	function size(sx: number, sy: number, e: PointerEvent) {
		grab('move', e);
		drag = { ...drag!, mode: 'dot', sx, sy, ax: e.clientX, b: dotH, w: dotW };
	}

	function slide(e: PointerEvent) {
		if (!drag) return;
		const d = (e.clientY - drag.at) / H;
		const { y, h } = drag;
		if (drag.mode === 'dot') {
			// About the centre, so an edge dragged out by d grows the box by 2d.
			const { sx = 0, sy = 0, b = 1, w = 1 } = drag;
			let tall = b + 2 * sy * (e.clientY - drag.at);
			let wide = w + 2 * sx * (e.clientX - (drag.ax ?? 0));
			if (locked) {
				const k = sy ? tall / b : wide / w;
				[tall, wide] = [b * k, w * k];
			}
			tall = clamp(tall, MIN * H, H);
			wide = clamp(wide, MIN * H, band.clientWidth);
			if (h > 0) place = { ...place, glyph_scale: factor(tall / (unit(h) * H)), glyph_aspect: locked ? place.glyph_aspect : factor(wide / tall) };
		} else if (drag.mode === 'move') put(clamp(y + d, 0, 1 - h), h);
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

	// The dots follow the box: each mark is scaled about its own centre
	// from the size the band drew it at, undoing the group's stretch of
	// the lane, `k`. A CSS transform replaces the mark's own stretch, so the
	// width is scaled from an unstretched one. Only this field's marks: the
	// origin would move another's stretch.
	$: if (own && occur && drawn && drawn.height > 0) {
		const grow = (unit(lane.h) * scale) / (unit(drawn.height) * saved.scale);
		const k = lane.h / drawn.height;
		for (const mark of own.querySelectorAll<SVGElement>('.point-fg'))
			Object.assign(mark.style, {
				transformBox: 'fill-box',
				transformOrigin: 'center',
				transform: `scale(${grow * aspect}, ${grow / k})`
			});
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
		{#if occur}
			<!-- svelte-ignore a11y-no-static-element-interactions -->
			<div
				class="dot"
				class:dragging={drag}
				data-field={field}
				style:top="{(lane.y + lane.h / 2) * H - dotH / 2}px"
				style:left="{dotX - dotW / 2}px"
				style:height="{dotH}px"
				style:width="{dotW}px"
				on:pointerdown={(e) => grab('move', e)}
			>
				{#each HANDLES as [sx, sy] (`${sx}${sy}`)}
					<span
						class="handle"
						data-handle="{sx},{sy}"
						style:left="{(sx + 1) * 50}%"
						style:top="{(sy + 1) * 50}%"
						style:cursor={CURSORS[`${sx * sy}`] ?? (sx ? 'ew-resize' : 'ns-resize')}
						on:pointerdown={(e) => size(sx, sy, e)}
					></span>
				{/each}
			</div>
		{:else}
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
		{/if}
	</div>

	{#if occur}
		<div class="size">
			<span class="dims">dot ×{scale.toFixed(2)} · {aspect.toFixed(2)} wide</span>
			<label><input type="checkbox" name="lock_aspect" bind:checked={locked} /> lock aspect</label>
			<button
				type="button"
				class="reset"
				disabled={scale === 1 && aspect === 1}
				on:click={() => (place = { ...place, glyph_scale: undefined, glyph_aspect: undefined })}>reset</button
			>
			{#if sample}<span class="unchosen">no dot today — drawn on {sample}</span>{/if}
		</div>
	{/if}

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
		background: var(--raised);
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

	.size {
		display: flex;
		flex-wrap: wrap;
		gap: 0.25rem 1rem;
		align-items: center;
		font-size: 0.75rem;
		color: var(--ink);
	}

	.size label {
		display: flex;
		gap: 0.4rem;
		align-items: center;
	}

	/* The occur dot's box: its body moves the lane, a handle sizes the dot. */
	.dot {
		position: absolute;
		box-sizing: border-box;
		border: 1px dashed var(--accent);
		cursor: grab;
		touch-action: none;
	}

	.dot.dragging {
		cursor: grabbing;
	}

	.handle {
		position: absolute;
		inline-size: 9px;
		block-size: 9px;
		transform: translate(-50%, -50%);
		background: var(--accent);
		touch-action: none;
	}

	.reset {
		background: none;
		border: 1px solid var(--rule);
		border-radius: 4px;
		color: var(--ink-bright);
		font: inherit;
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
