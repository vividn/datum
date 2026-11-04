<!--
	«On and off» (da-rru2t): a book, a TV and a phone (da-uzyih.4866) that go
	on and off on a script, overlapping, played in the game box
	(`GameBox.svelte`, da-uzyih.4864). At rest the book is closed and the TV
	and the phone are off. Each thing is its field's button in the fields
	widget itself (`FieldsWidget.svelte`, the collect tab's, restricted to
	the three), so logging here is logging anywhere: a tap starts the field,
	the next ends it, at the tap's own instant (`press` freezes the CLI's
	clock at the event, da-szzgy); `reading` is on while the book is
	open. When the game ends the box shows its accuracy: the share of the
	round each thing's record, read back from the database, agreed with it,
	on or off, over the three. The nowview over it on the tab draws the
	blocks as they happen.
-->
<script lang="ts">
	import { agreement, blocksOf, DEN, isOn, type Thing } from '#lib/den.ts';
	import { tailListing } from '#lib/tail.ts';
	import { percent, type Outcome } from '#lib/tutorial.ts';
	import GameBox from './GameBox.svelte';
	import { roundOf } from './round.js';

	let { fields }: { fields: string[] } = $props();

	/** The round as the fields' pane shares it: its taps and failures (LessonFields.svelte). */
	const shared = roundOf('on-off');
	/** Read at each start: a test shortens the round. */
	let duration = $state(DEN.length * 1000);
	/** The round's start, on the wall. */
	let wall0 = 0;

	function begin(at: number): void {
		duration = DEN.length * 1000;
		wall0 = at;
		shared.begin();
	}

	async function finish(elapsed: number): Promise<Outcome[]> {
		await shared.settled();
		const end = wall0 + elapsed;
		const shares = await Promise.all(
			DEN.things.map(async (thing) => {
				const yours = blocksOf((await tailListing({ field: thing.field, count: 200 })).rows, wall0);
				const truth = thing.on.map(([a, b]) => ({ from: wall0 + a * 1000, to: wall0 + b * 1000 }));
				return agreement(truth, yours, wall0, end);
			})
		);
		return [{ name: 'accuracy', value: percent(shares.reduce((a, b) => a + b, 0), shares.length) }];
	}
</script>

<!-- Each thing as a picture, its on state plain at a glance: the book open, a screen lit. -->
{#snippet picture(thing: Thing, on: boolean)}
	<svg viewBox="0 0 64 48" aria-hidden="true">
		{#if thing.field === 'reading'}
			{#if on}
				<rect x="2" y="7" width="60" height="38" rx="2" fill="#7950f2" />
				<rect x="5" y="4" width="26.5" height="38" fill="#f8f9fa" />
				<rect x="32.5" y="4" width="26.5" height="38" fill="#f8f9fa" />
				<path d="M9 11h19M9 16h19M9 21h15M9 26h19M9 31h12M36 11h19M36 16h19M36 21h19M36 26h14" stroke="#adb5bd" stroke-width="2" />
			{:else}
				<rect x="19" y="3" width="28" height="42" rx="2" fill="#7950f2" />
				<rect x="19" y="3" width="5" height="42" rx="1" fill="#5f3dc4" />
				<rect x="28" y="12" width="14" height="5" fill="#e5dbff" />
			{/if}
		{:else if thing.field === 'tv'}
			<rect x="3" y="2" width="58" height="36" rx="3" fill="#343a40" />
			<rect x="7" y="6" width="50" height="28" fill={on ? '#a5d8ff' : '#111'} />
			{#if on}
				<circle cx="44" cy="14" r="4" fill="#fab005" />
				<path d="M7 34l14-14 9 8 10-10 17 16z" fill="#1c7ed6" />
			{/if}
			<path d="M32 38v6M22 45h20" stroke="currentColor" stroke-width="3" />
		{:else}
			<rect x="20" y="1" width="24" height="46" rx="4" fill="#343a40" />
			<rect x="23" y="5" width="18" height="36" rx="1" fill={on ? '#fff4e6' : '#111'} />
			{#if on}
				{#each [0, 1, 2] as row (row)}
					{#each [0, 1] as col (col)}
						<rect x={26 + col * 7} y={9 + row * 9} width="5" height="5" rx="1" fill="#e8590c" />
					{/each}
				{/each}
			{/if}
		{/if}
	</svg>
{/snippet}

{#snippet den(t: number | null)}
	<div class="den">
		{#each DEN.things as thing (thing.field)}
			{@const on = t !== null && isOn(thing, t)}
			<figure class="thing" class:on data-field={thing.field} data-on={on} role="img" aria-label="the {thing.name}, {thing.field === 'reading' ? (on ? 'open' : 'closed') : on ? 'on' : 'off'}">
				{@render picture(thing, on)}
				<figcaption>{thing.field}</figcaption>
			</figure>
		{/each}
	</div>
{/snippet}

<div class="lesson">
	<GameBox {duration} {fields} onstart={begin} onend={finish} onreset={() => shared.settled()}>
		{#snippet neutral()}{@render den(null)}{/snippet}
		{#snippet game(t)}{@render den(t / 1000)}{/snippet}
	</GameBox>
</div>

<style>
	/* Three things at their most, for the stage to center (da-qx1it), narrowing together when it has less. */
	.den {
		display: flex;
		gap: 0.5rem;
		inline-size: 22rem;
	}

	.thing {
		flex: 0 1 7rem;
		min-inline-size: 0;
		display: flex;
		flex-direction: column;
		align-items: center;
		gap: 0.3rem;
		margin: 0;
		color: var(--ink);
	}

	.thing svg {
		inline-size: 100%;
		/* Under it, its name, where the pane has the room. */
		max-block-size: calc(var(--stage) - var(--caption, 1.4rem));
	}

	/* A short pane draws the pictures alone: the fields' buttons under them name them. */
	@container (block-size < 10rem) {
		.den {
			--caption: 0rem;
		}

		figcaption {
			display: none;
		}
	}

	figcaption {
		font-size: 0.8rem;
	}

	.thing.on figcaption {
		color: var(--ink-bright);
	}
</style>
