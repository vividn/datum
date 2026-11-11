<!--
	A field's labels (da-cepjy), vividn, 2026-11-11: «let's change the
	above/below to be a button (+label) / you can add multiple labels / each
	label has a dropdown for things it can labeled with, in addition to keys,
	also allow labelling with field, at, state / let's also allow labelling
	blocky data / each label also shows a little single moment dot (or bar),
	which let's you drag an example label (use sane example data depending on
	the label selected) around, snapping to a few predefined positions».

	Each row is one of the spec's `labels`: what it shows, and a sample mark,
	one dot or one bar, with the label on it in sample text. A drag of the
	label snaps it to the nearest position; the arrow keys step through them.
	The positions are datum's (`LabelAt`), spelt relative to time, and laid
	out here as datum lays them out on a horizontal day, from the same offsets
	in ems (`push_labels`, `push_bar_labels`), so the sample is the picture.

	Sample text fits the choice: the newest value of the key among the field's
	entries that draw a mark, else the key's name; `at` the newest such entry's time, else 09:15; a
	`state` of the field's; `field` its name. Cut to ten characters, as datum
	cuts a label (`LABEL_MAX`).
-->
<script lang="ts">
	import { run } from './run.js';
	import { tailArgv } from './calls.js';
	import type { Label } from './track.js';
	import type { TailListing } from './tail.js';

	let {
		labels,
		onchange,
		field,
		bar = false,
		keys = [],
		states = [],
		color = null
	}: {
		labels: Label[];
		onchange: (labels: Label[]) => void;
		field: string;
		/** Whether the field's marks are bars: an on/off or a change. */
		bar?: boolean;
		/** The field's keys as the form holds them. */
		keys?: string[];
		/** The field's states, which a `state` label's sample shows the first of. */
		states?: string[];
		color?: string | null;
	} = $props();

	/** What a label shows besides a key. */
	const OWN: readonly string[] = ['field', 'at', 'state'];

	/** The sample's box, and its mark: a dot at the middle, or a bar across it. */
	const W = 240;
	const H = 64;
	const MID = { x: W / 2, y: H / 2 };
	const DOT_R = 6;
	const BAR = { x0: 70, x1: 170, y0: 24, y1: 40 };

	type Spot = { at: string; x: number; y: number; anchor: 'start' | 'middle' | 'end' };
	/** A dot's positions, at twice its radius, as datum's `push_labels` places them on a horizontal day. */
	const DOT_FS = DOT_R * 2;
	const DOT_SPOTS: Spot[] = [
		{ at: 'over', x: MID.x, y: MID.y - 0.65 * DOT_FS, anchor: 'middle' },
		{ at: 'under', x: MID.x, y: MID.y + 1.45 * DOT_FS, anchor: 'middle' },
		{ at: 'before', x: MID.x - 0.65 * DOT_FS, y: MID.y + 0.35 * DOT_FS, anchor: 'end' },
		{ at: 'after', x: MID.x + 0.65 * DOT_FS, y: MID.y + 0.35 * DOT_FS, anchor: 'start' }
	];
	/** A bar's ten, as `push_bar_labels` places them: over and under at its start, middle and end; inside and outside each end. */
	const BAR_FS = 11;
	const bmid = (BAR.y0 + BAR.y1) / 2 + 0.35 * BAR_FS;
	const BAR_SPOTS: Spot[] = [
		...(['over', 'under'] as const).flatMap((side) => {
			const y = side === 'over' ? BAR.y0 - 0.25 * BAR_FS : BAR.y1 + 0.95 * BAR_FS;
			return [
				{ at: `${side}-start`, x: BAR.x0, y, anchor: 'start' as const },
				{ at: `${side}-middle`, x: (BAR.x0 + BAR.x1) / 2, y, anchor: 'middle' as const },
				{ at: `${side}-end`, x: BAR.x1, y, anchor: 'end' as const }
			];
		}),
		{ at: 'outside-start', x: BAR.x0 - 0.3 * BAR_FS, y: bmid, anchor: 'end' },
		{ at: 'inside-start', x: BAR.x0 + 0.3 * BAR_FS, y: bmid, anchor: 'start' },
		{ at: 'inside-end', x: BAR.x1 - 0.3 * BAR_FS, y: bmid, anchor: 'end' },
		{ at: 'outside-end', x: BAR.x1 + 0.3 * BAR_FS, y: bmid, anchor: 'start' }
	];
	const spots = $derived(bar ? BAR_SPOTS : DOT_SPOTS);
	const fs = $derived(bar ? BAR_FS : DOT_FS);
	/** A position of the other mark's, as datum reads it (`LabelAt::on_dot`, `on_bar`). */
	const ON_DOT: Record<string, string> = { 'over-start': 'over', 'over-middle': 'over', 'over-end': 'over', 'under-start': 'under', 'under-middle': 'under', 'under-end': 'under', 'outside-start': 'before', 'inside-start': 'before', 'inside-end': 'after', 'outside-end': 'after' };
	const ON_BAR: Record<string, string> = { over: 'over-middle', under: 'under-middle', before: 'outside-start', after: 'outside-end' };
	const spotOf = (at: string) => {
		const own = bar ? (ON_BAR[at] ?? at) : (ON_DOT[at] ?? at);
		return spots.find((s) => s.at === own) ?? spots[0];
	};

	/** The field's newest entries, for real sample values. */
	let rows: TailListing['rows'] = $state.raw([]);
	$effect(() => {
		const name = field;
		rows = [];
		if (!name) return;
		void run(tailArgv({ field: name, count: 20 })).then((r) => {
			if (r.code !== 0 || name !== field) return;
			// A TRACK or IGNORED row draws no mark, so it gives no sample.
			const marked = (row: TailListing['rows'][number]) => row.field === name && !['TRACKED', 'IGNORED'].includes(row.state_label ?? '');
			rows = (JSON.parse(r.stdout) as TailListing).rows.filter(marked).reverse();
		});
	});
	const shown = (v: unknown) => (Array.isArray(v) ? v.join(',') : v === null || v === undefined || typeof v === 'object' ? '' : String(v));
	const cut = (t: string) => ([...t].length > 10 ? [...t].slice(0, 9).join('') + '…' : t);
	function sample(show: string): string {
		if (show === 'field') return cut(field || 'field');
		// The clock the row was logged by, as datum prints a label's `at`.
		if (show === 'at') return rows.map((r) => /T(\d\d:\d\d)/.exec(r.time ?? '')?.[1]).find(Boolean) ?? '09:15';
		if (show === 'state') return cut(states[0] ?? (rows.map((r) => r.state_label).find(Boolean) || (bar ? 'on' : 'true')));
		const real = rows.map((r) => shown(r.data?.[show])).find(Boolean);
		return cut(real ?? show);
	}

	const set = (i: number, label: Partial<Label>) => onchange(labels.map((l, j) => (j === i ? { ...l, ...label } : l)));
	const add = () => onchange([...labels, { show: keys[0] ?? 'field', at: spots[0].at }]);
	const remove = (i: number) => onchange(labels.filter((_, j) => j !== i));

	/** The label being dragged: the position it snaps to so far, and where it was taken, from its text's middle. */
	let dragging: { i: number; at: string; dx: number; dy: number } | null = $state(null);
	/** The middle of `text` at spot `s`, in the sample's own units: where its snap dot is drawn. */
	function mid(s: Spot, text: string): [number, number] {
		const w = [...text].length * 0.6 * fs;
		return [s.anchor === 'start' ? s.x + w / 2 : s.anchor === 'end' ? s.x - w / 2 : s.x, s.y - 0.35 * fs];
	}
	/** The spot whose text's middle is nearest a point. */
	function nearest(x: number, y: number, text: string): string {
		const far = (s: Spot) => Math.hypot(mid(s, text)[0] - x, mid(s, text)[1] - y);
		return spots.reduce((best, s) => (far(s) < far(best) ? s : best)).at;
	}
	/** A pointer's place in the sample's own units. */
	function local(e: PointerEvent): [number, number] {
		const r = (e.currentTarget as Element).closest('svg')!.getBoundingClientRect();
		return [((e.clientX - r.left) / r.width) * W, ((e.clientY - r.top) / r.height) * H];
	}
	function grab(i: number, e: PointerEvent) {
		e.preventDefault();
		(e.currentTarget as Element).setPointerCapture?.(e.pointerId);
		const spot = spotOf(labels[i].at);
		const [x, y] = local(e);
		const [mx, my] = mid(spot, sample(labels[i].show));
		dragging = { i, at: spot.at, dx: x - mx, dy: y - my };
	}
	function move(i: number, e: PointerEvent) {
		if (dragging?.i !== i) return;
		const [x, y] = local(e);
		dragging = { ...dragging, at: nearest(x - dragging.dx, y - dragging.dy, sample(labels[i].show)) };
	}
	function drop(i: number) {
		if (dragging?.i !== i) return;
		const at = dragging.at;
		dragging = null;
		if (at !== labels[i].at) set(i, { at });
	}
	/** The arrow keys step the label through the positions, in their order. */
	function step(i: number, e: KeyboardEvent) {
		const by = { ArrowRight: 1, ArrowDown: 1, ArrowLeft: -1, ArrowUp: -1 }[e.key];
		if (!by) return;
		e.preventDefault();
		const k = spots.indexOf(spotOf(labels[i].at));
		set(i, { at: spots[(k + by + spots.length) % spots.length].at });
	}
</script>

<div class="mark-labels">
	{#each labels as label, i (i)}
		{@const spot = spotOf(dragging?.i === i ? dragging.at : label.at)}
		<div class="label-row" data-label={i}>
			<select aria-label="what label {i + 1} shows" data-show value={label.show} onchange={(e) => set(i, { show: e.currentTarget.value })}>
				<!-- A key the spec names that the form no longer lists stays offered, so nothing is dropped unseen. -->
				{#each [...new Set([...keys, ...(OWN.includes(label.show) || keys.includes(label.show) ? [] : [label.show])])] as key (key)}
					<option value={key}>{key}</option>
				{/each}
				{#each OWN as own (own)}
					<option value={own}>{own === 'at' ? 'at (its time)' : own}</option>
				{/each}
			</select>
			<svg class="sample" viewBox="0 0 {W} {H}" width={W} height={H} data-at={spot.at} role="group" aria-label="label {i + 1}'s place on a {bar ? 'bar' : 'dot'}">
				{#if bar}
					<rect class="mark" x={BAR.x0} y={BAR.y0} width={BAR.x1 - BAR.x0} height={BAR.y1 - BAR.y0} fill={color ?? 'var(--accent)'} />
				{:else}
					<circle class="mark" cx={MID.x} cy={MID.y} r={DOT_R} fill={color ?? 'var(--accent)'} />
				{/if}
				{#each spots as s (s.at)}
					{@const [cx, cy] = mid(s, sample(label.show))}
					<circle class="spot" class:near={dragging?.i === i && dragging.at === s.at} data-spot={s.at} {cx} {cy} r="2" />
				{/each}
				<text
					class="sample-label"
					class:inside={spot.at.startsWith('inside')}
					x={spot.x}
					y={spot.y}
					text-anchor={spot.anchor}
					font-size={fs}
					fill={spot.at.startsWith('inside') ? 'white' : (color ?? 'var(--accent)')}
					tabindex="0"
					role="slider"
					aria-label="label {i + 1}: drag it to a place"
					aria-valuetext={spot.at}
					aria-valuenow={spots.indexOf(spot)}
					aria-valuemin={0}
					aria-valuemax={spots.length - 1}
					onpointerdown={(e) => grab(i, e)}
					onpointermove={(e) => move(i, e)}
					onpointerup={() => drop(i)}
					onpointercancel={() => (dragging = null)}
					onkeydown={(e) => step(i, e)}>{sample(label.show)}</text
				>
			</svg>
			<button type="button" class="remove" aria-label="remove label {i + 1}" title="remove this label" onclick={() => remove(i)}>×</button>
		</div>
	{/each}
	<button type="button" class="add" onclick={add}>+ label</button>
</div>

<style>
	.mark-labels {
		display: flex;
		flex-direction: column;
		gap: 0.3rem;
		font-size: 0.75rem;
		color: var(--ink);
	}

	.label-row {
		display: flex;
		flex-wrap: wrap;
		align-items: center;
		gap: 0.4rem;
	}

	select,
	.add,
	.remove {
		min-block-size: 28px;
		background: var(--raised);
		border: 1px solid var(--rule);
		border-radius: 4px;
		color: var(--ink-bright);
		font: inherit;
	}

	.add {
		align-self: flex-start;
	}

	.remove {
		inline-size: 28px;
		padding: 0;
	}

	.sample {
		max-inline-size: 100%;
		block-size: auto;
		background: var(--ground, #000);
		border: 1px solid var(--rule);
		border-radius: 4px;
		touch-action: none;
	}

	.spot {
		fill: var(--quiet);
		pointer-events: none;
	}

	.spot.near {
		fill: var(--accent);
	}

	.sample-label {
		cursor: grab;
		user-select: none;
		paint-order: stroke;
		stroke: #000;
		stroke-width: 2;
	}

	.sample-label:focus-visible {
		outline: 1px solid var(--accent);
	}
</style>
