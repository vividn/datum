<!--
	The switch lesson's scene (da-uzyih.4867): a house seen from above, and a
	person, a dot, wandering from room to room and out into the garden, and
	about each room's furniture while there (`#lib/house.ts`), played in the game box
	(`GameBox.svelte`, da-uzyih.4864). Where they are is one switch field,
	`room`, logged through the fields widget itself
	(`FieldsWidget.svelte`, the collect tab's, restricted to it): a tap on the
	field opens its states, a tap on a room switches it.

	The game starts itself (vividn, 2026-10-27 14:18: «to start the game, the
	user should change the state to be where the person is»): at rest the dot
	stands in a room, the box says in Start's place that switching
	`room` there starts it, and the game begins once a tap has
	switched it there. The room at rest is never the one already held, so it
	is a switch. The cue and the start it promises are the rest's alone: on
	the game-over screen a switch starts nothing, until Reset.

	A tap is a `press` like any widget's, in its handler before any await,
	so the CLI's clock is frozen at the tap (da-szzgy) and the bottom bar
	shows the command it ran. When the game ends the box shows its
	accuracy: the share of the round the record — the entries the taps
	logged, read back from the database, the one that started it included —
	held the place the dot was.
-->
<script lang="ts">
	import { tailArgv } from '#lib/calls.ts';
	import { run } from '#lib/run.ts';
	import { isStatus, percent, type Outcome } from '#lib/tutorial.ts';
	import { FIELD, PLAN, ROOMS, ROUND_MS, WALLS, accuracy, dotAt, round, scene, stateAt, type Change, type Tap } from '#lib/house.ts';
	import GameBox from './GameBox.svelte';
	import { get } from 'svelte/store';
	import { fields as buttons } from './engine.js';
	import { roundOf } from './round.js';

	let { fields }: { fields: string[] } = $props();

	let box: GameBox;
	const motion = typeof matchMedia !== 'function' || !matchMedia('(prefers-reduced-motion: reduce)').matches;
	const roomButtons = $derived($buttons.filter((f) => f.field === FIELD));
	/** Each room's state color, the spec's as `fields --json` resolves it: its name and furniture are drawn in it. Until the field's button is read, `--muted`. */
	const colors = $derived(roomButtons[0]?.stateColors ?? {});

	/** The next round's walk, its first room the one the dot stands in at rest: never the room the field holds now. */
	function fresh(): Change[] {
		const held = roomButtons[0]?.current;
		return scene(round.seed ?? Math.floor(Math.random() * 2 ** 32), typeof held === 'string' ? held : undefined);
	}
	let next: Change[] = $state.raw(fresh());
	let changes: Change[] = $state.raw([]);
	let start = 0;
	/** The round as the fields' pane shares it: its taps and failures (LessonFields.svelte). */
	const shared = roundOf('switch');
	const { phase } = shared;
	/** At rest, a switch to the dot's room starts the game. */
	shared.wrote = (argv) => {
		if (argv[0] === 'switch' && argv[1] === FIELD && argv[2] === next[0].state && get(phase) === 'neutral') box.begin();
	};

	function begin(at: number): void {
		changes = next;
		start = at;
		shared.begin();
	}

	/** The round's record, from the database: the switch that started it, at 0, and every one after. */
	async function finish(elapsed: number): Promise<Outcome[]> {
		const played = changes;
		// Once the round's last writes have landed, so the next room is never the one they left held.
		await shared.settled();
		next = fresh();
		const listed = await run(tailArgv({ field: FIELD, count: 200 }));
		const rows: { time: string | null; state: unknown }[] = listed.code === 0 ? JSON.parse(listed.stdout).rows.filter((row: { state: unknown }) => !isStatus(row.state)) : [];
		const taps: Tap[] = rows
			.map(({ time, state }) => ({ state: String(state), at: time ? Date.parse(time) - start : NaN }))
			.filter((tap) => tap.at <= elapsed)
			.sort((a, b) => a.at - b.at);
		// The last switch at or before the start is the room the round began on.
		const first = taps.findLastIndex((tap) => tap.at <= 0);
		const record = first < 0 ? taps : [{ ...taps[first], at: 0 }, ...taps.slice(first + 1)];
		return [{ name: 'accuracy', value: percent(accuracy(played, record, elapsed)) }];
	}

	function reset(): Promise<unknown> {
		next = fresh();
		return shared.settled();
	}
</script>

{#snippet plan(walk: Change[], t: number)}
	{@const [x, y] = dotAt(walk, t, motion)}
	<svg class="house" viewBox="0 0 {PLAN.width} {PLAN.height}" preserveAspectRatio="xMinYMid meet" role="img" aria-label="the house, and where the person is" data-room={stateAt(walk, t)}>
		{#each ROOMS as r (r.state)}
			<rect class={r.state === 'garden' ? 'garden' : 'floor'} x={r.x} y={r.y} width={r.width} height={r.height} style:--room={colors[r.state]} />
			<text x={r.x + 6} y={r.y + 14} data-room={r.state} style:--room={colors[r.state]}>{r.state}</text>
		{/each}
		{#each ROOMS as r (r.state)}
			{#each r.things as { name, shapes, colors: fills } (name)}
				<g class="thing" data-room={r.state} data-thing={name} style:--room={colors[r.state]}>
					{#each shapes as s, i (i)}
						{#if s.length === 3}<circle cx={s[0]} cy={s[1]} r={s[2]} style:fill={fills?.[i]} style:stroke={fills && 'none'} />{:else}<rect x={s[0]} y={s[1]} width={s[2]} height={s[3]} rx={s[4]} />{/if}
					{/each}
				</g>
			{/each}
		{/each}
		<path class="walls" d={WALLS} />
		<circle class="dot" cx={x} cy={y} r="6" />
	</svg>
{/snippet}

{#snippet cue()}
	<p class="cue">Switch <b>{FIELD}</b> to <b>{next[0].state}</b> to start ↓</p>
{/snippet}

<div class="lesson">
	<GameBox bind:this={box} duration={ROUND_MS} {fields} bind:phase={$phase} onstart={begin} onend={finish} onreset={reset} {cue}>
		{#snippet neutral()}{@render plan(next, 0)}{/snippet}
		{#snippet game(t)}{@render plan(changes, t)}{/snippet}
	</GameBox>
</div>

<style>
	/* The cue's column: «living_room» on a line of its own. */
	.lesson {
		--controls: 6.5rem;
	}

	/* The whole of the box's height, the cue wrapping on its right (vividn, 2026-10-30, da-uzyih.4917), and as wide as
	   that draws it, for the stage to center. */
	.house {
		display: block;
		block-size: var(--stage);
	}

	.floor {
		fill: var(--raised);
	}

	.garden {
		fill: color-mix(in srgb, var(--room, var(--muted)) 22%, transparent);
	}

	.walls {
		fill: none;
		stroke: var(--ink);
		stroke-width: 3;
		stroke-linecap: square;
	}

	/* Furniture, seen from above: a shape a piece, under the walls' ink, in its room's color. */
	.thing {
		fill: color-mix(in srgb, var(--room, var(--muted)) 40%, transparent);
		stroke: var(--room, var(--muted));
		stroke-width: 1;
	}

	text {
		fill: var(--room, var(--muted));
		font-size: 11px;
	}

	.dot {
		fill: var(--accent-bright);
		stroke: var(--ink-bright);
		stroke-width: 1.5;
	}

	.cue {
		margin: 0;
		font-size: 0.85rem;
		line-height: 1.25;
		color: var(--ink-bright);
	}
</style>
