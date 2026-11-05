<!--
	Collect's now strip (da-uuzu6): the last half second of today's data layer,
	now at the right edge, sliding with the clock as the nowview's picture
	does, its marks laid as the nowview lays them (`laid`, #lib/nowStrip.ts).
	A state that began within the strip's half second sweeps in from the
	right, and a dot crosses the strip in half a second. `pending` is what a
	tap added before the database answered, drawn over the layer.

	The marks are laid in px from `origin`, so their numbers stay small, and
	one transform on `.strip-slide` slides them (not `.flow` or `.slide`, the nowview's own): on each frame while an edge is in
	the strip or any part of a dot is (vividn, 2026-11-05: «they should go
	completely off the left side and then be destroyed»), and otherwise not
	until one comes or `origin` is laid again. A dot of the layer wholly past
	the left edge is removed. At a selected instant `at` (da-0qdsp) the strip's
	span about it is drawn, still, the instant at the center.

	The nowview draws it too, in its now section (da-d6vzq): `vertical` where
	time runs down the page, now at the strip's foot, and `prefix` naming its
	copies of the stripes apart from collect's.
-->
<script lang="ts">
	import { untrack } from 'svelte';
	import { LAYER_HEIGHT } from './panes.js';
	import { STRIP_MS, laid, type Mark } from './nowStrip.js';

	let {
		doc,
		pending = [],
		at = null,
		width,
		height,
		mirrored = false,
		vertical = false,
		prefix = 'collect-'
	}: {
		doc: Document | null | undefined;
		pending?: Mark[];
		at?: number | null;
		width: number;
		height: number;
		mirrored?: boolean;
		vertical?: boolean;
		prefix?: string;
	} = $props();

	/** How far either side of `origin` a block is laid, and how often `origin` is laid again. */
	const SPAN = 600_000;
	const REBASE = 60_000;

	let live = $state(Date.now());
	const origin = $derived(at ?? live);
	let svg = $state<SVGSVGElement>();
	let flow = $state<SVGGElement>();
	let layer = $state<SVGGElement>();
	/** The strip's length along time, and across it. */
	const [along, wide] = $derived(vertical ? [height, width] : [width, height]);
	const k = $derived(along / STRIP_MS);
	const y = $derived(wide / LAYER_HEIGHT);
	const drawn = $derived(laid(doc, origin, k, y, SPAN, vertical, prefix));
	/** `v` along time and `u` across, as x and y. */
	const xy = (v: number, u: number) => (vertical ? [u, v] : [v, u]);
	/** Each tap's mark as drawn: clamped to `SPAN` about `origin`, in px from it. */
	const taps = $derived(
		pending.map((m) => {
			const [from, to] = [Math.max(m.from, origin - SPAN), Math.min(m.to, origin + SPAN)];
			return { ...m, from, to, x: (from - origin) * k, w: (to - from) * k };
		})
	);

	$effect(() => void layer?.replaceChildren(drawn.node));

	$effect(() => {
		const edges = [...drawn.edges, ...taps.filter((m) => !m.dot).flatMap((m) => [m.from, m.to])];
		/** Each dot from its instant to when it is wholly past the left edge: the strip's span and its half length later, in the strip's units. */
		// Screen px a unit along time: a nested `<svg>`'s client rect is its content's.
		const ctm = svg?.getScreenCTM();
		const px = (ctm && (vertical ? Math.hypot(ctm.c, ctm.d) : Math.hypot(ctm.a, ctm.b))) || 1;
		const half = (els: Element[]) => Math.max(0, ...els.map((el) => (vertical ? el.getBoundingClientRect().height : el.getBoundingClientRect().width) / 2 / px));
		const dots = [
			...drawn.dots.map((d) => ({ ...d, to: d.at + STRIP_MS + half(d.marks) / k })),
			...taps.filter((m) => m.dot).map((m) => ({ at: m.from, marks: [], to: m.from + STRIP_MS + ((m.h / 2) * y * (vertical ? 1 : (m.aspect ?? 1))) / k }))
		];
		void along;
		const to = (v: number) => (vertical ? `translate(0 ${v})` : `translate(${v} 0)`);
		if (at !== null) return void flow?.setAttribute('transform', to(along / 2));
		let frame = 0;
		let timer: ReturnType<typeof setTimeout> | undefined;
		const slide = () => {
			const now = Date.now();
			if (now - origin > REBASE) return void (live = now);
			flow?.setAttribute('transform', to(along - (now - origin) * k));
			for (const d of dots) if (d.to < now) d.marks.forEach((el) => el.remove());
			if (edges.some((e) => e > now - STRIP_MS - 50 && e <= now) || dots.some((d) => d.at <= now && d.to + 50 > now)) frame = requestAnimationFrame(slide);
			else timer = setTimeout(slide, Math.min(origin + REBASE, ...[...edges, ...dots.flatMap((d) => [d.at, d.to])].filter((e) => e > now)) - now + 1);
		};
		untrack(slide);
		return () => {
			cancelAnimationFrame(frame);
			clearTimeout(timer);
		};
	});

	/** The layer's stripes, under ids of the strip's own. */
	function patterns(defs: SVGDefsElement, of: Document | null | undefined) {
		const lay = (of: Document | null | undefined) =>
			defs.replaceChildren(
				...[...(of?.querySelectorAll('pattern[id]') ?? [])].map((p) => {
					const copy = document.importNode(p, true);
					copy.id = `${prefix}${p.id}`;
					return copy;
				})
			);
		lay(of);
		return { update: lay };
	}
</script>

<svg class="now-strip" class:mirrored {width} {height} aria-hidden="true" bind:this={svg}>
	<defs use:patterns={doc}></defs>
	<g class="strip-slide" bind:this={flow}>
		<g bind:this={layer}></g>
		{#each taps as m}
			{#if m.dot}
				{@const a = m.aspect ?? 1}
				{@const [cx, cy] = xy(m.x, (m.y + m.h / 2) * y)}
				<circle
					class="pending"
					data-mark={m.field}
					{cx}
					{cy}
					r={(m.h / 2) * y}
					fill={m.fill}
					transform={a === 1 ? undefined : `matrix(${a} 0 0 1 ${cx * (1 - a)} 0)`}
				/>
			{:else}
				{@const [x, ry] = xy(m.x, m.y * y)}
				{@const [w, h] = xy(m.w, m.h * y)}
				<rect class="pending" class:ground={m.fill === null} data-mark={m.field} {x} y={ry} width={w} height={h} fill={m.fill} />
			{/if}
		{/each}
	</g>
</svg>

<style>
	.now-strip {
		position: absolute;
		inset: 0;
		overflow: hidden;
		pointer-events: none;
		background: none;
	}

	.mirrored {
		transform: scaleX(-1);
	}

	.ground {
		fill: var(--ground);
	}
</style>
