<!--
	The field form's placement, on its Basics (da-btyvj, da-i9upr): where a field's lane sits in
	the day, chosen by dragging it on today's REAL band — the CLI's own
	`dayview`, so the field is placed against its actual neighbours.

	The model was already there (`PresentationSpec`: `y`/`height` as
	fractions of the day, or the anchor triple); this is its face. Without
	one, a field's `y` is a hash of its NAME — a lane nobody chose — and the
	lane's reset has nothing to undo until someone does.

	No box is drawn at rest (da-r6wwe): a press and drag on any of the
	field's marks moves its lane, and a tap on one draws a thin box round
	that mark, whose hints size it — on a bar a dot at its top and bottom
	centre, which move that edge of the lane. Anchored to a neighbour (⚓),
	the band is that neighbour's lane at its full height, and the same drag
	writes the RATIOS to it and not `y` / `height`, so the lane moves when
	its anchor does. Nothing is written here: `place` is bound to the
	modal, which saves it with the rest.

	Anchoring is asked for (da-w5ruf), vividn, 2026-10-26: «the chain menu
	is too much». A field allows it by «place other fields relative to this
	one?» (`anchorable`), and one with fields anchored to it names them
	instead, each a link to its setup, so it cannot be unchecked. The ⚓
	picker offers only those, and is not drawn when there are none;
	anchored, the line reads «⚓ body ×»: «body» opens body's setup, ×
	frees the lane where it stands. The checkbox is now a toggle beside the
	eye, «⚓ be anchor», on an on/off or switch field alone, and the names
	read «⚓ anchors: …» (da-y72v8).

	An occur field's mark is its dot (da-c1b6m): the box is drawn round the
	dot tapped, centred on the lane, its corners hinted; an edge or corner
	sizes the dot about its centre, writing
	`symbol_scale` (the dot's height over the standard dot's, so a field tied
	to this one stays put) and `symbol_aspect`. The lock, on until turned off,
	keeps width over height, the reset puts back a round dot of the standard
	size, whatever the lane (da-ijjjn) — which is how tall every view draws
	it, as a fraction of the day. The lane's own reset puts back the lane it
	has with no `y` or `height` (da-y72v8).

	The band draws the field as the form holds it (da-srrqd), vividn,
	2026-10-22: «you can see the dots/states/bars and what they look like as
	real data as you're aligning it to the rest of your data». `dayview
	--sample` takes the form's spec, at the lane as it stands, and on a day
	with nothing of the field to draw it draws entries in its shape that no
	database holds. It is asked again whenever the spec changes, but not
	while a drag is under way: the drawn marks are carried there meanwhile.

	What is drawn around the field is the eye's above the band (da-jc8ic),
	cycled by a click and opened on «context» each time: «context», the
	field's sample alone over the day before now; «current», every field's
	state now across the band, the sample over it; «6 hours», the six hours
	before now with no integrity marks, where the field's own entries there
	are what is placed, faintly outlined, and a sample only when it has none
	(`dayview --context`). Each ends at now, at the band's right edge, and a
	field with no name yet has no sample in any (da-y72v8). A click on it
	lets go of a tapped mark's box, whose mark the new picture draws
	elsewhere.
-->
<script lang="ts">
	import { onMount, untrack } from 'svelte';
	import { on } from 'svelte/events';
	import { inlineSvg } from './svgSafe.js';
	import { bandArgv, fieldArgv, fieldsArgv } from './calls.js';
	import { run } from './run.js';
	import type { FieldListing } from './fields.js';
	import type { Place, Spec } from './track.js';

	let {
		field,
		place = $bindable(),
		/** Whether the field is an occur field, the one whose dots are sized here. */
		occur = false,
		/** The spec a save would store, which the band draws: none is drawn for a field with no name yet (da-y72v8). */
		spec = null,
		/** Go to the field whose setup a link names. */
		onopen
	}: { field: string; place: Place; occur?: boolean; spec?: Spec | null; onopen: (field: string) => void } = $props();

	/** The day's height in pixels: tall enough to take a lane's edge with a finger, two thirds of the 160 it was (da-y72v8). */
	const H = 110;
	/** The thinnest lane a drag leaves. */
	const MIN = 0.02;
	/** A dot's height at a scale of 1, whatever its lane: the CLI's `STANDARD_DOT`, two thirds of a lane with no height (`tests/standard-dot-pure.test.mjs`). */
	const STANDARD_DOT = 0.08;

	let band: HTMLDivElement | undefined = $state.raw();
	let svg = $state.raw<SVGSVGElement | null>(null);
	/** Every field's lane as the pictures resolve it (`fields --json`), anchors included. */
	let lanes: Record<string, FieldListing> = $state.raw({});
	/** The row of a field `lanes` has none for, a new one: its lane is where the pictures will draw it. */
	let unlisted: FieldListing | undefined = $state.raw();
	/** The fields today's band draws, whose names label their lanes. */
	let named: string[] = $state.raw([]);
	let drag: {
		mode: 'move' | 'top' | 'bottom' | 'dot';
		at: number;
		y: number;
		h: number;
		/** For `dot`: the handle's sides (-1, 0, 1 on x and y), the box's size in px. */
		sx?: number;
		sy?: number;
		b?: number;
		w?: number;
		/** The press's x. */
		ax?: number;
		/** For `move`: the mark pressed, selected if it is let go where it was; whether it has moved since. */
		mark?: { x: number; w: number };
		moved?: boolean;
	} | null = $state(null);
	/** Whether sizing the dot keeps its width over its height: on until turned off (da-y72v8). */
	let locked = $state(true);
	/** The mark tapped, which the box is drawn round: its left and width along the band, in px. */
	let sel: { x: number; w: number } | null = $state.raw(null);
	/** The lane and dot the band was drawn at, which the marks are carried from. */
	let shown = $state.raw({ y: 0, h: 0, scale: 1 });
	/** Whether `fields --json` has been read, which the lane of a field with no place is. */
	let listed = $state(false);
	/** What the band draws around the field, by `dayview --context`, in the order a click turns them. */
	const AROUND = [
		{ context: 'none', label: 'context' },
		{ context: 'now', label: 'current' },
		{ context: '6h', label: '6 hours' }
	] as const;
	let around = $state(0);

	onMount(async () => {
		const listing = await run(fieldsArgv());
		if (listing.code === 0) {
			lanes = Object.fromEntries(
				(JSON.parse(listing.stdout) as FieldListing[]).map((f) => [f.field, f])
			);
		}
		listed = true;
	});

	/** What the band is to draw, and what it was last asked to. */
	let want = '';
	let asked = '';
	let drawing = false;
	/** Draw the band until it is what is wanted: the latest only, however many changes came while one drew. */
	async function draw() {
		if (drawing) return;
		drawing = true;
		while (asked !== want && band) {
			asked = want;
			const { at, sample, context } = JSON.parse(asked);
			const picture = await run([
				...bandArgv(Math.floor(band.clientWidth), H),
				...(sample ? ['--sample', JSON.stringify(sample)] : []),
				'--context',
				context
			]);
			// Gone while it read: the shape changed, or the tab did. A spec the CLI refuses keeps the last picture.
			const next = picture.code === 0 && band ? inlineSvg(picture.stdout) : null;
			if (!next) continue;
			// What it was drawn around — `none`, `now` or `6h`, the eye's — for a reader to wait on.
			next.dataset.context = context;
			svg?.remove();
			band.prepend(next);
			[svg, shown] = [next, at];
			named = [...next.querySelectorAll('g.field')].map((g) => g.getAttribute('data-field') ?? '');
		}
		drawing = false;
		// The picture is the one wanted, for a reader to wait on before it reads a mark (da-bnc6r).
		if (band) band.dataset.drawn = '';
	}

	/** The dot box's handles, by the sides each moves: four corners, four edges. */
	const HANDLES = [-1, 0, 1].flatMap((sx) => [-1, 0, 1].map((sy) => [sx, sy])).filter(([x, y]) => x || y);
	/** A bar box's handles: the lane's edges, each moved by the hint at its centre. */
	const EDGES = ['top', 'bottom'] as const;
	/** A corner's cursor, by the sign of its sides' product. */
	const CURSORS: Record<string, string> = { '1': 'nwse-resize', '-1': 'nesw-resize' };

	const round = (n: number) => Math.round(n * 1000) / 1000;
	const clamp = (n: number, lo: number, hi: number) => Math.min(hi, Math.max(lo, n));

	/** Ask for the row of `name`, kept only while it is still the field being placed. */
	async function unknown(name: string) {
		const r = await run(fieldArgv(name));
		const row = r.code === 0 ? (JSON.parse(r.stdout) as FieldListing[])[0] : undefined;
		if (row?.field === field) unlisted = row;
	}
	$effect.pre(() => {
		if (listed && field && !lanes[field] && unlisted?.field !== field) untrack(() => unknown(field));
	});

	/** The lane as the band drew it, from the stored spec, or where it will for a field nobody placed (da-gq232). */
	const drawn = $derived(lanes[field] ?? (unlisted?.field === field ? unlisted : undefined));
	// The resolver holds a field only by an anchor with a spec; without one it draws it free.
	const anchor = $derived(place.relative_to && lanes[place.relative_to]?.spec ? lanes[place.relative_to] : undefined);
	/** The lane as it stands now, in fractions of the day. */
	const lane = $derived(
		anchor
			? {
					y: anchor.y + anchor.height * (place.anchor_pos_ratio ?? 0),
					h: anchor.height * (place.anchor_size_ratio ?? 1)
				}
			: // A `y` with no `height` is the band's centre, not its top (da-1r506): the drawn lane says where that is.
				{ y: (place.height === undefined ? undefined : place.y) ?? drawn?.y ?? 0, h: place.height ?? drawn?.height ?? 0.12 }
	);
	/** What the band spans, in fractions of the day: the anchor's lane, or the whole day. */
	const frame = $derived(anchor?.height ? { y: anchor.y, h: anchor.height } : { y: 0, h: 1 });
	/** A fraction of the day as px down the band. */
	const px = (v: number) => ((v - frame.y) / frame.h) * H;

	/** Would tying to `name` close a loop back to this field? */
	function loops(name: string, seen = new Set<string>()): boolean {
		if (name === field) return true;
		if (seen.has(name)) return false;
		seen.add(name);
		const next = lanes[name]?.spec?.relative_to;
		return !!next && loops(next, seen);
	}
	/** The fields anchored to this one. */
	const anchored = $derived(Object.keys(lanes).filter((n) => lanes[n].spec?.relative_to === field).sort());
	// Only a field with a spec can anchor: the resolver has nothing else to hold it by.
	const anchors = $derived(
		Object.keys(lanes).filter(
			(n) =>
				!!lanes[n].spec &&
				(lanes[n].spec?.anchorable || Object.values(lanes).some((l) => l.spec?.relative_to === n)) &&
				lanes[n].height > 0 &&
				!loops(n)
		)
	);

	/**
	 * Put the lane at `y`/`h`: as ratios to `to` when tied, as absolutes when
	 * free — and free when `to` is gone or has no height to be a ratio of.
	 */
	function put(y: number, h: number, to = place.relative_to) {
		const a = to ? lanes[to] : undefined;
		// The dot's size is not the lane's, and goes along as it was.
		const { symbol_scale, symbol_aspect, anchorable } = place;
		const at =
			to && a?.spec && a.height > 0
				? {
						relative_to: to,
						anchor_pos_ratio: round((y - a.y) / a.height),
						anchor_size_ratio: round(h / a.height)
					}
				: { y: round(y), height: round(h) };
		place = { ...at, symbol_scale, symbol_aspect, anchorable };
	}

	/** Anchor to `to`, the lane put inside its anchor's where it stood outside it. */
	function tie(to: string) {
		const a = lanes[to];
		const h = Math.min(lane.h, a.height);
		put(clamp(lane.y, a.y, a.y + a.height - h), h, to);
	}

	/**
	 * Back to the lane with no `y` or `height`: the name's, or, anchored, the
	 * anchor's whole lane — written as the ratios 0 and 1, since the triple is
	 * all set or none (`validate_anchor_triple`), and a half one is drawn free.
	 */
	const unplace = () =>
		(place = place.relative_to ? { ...place, anchor_pos_ratio: 0, anchor_size_ratio: 1 } : { ...place, y: undefined, height: undefined });
	const placed = $derived(
		place.relative_to ? (place.anchor_pos_ratio ?? 0) !== 0 || (place.anchor_size_ratio ?? 1) !== 1 : place.y !== undefined || place.height !== undefined
	);

	/** A size factor as stored: 1 is unset. */
	const factor = (n: number) => (round(n) === 1 ? undefined : round(n));
	const scale = $derived(place.symbol_scale ?? 1);
	const aspect = $derived(place.symbol_aspect ?? 1);
	/** The dot's box in px: the standard times its scale, about the lane's centre, in the frame. */
	const dotH = $derived((STANDARD_DOT * scale * H) / frame.h);
	const dotW = $derived(dotH * aspect);

	function grab(mode: 'move' | 'top' | 'bottom', e: PointerEvent) {
		e.preventDefault();
		e.stopPropagation();
		drag = { mode, at: e.clientY, y: lane.y, h: lane.h, ax: e.clientX };
	}

	/** This field's mark at a point on the band, under whatever is drawn over it (gridlines, the now-line, another field); never the page's own dayview under the dialog. */
	const markAt = (x: number, y: number) =>
		document.elementsFromPoint(x, y).find((el) => svg?.contains(el) && el.closest('g.field')?.getAttribute('data-field') === field);

	/** Press the band: on a mark, the lane moves with it, and it is selected if let go where it was; elsewhere, nothing is. */
	function press(e: PointerEvent) {
		const mark = markAt(e.clientX, e.clientY);
		if (!mark) return void (sel = null);
		const r = mark.getBoundingClientRect();
		const left = band!.getBoundingClientRect().left + band!.clientLeft;
		grab('move', e);
		drag = { ...drag!, mark: { x: r.left - left, w: r.width } };
	}

	function release() {
		if (drag?.mark && !drag.moved) sel = drag.mark;
		drag = null;
	}

	/** Press a handle of the dot's box: `sx`/`sy` are the sides it moves. */
	function size(sx: number, sy: number, e: PointerEvent) {
		grab('move', e);
		drag = { ...drag!, mode: 'dot', sx, sy, b: dotH, w: dotW };
	}

	function slide(e: PointerEvent) {
		if (!drag) return;
		// A tap's jitter moves nothing.
		if (drag.mark && !drag.moved && Math.hypot(e.clientX - (drag.ax ?? 0), e.clientY - drag.at) < 4) return;
		drag.moved = true;
		const d = ((e.clientY - drag.at) / H) * frame.h;
		const { y, h } = drag;
		const [lo, hi, min] = [frame.y, frame.y + frame.h, MIN * frame.h];
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
			wide = clamp(wide, MIN * H, band!.clientWidth);
			place = { ...place, symbol_scale: factor((tall * frame.h) / (STANDARD_DOT * H)), symbol_aspect: locked ? place.symbol_aspect : factor(wide / tall) };
		} else if (drag.mode === 'move') put(clamp(y + d, lo, hi - h), h);
		else if (drag.mode === 'bottom') put(y, clamp(h + d, min, hi - y));
		else {
			const top = clamp(y + d, lo, y + h - min);
			put(top, y + h - top);
		}
	}

	// The band is drawn by the spec as the form holds it, freed at the lane
	// as it stands, so the field is drawn where its box is, tied or not.
	$effect.pre(() => {
		if (band && listed && !drag) {
			const at = { y: round(lane.y), h: round(lane.h), scale };
			const free = { relative_to: undefined, anchor_pos_ratio: undefined, anchor_size_ratio: undefined, y: at.y, height: at.h };
			want = JSON.stringify({ at, sample: field && spec && { ...spec, ...free }, context: AROUND[around].context });
			if (want !== asked) delete band.dataset.drawn;
			untrack(draw);
		}
	});

	// The band is zoomed to the frame: the anchor's lane spans it.
	$effect(() => {
		if (svg) Object.assign(svg.style, { transformOrigin: '0 0', transform: `translateY(${px(0)}px) scaleY(${1 / frame.h})` });
	});

	// The field's own marks follow the lane while it moves: the band was drawn
	// at `shown`, so its group is carried from there to here.
	const own = $derived(svg?.querySelector(`g.field[data-field="${CSS.escape(field)}"]`));
	// Its own entries, not a sample, whose marks name no entry: outlined against the rest.
	// Read again with each picture: the marks change only with a new `svg`.
	$effect(() => {
		own?.classList.toggle('real', !!own.querySelector('[data-id]'));
	});
	$effect(() => {
		if (own && shown.h > 0) {
			const k = lane.h / shown.h;
			own.setAttribute('transform', `matrix(1 0 0 ${k} 0 ${(lane.y - shown.y * k) * H})`);
		}
	});
	$effect(() => {
		if (own instanceof SVGElement) own.style.cursor = drag?.moved ? 'grabbing' : 'grab';
	});

	// The dots follow the box: each mark is scaled about its own centre
	// from the size the band drew it at, undoing the group's stretch of
	// the lane, `k`, and widened as the frame's zoom heightens it. A CSS
	// transform replaces the mark's own stretch, so the
	// width is scaled from an unstretched one. Only this field's marks: the
	// origin would move another's stretch.
	$effect(() => {
		if (own && occur && shown.h > 0) {
			const grow = scale / shown.scale;
			const k = lane.h / shown.h;
			for (const mark of own.querySelectorAll<SVGElement>('.point-fg'))
				Object.assign(mark.style, {
					transformBox: 'fill-box',
					transformOrigin: 'center',
					transform: `scale(${(grow * aspect) / frame.h}, ${grow / k})`
				});
		}
	});

	const relation = $derived(
		!anchor
			? ''
			: (place.anchor_pos_ratio ?? 0) >= 1
				? 'below'
				: (place.anchor_pos_ratio ?? 0) + (place.anchor_size_ratio ?? 1) <= 0
					? 'above'
					: 'on'
	);
</script>

<svelte:window onpointermove={slide} onpointerup={release} onpointercancel={() => (drag = null)} />

<!-- It drags itself: a sideways drag in it is never the pane's tab swipe (`data-pans`, PaneFrame.svelte; da-4t0yn). -->
<div class="placement" data-pans="x">
	<div class="top">
		<button
			type="button"
			class="around toggle"
			data-context={AROUND[around].context}
			aria-label="what is drawn around {field}: {AROUND[around].label}; click for the next"
			title="what is drawn around {field}: click for the next"
			onclick={() => ([around, sel] = [(around + 1) % AROUND.length, null])}
		>
			<svg viewBox="0 0 16 16" aria-hidden="true">
				{#if around === 0}
					<path d="M1.5 6.5 Q8 12.5 14.5 6.5 M4 9.6 2.8 11.4 M8 11 V13.3 M12 9.6 13.2 11.4" />
				{:else if around === 1}
					<path d="M1.5 8 H14.5 Q8 14.5 1.5 8Z" />
					<path class="pupil" d="M5.6 8 A2.4 2.4 0 0 0 10.4 8Z" />
				{:else}
					<path d="M1.5 8 Q8 1.5 14.5 8 Q8 14.5 1.5 8Z" />
					<circle class="pupil" cx="8" cy="8" r="2.4" />
				{/if}
			</svg>
			<!-- Every label in one cell, the others hidden, so the button is as wide whichever it reads. -->
			<span class="labels">
				{#each AROUND as a, i (a.context)}<span class:shown={i === around}>{a.label}</span>{/each}
			</span>
		</button>
		{#if !occur && !anchored.length}
			<button
				type="button"
				class="be-anchor toggle"
				aria-pressed={!!place.anchorable}
				title="place other fields relative to this one"
				onclick={() => (place = { ...place, anchorable: !place.anchorable || undefined })}>⚓ be anchor</button
			>
		{/if}
		<div class="tie">
			{#if place.relative_to}
				<span class="anchor">
					⚓ <button type="button" class="link" onclick={() => onopen(place.relative_to ?? '')}>{place.relative_to}</button>
					<button type="button" class="unanchor" aria-label="unanchor from {place.relative_to}" onclick={() => put(lane.y, lane.h, '')}>×</button>
				</span>
			{:else if anchors.length}
				<select aria-label="anchor to a field" value="" onchange={(e) => tie(e.currentTarget.value)}>
					<option value="">anchor to</option>
					{#each anchors as name (name)}
						<option value={name}>⚓ {name}</option>
					{/each}
				</select>
			{/if}
		</div>
	</div>

	<!-- A finger on a mark drags it, and does not scroll the dialog. -->
	<!-- svelte-ignore a11y_no_static_element_interactions -->
	<div
		class="band"
		bind:this={band}
		style:height="{H}px"
		onpointerdown={press}
		{@attach (node) =>
			on(node, 'touchstart', (e) => markAt(e.touches[0].clientX, e.touches[0].clientY) && e.preventDefault(), { passive: false })}
	>
		{#each named.filter((n) => n !== field && lanes[n]) as name (name)}
			<span class="name" style:top="{px(lanes[name].y)}px">{name}</span>
		{/each}
		<span class="own" style:top="{px(lane.y + lane.h / 2)}px">{field}</span>
		{#if sel && occur}
			<div
				class="box dot"
				data-field={field}
				style:top="{px(lane.y + lane.h / 2) - dotH / 2}px"
				style:left="{sel.x + sel.w / 2 - dotW / 2}px"
				style:height="{dotH}px"
				style:width="{dotW}px"
			>
				{#each HANDLES as [sx, sy] (`${sx}${sy}`)}
					<span
						class="handle"
						class:hint={sx && sy}
						data-handle="{sx},{sy}"
						style:left="{(sx + 1) * 50}%"
						style:top="{(sy + 1) * 50}%"
						style:transform="translate({(sx - 1) * 50}%, {(sy - 1) * 50}%)"
						style:cursor={CURSORS[`${sx * sy}`] ?? (sx ? 'ew-resize' : 'ns-resize')}
						onpointerdown={(e) => size(sx, sy, e)}
					></span>
				{/each}
			</div>
		{:else if sel}
			<div
				class="box bar"
				data-field={field}
				style:top="{px(lane.y)}px"
				style:height="{(lane.h / frame.h) * H}px"
				style:left="{sel.x}px"
				style:width="{sel.w}px"
			>
				{#each EDGES as edge (edge)}
					<span
						class="handle hint round"
						data-handle={edge}
						style:left="50%"
						style:top={edge === 'top' ? '0' : '100%'}
						onpointerdown={(e) => grab(edge, e)}
					></span>
				{/each}
			</div>
		{/if}
	</div>

	{#snippet reset(what: string, disabled: boolean, onclick: () => void)}
		<button type="button" class="icon" aria-label="reset {what}" title="reset {what}" {disabled} {onclick}>
			<svg viewBox="0 0 16 16" aria-hidden="true"><path d="M3.5 8 A4.5 4.5 0 1 0 5 4.6 M5 1.8 V4.6 H2.2" /></svg>
		</button>
	{/snippet}

	{#if occur}
		<p class="row size">
			{@render reset("the dot's size", scale === 1 && aspect === 1, () => (place = { ...place, symbol_scale: undefined, symbol_aspect: undefined }))}
			<button
				type="button"
				class="icon toggle"
				aria-label="lock the dot's aspect"
				title={locked ? 'aspect locked: sizing keeps its shape' : 'aspect free: each side sizes alone'}
				aria-pressed={locked}
				onclick={() => (locked = !locked)}
			>
				<svg viewBox="0 0 16 16" aria-hidden="true">
					<rect x="3.5" y="7.5" width="9" height="6.5" rx="1" />
					<path d={locked ? 'M5.5 7.5 V5 A2.5 2.5 0 0 1 10.5 5 V7.5' : 'M5.5 7.5 V5 A2.5 2.5 0 0 1 10.5 5 V5.5'} />
				</svg>
			</button>
			<span class="dims">{(scale * aspect).toFixed(2)} × {scale.toFixed(2)}</span>
		</p>
	{/if}

	<p class="row readout">
		{@render reset('the lane', !placed, unplace)}
		<span>
			{#if anchor}
				{relation} {place.relative_to} · ×{place.anchor_size_ratio ?? 1}
			{:else if place.relative_to}
				anchored to {place.relative_to}, which has no spec — drawn free at y {lane.y.toFixed(2)}
			{:else}
				y {lane.y.toFixed(2)}, h {lane.h.toFixed(2)}
			{/if}
		</span>
	</p>

	{#if anchored.length}
		<p class="anchorees">
			⚓ anchors: {#each anchored as name, i (name)}{#if i},{' '}{/if}<button type="button" class="link" onclick={() => onopen(name)}>{name}</button>{/each}
		</p>
	{/if}
</div>

<style>
	.placement {
		display: flex;
		flex-direction: column;
		gap: 0.4rem;
	}

	.top {
		display: flex;
		align-items: center;
		gap: 0.5rem;
	}

	.tie {
		margin-inline-start: auto;
	}

	.toggle {
		display: flex;
		gap: 0.3rem;
		align-items: center;
		min-block-size: 28px;
		background: none;
		border: 1px solid var(--rule);
		border-radius: 4px;
		color: var(--ink-bright);
		font: inherit;
		font-size: 0.75rem;
	}

	.toggle[aria-pressed='true'] {
		border-color: var(--accent);
		color: var(--accent);
	}

	.labels {
		display: grid;
	}

	.labels > span {
		grid-area: 1 / 1;
		visibility: hidden;
	}

	.labels > .shown {
		visibility: visible;
	}

	.around svg,
	.icon svg {
		inline-size: 16px;
		block-size: 16px;
		fill: none;
		stroke: currentColor;
		stroke-width: 1.3;
		stroke-linecap: round;
	}

	.around .pupil {
		fill: currentColor;
		stroke: none;
	}

	/* The field's own entries among the rest: enough to tell them apart, no more. */
	.band :global(g.field.real) {
		filter: drop-shadow(0 0 1px color-mix(in srgb, var(--ink-bright) 70%, transparent));
	}

	select {
		min-block-size: 28px;
		background: var(--raised);
		border: 1px solid var(--rule);
		border-radius: 4px;
		color: var(--ink-bright);
		font: inherit;
	}

	.anchor,
	.anchorees {
		margin: 0;
		font-size: 0.75rem;
		color: var(--ink);
	}

	.link,
	.unanchor {
		min-block-size: 28px;
		padding: 0 0.25rem;
		background: none;
		border: none;
		color: var(--accent);
		font: inherit;
		cursor: pointer;
	}

	.link {
		padding: 0;
		text-decoration: underline;
	}

	.unanchor {
		color: var(--muted);
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

	.own {
		left: auto;
		right: 0.25rem;
		transform: translateY(-50%);
		color: var(--accent);
	}

	/* The dot's size and the lane's place: a reset, the dot's lock, the numbers. */
	.row {
		display: flex;
		gap: 0.4rem;
		align-items: center;
		margin: 0;
		font-size: 0.75rem;
		color: var(--ink);
	}

	.icon {
		display: grid;
		place-items: center;
		inline-size: 28px;
		block-size: 28px;
		padding: 0;
		background: none;
		border: 1px solid var(--rule);
		border-radius: 4px;
		color: var(--ink-bright);
	}

	.icon:disabled {
		color: var(--quiet);
	}

	/* The mark tapped: a thin line round it, which only its hints take a press through. */
	.box {
		position: absolute;
		box-sizing: border-box;
		min-block-size: 4px;
		border: 1px solid var(--accent);
		pointer-events: none;
	}

	/* A handle is a finger's reach; a hint, what is drawn of it, is small.
	   A dot's stand outside its box, each on its side or corner (inline):
	   centred on a dot hardly taller than one, they would overlap, the
	   later taking the press (da-ijjjn). */
	.handle {
		position: absolute;
		inline-size: 14px;
		block-size: 14px;
		transform: translate(-50%, -50%);
		pointer-events: auto;
		touch-action: none;
	}

	.handle.hint::after {
		content: '';
		position: absolute;
		inset: 4px;
		background: var(--accent);
	}

	.handle.round::after {
		border-radius: 50%;
	}

	.bar .handle {
		cursor: ns-resize;
	}

</style>
