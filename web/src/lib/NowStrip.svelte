<!--
	Collect's now strip (da-uuzu6): the last half second of today's data layer,
	now at the right edge, sliding with the clock as the nowview's picture
	does, its marks laid as the nowview lays them (`laid`, #lib/nowStrip.ts).
	A state that began within the strip's half second sweeps in from the
	right, and a dot crosses the strip in half a second. `pending` is what a
	tap added before the database answered, drawn in its field's lane of the
	layer, so at its place in the stacking as the layer's own mark is
	(vividn, 2026-11-05: «please keep z ordering during this animation»), and
	over the layer where the layer has no lane of it.

	The marks are laid in px from `origin`, so their numbers stay small, and
	one transform on `.strip-slide` slides them (not `.flow` or `.slide`, the nowview's own): on each frame while an edge is in
	the strip or any part of a dot is (vividn, 2026-11-05: «they should go
	completely off the left side and then be destroyed»), and otherwise not
	until one comes or `origin` is laid again. The stripes do not slide: each
	pattern's copy is moved back as far as the marks move on, so a field's
	stripes stand still on screen while another's state sweeps in (da-5yemo).
	A dot of the layer wholly past
	the left edge is removed. At a selected instant `at` (da-0qdsp) the strip's
	span about it is drawn, still, the instant at the center, the strip `span`
	ms long, which collect zooms out to frame a span selected (da-xqae2).

	The nowview draws it too, in its now section (da-d6vzq): `vertical` where
	time runs down the page, now at the strip's foot, and `prefix` naming its
	copies of the stripes apart from collect's.
-->
<script lang="ts" module>
	/** Each end's clip, by an id no other strip's takes. */
	let ends = 0;
</script>

<script lang="ts">
	import { untrack } from 'svelte';
	import { LAYER_HEIGHT } from './panes.js';
	import { STRIP_MS, laid, type Mark } from './nowStrip.js';

	let {
		doc,
		pending = [],
		at = null,
		span = STRIP_MS,
		width,
		height,
		mirrored = false,
		vertical = false,
		prefix = 'collect-'
	}: {
		doc: Document | null | undefined;
		pending?: Mark[];
		at?: number | null;
		span?: number;
		width: number;
		height: number;
		mirrored?: boolean;
		vertical?: boolean;
		prefix?: string;
	} = $props();

	/** How far either side of `origin` a block is laid, and how often `origin` is laid again. */
	const SPAN = 600_000;
	const REBASE = 60_000;
	/** Past anything a lane draws, either way: an end's clip keeps all before its instant. */
	const FAR = 1e6;

	let live = $state(Date.now());
	const origin = $derived(at ?? live);
	let svg = $state<SVGSVGElement>();
	let flow = $state<SVGGElement>();
	let layer = $state<SVGGElement>();
	/** The strip's length along time, and across it. */
	const [along, wide] = $derived(vertical ? [height, width] : [width, height]);
	const k = $derived(along / span);
	const y = $derived(wide / LAYER_HEIGHT);
	/** How far either side of `origin` is laid: past the strip's half, however far it is zoomed out. */
	const reach = $derived(Math.max(SPAN, span));
	const drawn = $derived(laid(doc, origin, k, y, reach, vertical, prefix));
	/** `v` along time and `u` across, as x and y. */
	const xy = (v: number, u: number) => (vertical ? [u, v] : [v, u]);
	/** Each tap's mark as drawn: clamped to `reach` about `origin`, in px from it. */
	const taps = $derived(
		pending.map((m) => {
			const [from, to] = [Math.max(m.from, origin - reach), Math.min(m.to, origin + reach)];
			return { ...m, from, to, x: (from - origin) * k, w: (to - from) * k };
		})
	);

	/** `v` along time, as a translation. */
	const to = (v: number) => (vertical ? `translate(0 ${v})` : `translate(${v} 0)`);
	/** The strip's copies of the stripes, and how far `.strip-slide` has moved its marks. */
	let stripes: SVGDefsElement | undefined;
	let slid = 0;
	/** The marks moved `v` along time, and every stripe moved back as far, so it stands where the layer drew it. */
	function slideTo(v: number) {
		slid = v;
		flow?.setAttribute('transform', to(v));
		for (const p of stripes?.children ?? []) p.setAttribute('patternTransform', `${to(-v)} ${(p as SVGElement).dataset.drawn}`);
	}

	const NS = 'http://www.w3.org/2000/svg';
	const put = <E extends Element>(el: E, attrs: Record<string, string | number | undefined>) => (Object.entries(attrs).forEach(([name, v]) => v !== undefined && el.setAttribute(name, String(v))), el);
	/** A tap's mark: a dot, or a block. */
	function tapMark(m: (typeof taps)[number]): SVGElement {
		const el = document.createElementNS(NS, m.dot ? 'circle' : 'rect');
		if (m.dot) {
			const a = m.aspect ?? 1;
			const [cx, cy] = xy(m.x, (m.y + m.h / 2) * y);
			put(el, { cx, cy, r: (m.h / 2) * y, transform: a === 1 ? undefined : `matrix(${a} 0 0 1 ${cx * (1 - a)} 0)` });
		} else {
			const [x, ry] = xy(m.x, m.y * y);
			const [w, h] = xy(m.w, m.h * y);
			put(el, { x, y: ry, width: w, height: h });
		}
		put(el, { class: 'pending', 'data-mark': m.field, fill: m.fill ?? undefined });
		return el;
	}

	// The layer, and each tap's mark at the end of its field's lane in the stretch holding now, the last, a lane of
	// its own over the layer where the layer has none. An end draws nothing: what its lane holds so far is clipped to
	// before its instant, so a bar beneath shows through (da-kqloo, vividn, 2026-11-06: «it should wipe transparent instead»).
	$effect(() => {
		const root = layer;
		if (!root) return;
		root.replaceChildren(drawn.node);
		const lanes = new Map([...root.querySelectorAll('g.strip-field[data-mark]')].map((g) => [g.getAttribute('data-mark'), g]));
		const lane = (field: string) => lanes.get(field) ?? lanes.set(field, root.appendChild(document.createElementNS(NS, 'g'))).get(field)!;
		const undo = taps.map((m) => {
			const into = lane(m.field);
			if (m.fill !== null) {
				const el = into.appendChild(tapMark(m));
				return () => el.remove();
			}
			const clip = document.createElementNS(NS, 'clipPath');
			clip.id = `${prefix}end-${++ends}`;
			const [[x, ry], [w, h]] = [xy(-FAR, -FAR), xy(m.x + FAR, 2 * FAR)];
			clip.append(put(document.createElementNS(NS, 'rect'), { x, y: ry, width: w, height: h }));
			const before = put(document.createElementNS(NS, 'g'), { 'clip-path': `url(#${clip.id})` });
			before.append(...into.childNodes);
			into.append(clip, before);
			return () => (clip.remove(), before.replaceWith(...before.childNodes));
		});
		return () => undo.reverse().forEach((f) => f());
	});

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
		if (at !== null) return void slideTo(along / 2);
		let frame = 0;
		let timer: ReturnType<typeof setTimeout> | undefined;
		const slide = () => {
			const now = Date.now();
			if (now - origin > REBASE) return void (live = now);
			slideTo(along - (now - origin) * k);
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
		stripes = defs;
		const lay = (of: Document | null | undefined) => {
			defs.replaceChildren(
				...[...(of?.querySelectorAll('pattern[id]') ?? [])].map((p) => {
					const copy = document.importNode(p, true) as SVGElement;
					copy.id = `${prefix}${p.id}`;
					copy.dataset.drawn = p.getAttribute('patternTransform') ?? '';
					return copy;
				})
			);
			slideTo(slid);
		};
		lay(of);
		return { update: lay };
	}
</script>

<svg class="now-strip" class:mirrored {width} {height} aria-hidden="true" bind:this={svg}>
	<defs use:patterns={doc}></defs>
	<g class="strip-slide" bind:this={flow}>
		<g bind:this={layer}></g>
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
</style>
