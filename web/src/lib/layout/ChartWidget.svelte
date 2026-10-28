<!--
	The chart widget (da-xv7ti): one key of some fields' entries over time.

	A number key draws each field as a series in its own colour — the
	entries as points, and their mean a bucket as a line — on one plot, so
	the weight of apples, pears and oranges reads side by side. Any other
	key draws its values counted a bucket, stacked. Which fields, which key,
	the bucket, the window and points or means are the widget's options, in
	the menu its name opens; the fields and the keys are what this widget
	OFFERS that menu (`offers`, options.ts), since only it has read them.

	Every row is `datum tail F --json`'s, a field read page by page back to
	the window's start (`readField`), again on every beat; `#lib/chart.ts`
	arranges them.
-->
<script lang="ts">
	import { untrack } from 'svelte';
	import { browser } from '$app/env';
	import Pane from '#lib/Pane.svelte';
	import { run, runKey } from '#lib/run.ts';
	import {
		DEFAULT_CHART,
		FIELDS_OFFER,
		chartOf,
		dayLabel,
		keysOf,
		keysOffer,
		nowX,
		readField,
		startOf,
		type ChartSettings,
		type Row
	} from '#lib/chart.ts';
	import { fields } from './engine.js';
	import { offers } from './options.js';

	let {
		settings = DEFAULT_CHART,
		epoch = 0
	}: {
		settings?: ChartSettings;
		epoch?: number;
	} = $props();

	let rows = $state.raw<Row[]>([]);
	let started = 0;
	let shown = -1;
	const keys = new Map<string, string>();
	const keyOf = (field: string) => keys.get(field) ?? keys.set(field, runKey()).get(field)!;

	$effect.pre(() => {
		const asked = [settings.fields, settings.range, epoch] as const;
		if (browser) untrack(() => void load(...asked));
	});

	async function load(asked: string[], range: string, _epoch: number): Promise<void> {
		const at = ++started;
		const read = await Promise.all(
			asked.map((field) =>
				readField(field, startOf(range, nowX()), async (argv) => {
					const r = await run(argv, undefined, undefined, keyOf(field), undefined, undefined, true);
					try {
						return r.code === 0 ? ((JSON.parse(r.stdout).rows ?? []) as Row[]) : null;
					} catch {
						return null;
					}
				})
			)
		);
		// Last started wins, not last to return (see CliPane.svelte).
		if (at < shown) return;
		shown = at;
		rows = read.flat();
	}

	$effect.pre(() => {
		const offered = {
			[FIELDS_OFFER]: $fields.map(({ field }) => ({ value: field, label: field })),
			[keysOffer(settings.fields)]: [
				{ value: '', label: 'first number key' },
				...keysOf(rows).map((k) => ({ value: k, label: k }))
			]
		};
		untrack(() => offers.update((o) => ({ ...o, ...offered })));
	});

	const colors = $derived(Object.fromEntries($fields.map((f) => [f.field, f.color])));
	const stateColors = $derived(Object.fromEntries($fields.map((f) => [f.field, f.stateColors])));
	const chart = $derived(chartOf(rows, settings, nowX(), colors, stateColors));

	let width = $state(0);
	let height = $state(0);
	const PAD = { left: 34, right: 6, top: 6, bottom: 16 };
	const plotW = $derived(Math.max(0, width - PAD.left - PAD.right));
	const plotH = $derived(Math.max(0, height - PAD.top - PAD.bottom));
	const sx = $derived((x: number) => PAD.left + ((x - chart.from) / (chart.to - chart.from)) * plotW);
	const [lo, hi] = $derived(
		chart.kind === 'numeric'
			? chart.min === chart.max
				? [chart.min - 1, chart.max + 1]
				: [chart.min, chart.max]
			: [0, chart.kind === 'categorical' ? chart.max : 1]
	);
	const sy = $derived((y: number) => PAD.top + (1 - (y - lo) / (hi - lo)) * plotH);
	const ticks = $derived(chart.kind === 'categorical' ? [0, hi] : [lo, (lo + hi) / 2, hi]);
	const fmt = (v: number) => String(Number(v.toPrecision(3)));
	const points = $derived(settings.show !== 'average');
	const means = $derived(settings.show !== 'points');
	const legend = $derived(
		chart.kind === 'numeric'
			? chart.series.map(({ name, color }) => ({ name, color }))
			: chart.kind === 'categorical'
				? chart.names
				: []
	);
	const what = $derived(chart.kind === 'categorical' ? 'count' : means ? 'mean' : 'each');
</script>

<Pane title="chart">
	<div class="chart" data-kind={chart.kind}>
		{#if settings.fields.length === 0}
			<p class="quiet">Choose fields from this widget's menu: tap its name.</p>
		{:else}
			<p class="head">
				<b>{chart.key}</b> · {what}{what === 'each' ? '' : ` per ${settings.bucket}`} · {settings.fields.join(', ')}
			</p>
			{#if legend.length}
				<ul class="legend">
					{#each legend as { name, color } (name)}
						<li data-name={name}><i style:background={color}></i>{name}</li>
					{/each}
				</ul>
			{/if}
			<div class="plot" bind:clientWidth={width} bind:clientHeight={height}>
				{#if chart.kind === 'empty'}
					<p class="quiet">No “{chart.key}” in this window.</p>
				{:else if width > 0 && height > 0}
					<svg {width} {height} role="img" aria-label="{chart.key} over time">
						{#each ticks as t (t)}
							<line class="grid" x1={PAD.left} x2={width - PAD.right} y1={sy(t)} y2={sy(t)} />
							<text class="tick" x={PAD.left - 3} y={sy(t)} text-anchor="end" dominant-baseline="middle"
								>{fmt(t)}</text
							>
						{/each}
						<text class="tick" x={PAD.left} y={height - 3}>{dayLabel(chart.from)}</text>
						<text class="tick" x={width - PAD.right} y={height - 3} text-anchor="end">{dayLabel(chart.to - 1)}</text>
						{#if chart.kind === 'numeric'}
							{#each chart.series as s (s.name)}
								<g class="series" data-series={s.name} data-points={s.points.length} data-means={s.means.length}>
									{#if means && s.means.length}
										<polyline
											class="mean"
											stroke={s.color}
											points={s.means.map((m) => `${sx(m.x)},${sy(m.y)}`).join(' ')}
										/>
										{#each s.means as m (m.x)}
											<rect class="mean" fill={s.color} x={sx(m.x) - 3} y={sy(m.y) - 3} width="6" height="6"
												><title>{s.name} mean {fmt(m.y)}</title></rect
											>
										{/each}
									{/if}
									{#if points}
										{#each s.points as p, i (i)}
											<circle class="point" fill={s.color} cx={sx(p.x)} cy={sy(p.y)} r="2.5"
												><title>{s.name} {fmt(p.y)}</title></circle
											>
										{/each}
									{/if}
								</g>
							{/each}
						{:else}
							{#each chart.stacks as stack (stack.x)}
								<g class="stack" data-field={stack.field}>
									{#each stack.parts as part, i (part.name)}
										{@const below = stack.parts.slice(0, i).reduce((t, p) => t + p.count, 0)}
										<rect
											data-name={part.name}
											data-count={part.count}
											fill={part.color}
											x={sx(stack.x) + 0.5}
											width={Math.max(1, sx(stack.x + stack.width) - sx(stack.x) - 1)}
											y={sy(below + part.count)}
											height={sy(below) - sy(below + part.count)}><title>{stack.field} {part.name}: {part.count}</title></rect
										>
									{/each}
								</g>
							{/each}
						{/if}
					</svg>
				{/if}
			</div>
		{/if}
	</div>
</Pane>

<style>
	.chart {
		display: flex;
		flex-direction: column;
		height: 100%;
		min-height: 0;
		gap: 0.25rem;
	}
	.head {
		margin: 0;
		color: var(--muted);
		overflow-wrap: anywhere;
	}
	.head b {
		color: var(--ink);
	}
	.legend {
		display: flex;
		flex-wrap: wrap;
		gap: 0.1rem 0.75rem;
		margin: 0;
		padding: 0;
		list-style: none;
	}
	.legend i {
		display: inline-block;
		width: 0.7em;
		height: 0.7em;
		margin-right: 0.3em;
		border-radius: 2px;
	}
	.plot {
		flex: 1;
		min-height: 3rem;
		position: relative;
	}
	svg {
		position: absolute;
		inset: 0;
		display: block;
	}
	.grid {
		stroke: var(--rule);
	}
	.tick {
		fill: var(--muted);
		font-size: 0.7rem;
	}
	polyline.mean {
		fill: none;
		stroke-width: 2;
	}
	.point {
		opacity: 0.75;
	}
	.quiet {
		color: var(--muted);
		margin: 0;
	}
</style>
