<!--
	The moment lesson's scene (da-nl1co, da-uzyih.4865): a café table seen
	at a tilt (da-uzyih.4873), a placemat, a plate and silverware on it. A
	croissant, a cake or a bagel appears on the plate, or a coffee or a tea
	beside it, one at a time at a gentle pace, each kind its own moment
	field; about a second and a half later a baked good is eaten in bites
	and a cup drunk, wiped out from top to bottom (vividn, 2026-10-27,
	2026-10-28). The things are
	Google's Noto emoji, vendored, standing upright on the table, a baked good
	on the plate and a cup right of the knife; the fork and knife are shapes
	lying on it (da-uzyih.4891). The plate and the silverware cast a soft
	shadow on the table at its tilt (da-uzyih.4896). Under
	`prefers-reduced-motion`, read as each round starts, it simply goes. All
	of it is drawn from the game's own clock, `t`, so a held clock holds it.
	Played in the game box (`GameBox.svelte`, da-uzyih.4864) and logged on
	the collect widget in the pane under it, filtered to the café's fields (da-2s9im): a tap logs `moment
	coffee` and its like, and so does the bar's line.

	A tap is a `press` like any widget's, in its handler before any await,
	so the CLI's clock is frozen at the tap (da-szzgy) and the bottom bar
	shows the command it ran. When the game ends the record — the entries
	the taps logged, read back from the database — is matched to what the
	scene says really appeared (`#lib/cafe.ts`), and the box shows two
	numbers: the accuracy, the things logged over the things that appeared
	and the taps that matched none, and the mean reaction time, from a
	thing appearing to its tap.
-->
<script lang="ts">
	import { onMount } from 'svelte';
	import { tailArgv } from '#lib/calls.ts';
	import { run } from '#lib/run.ts';
	import { isStatusState } from '#lib/fields.ts';
	import { percent, type Outcome } from '#lib/tutorial.ts';
	import { GOING_MS, KINDS, MATCH_MS, STAND_MS, round, scene, score, type Item, type Tap } from '#lib/cafe.ts';
	import GameBox from './GameBox.svelte';
	import { roundOf } from './round.js';

	let { fields }: { fields: string[] } = $props();

	let items: Item[] = $state.raw([]);
	let duration = $state(0);
	let start = 0;
	/** The round as the collect pane's presses reach it: what is still landing. */
	const shared = roundOf('moment');
	onMount(() => shared.watch(fields));

	/** No bites and no wipe: a thing simply goes. */
	let still = false;
	const id = $props.id();
	/** The thing on the table `t` ms in, if any: `scene` sets one down only once the last is gone, so never two (or two masks). */
	const shown = (t: number): Item | undefined => items.find((item) => t >= item.at && t - item.at < STAND_MS + (still ? 0 : GOING_MS));
	/** How far gone it is: 0 standing, 1 gone. */
	const going = (item: Item, t: number): number => Math.min(1, Math.max(0, (t - item.at - STAND_MS) / GOING_MS));
	/**
	 * Where each kind's picture stands, x, y and size: a baked good centered on the plate, its base at y 75
	 * inside the rim, and a cup in the mat's far right corner beyond the knife, centered at x 148, its base at
	 * y 53 and smaller for being farther; each by what its emoji draws (its bounds in the 128 box).
	 */
	const PLACE: Record<string, number[]> = {
		croissant: [83, 42.9, 34],
		cake: [82.8, 42.2, 34],
		bagel: [83.1, 44.6, 34],
		coffee: [134, 25.9, 28],
		tea: [134.1, 25.7, 28]
	};
	/** The bites a baked good is eaten in, in turn: x, y, r, over its picture, a share of its size. */
	const BITES = [
		[0.95, 0.3, 0.3], // 1: the right edge
		[0.025, 0.65, 0.3], // 2: the left edge
		[0.525, 0.025, 0.3], // 3: the top
		[0.45, 0.95, 0.3], // 4: the bottom
		[0.5, 0.5, 0.75] // 5, the last: the rest of it
	];
	/** A bite's size, from none to whole, as the thing goes. */
	const bite = (k: number, gone: number): number => Math.min(1, Math.max(0, gone * BITES.length - k));
	const glyph = (field: string): string => KINDS.find((k) => k.field === field)?.glyph ?? '';
	/** Google's Noto emoji, vendored (`static/noto-emoji`), so every device draws the same picture. */
	const emoji = (g: string): string => `/noto-emoji/emoji_u${g.codePointAt(0)?.toString(16)}.svg`;
	/**
	 * An outline lying on the table, its points `[x, d]` — across, and into the table from its near end — with
	 * that end at `x0`, `d0` in from the mat's near edge, in the near edge's units: it recedes toward (100, −180),
	 * as the mat's sides do, so it narrows and shortens as the mat does (the mat's far edge is 132 in). 661 is
	 * the plate's own tilt: a circle drawn 12.3 deep for 32.5 wide at y 70, so the mat lies as the plate does.
	 */
	const lying = (outline: number[][], x0: number, d0: number): string =>
		outline
			.map(([x, d]) => {
				const k = 661 / (661 + d0 + d);
				return `${(100 + (x0 + x - 100) * k).toFixed(1)},${(-180 + 270 * k).toFixed(1)}`;
			})
			.join(' ');
	/** A fork, tines to the far end. */
	const FORK = [
		[-1.4, 0], [-2.4, 3.6], [-1, 32.4], [-3.2, 38.4], [-3.2, 54], [-2.2, 54], [-2.2, 44.4], [-1.4, 44.4], [-1.4, 54], [-0.4, 54], [-0.4, 44.4],
		[0.4, 44.4], [0.4, 54], [1.4, 54], [1.4, 44.4], [2.2, 44.4], [2.2, 54], [3.2, 54], [3.2, 38.4], [1, 32.4], [2.4, 3.6], [1.4, 0]
	];
	/** A knife, its blade to the far end and its edge to the left, toward the plate. */
	const KNIFE = [
		[-1.2, 0], [-1.9, 2.4], [-1.6, 24], [-2.2, 25.2], [-2, 52.8], [-1, 58.8], [0.4, 60.6], [1.3, 58.8], [1.4, 25.2], [1.6, 24], [1.9, 2.4], [1.2, 0]
	];
	function begin(at: number): void {
		({ items, duration } = scene(round.seed ?? Math.floor(Math.random() * 2 ** 32)));
		still = matchMedia('(prefers-reduced-motion: reduce)').matches;
		start = at;
		shared.begin();
	}

	/**
	 * The round's record, from the database: every entry of the lesson's
	 * fields from its start to its end, beside the things that appeared
	 * before it ended. A tap outside a game logs as any tap does and is not
	 * scored.
	 */
	async function finish(elapsed: number): Promise<Outcome[]> {
		await shared.settled();
		const taps: Tap[] = [];
		for (const { field } of KINDS) {
			const listed = await run(tailArgv({ field, count: 200 }));
			if (listed.code !== 0) continue;
			for (const { time, state } of JSON.parse(listed.stdout).rows as { time: string | null; state: unknown }[])
				if (time && !isStatusState(state) && Date.parse(time) >= start && Date.parse(time) - start <= elapsed) taps.push({ field, at: Date.parse(time) - start });
		}
		// What appeared before it ended: a thing still in its time to be tapped counts if caught, and is no miss if not.
		const { items: appeared, extra } = score(
			items.filter((item) => item.at <= elapsed),
			taps
		);
		// Scored: each thing caught or missed. The mean is over the caught alone, a reaction time each.
		const scored = appeared.filter(({ item, tap }) => tap || item.at + MATCH_MS <= elapsed);
		const reactions = scored.flatMap(({ item, tap }) => (tap ? [tap.at - item.at] : []));
		const mean = reactions.reduce((a, b) => a + b, 0) / reactions.length;
		return [
			{ name: 'accuracy', value: percent(reactions.length, scored.length + extra.length) },
			{ name: 'mean reaction time', value: reactions.length ? `${(mean / 1000).toFixed(1)} s` : '—' }
		];
	}
</script>

{#snippet table(item?: Item, t = 0)}
	{@const gone = item ? going(item, t) : 0}
	{@const drink = item?.field === 'coffee' || item?.field === 'tea'}
	{@const [x, y, size] = item ? PLACE[item.field] : []}
	<svg class="cafe" viewBox="20 10 160 80" preserveAspectRatio="xMinYMid meet" role="img" aria-label="a café table">
		<rect class="wall" x="-500" y="-500" width="1200" height="524" />
		<rect class="wood" x="-500" y="24" width="1200" height="600" />
		<polygon class="mat" points="40,45 160,45 172,90 28,90" />
		<!-- Each shadow is its shape a little right and nearer, on the table's plane, softened. -->
		<filter id="{id}-soft" filterUnits="userSpaceOnUse" x="0" y="0" width="200" height="100"><feGaussianBlur stdDeviation="0.6" /></filter>
		<ellipse class="shadow" filter="url(#{id}-soft)" cx="101" cy="71.5" rx="32.5" ry="12.3" />
		<ellipse class="plate" cx="100" cy="70" rx="32.5" ry="12.3" />
		<ellipse class="rim" cx="100" cy="70" rx="24.4" ry="8.7" />
		<!-- Lying on the table, so foreshortened as the table is; the things on it stand. -->
		<polygon class="shadow" filter="url(#{id}-soft)" points={lying(FORK, 56, 23.5)} />
		<polygon class="shadow" filter="url(#{id}-soft)" points={lying(KNIFE, 146, 23.5)} />
		<polygon class="silverware fork" points={lying(FORK, 55, 25)} />
		<polygon class="silverware knife" points={lying(KNIFE, 145, 25)} />
		{#if item}
			<mask id="{id}-going" maskUnits="userSpaceOnUse">
				<rect width="200" height="100" fill="white" />
				{#if drink}
					<rect width="200" height={y + gone * size} />
				{:else}
					{#each BITES as [bx, by, r], k}<circle cx={x + bx * size} cy={y + by * size} r={r * size * bite(k, gone)} />{/each}
				{/if}
			</mask>
			<g class="thing" data-field={item.field} mask="url(#{id}-going)">
				<image href={emoji(glyph(item.field))} {x} {y} width={size} height={size} />
			</g>
		{/if}
	</svg>
{/snippet}

<div class="lesson">
	<GameBox {duration} {fields} onstart={begin} onend={finish} onreset={() => shared.settled()}>
		{#snippet neutral()}
			{@render table()}
		{/snippet}
		{#snippet game(t)}
			{@render table(shown(t), t)}
		{/snippet}
	</GameBox>
</div>

<style>
	/* A table seen at a tilt, mixed from the page's own colors so it sits in any of them. Its wall and wood run on
	   past the picture to the stage's edges, so Start stands on the table, right of the mat, where nothing appears
	   (vividn, 2026-10-30, da-uzyih.4917). As wide as its height draws it, for the stage to center. */
	.cafe {
		display: block;
		block-size: var(--stage);
		overflow: visible;
	}

	.wall {
		fill: color-mix(in srgb, var(--ink) 8%, var(--ground));
	}

	.wood {
		fill: color-mix(in srgb, #a0703f 45%, var(--ground));
	}

	.mat {
		fill: color-mix(in srgb, var(--accent) 22%, var(--ground));
	}

	.plate {
		fill: color-mix(in srgb, white 85%, var(--ground));
		stroke: color-mix(in srgb, white 55%, var(--ground));
	}

	.rim {
		fill: none;
		stroke: color-mix(in srgb, white 65%, var(--ground));
	}

	.silverware {
		fill: color-mix(in srgb, white 70%, var(--ground));
		stroke: color-mix(in srgb, white 40%, var(--ground));
		stroke-width: 0.3;
		stroke-linejoin: round;
	}

	/* The page's own ground, seen through: the table darkened where the light does not reach. */
	.shadow {
		fill: color-mix(in srgb, var(--ground) 45%, transparent);
	}

</style>
