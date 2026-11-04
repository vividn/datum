<!--
	The box a tutorial game is played in (da-uzyih.4864), the moment, on/off
	and switch lessons'. vividn, 2026-10-27: «a separate box, where the
	things appear. It should have a good default screen that shows kind of
	the neutral state for each of the games».

	At rest it draws the lesson's `neutral` scene; while a game runs, its
	`game`, given the milliseconds since the start, in the middle of the
	stage, going left only as far as Start's column makes it (da-qx1it). On
	the right, Start:
	while the game runs the same button reads «End now», with the time gone
	filling it as a bar, and a press ends the game early, as running out of
	time does. The box then shows the game's one result, what `onend`
	returns (vividn, 14:18: «the game screen can just show accuracy as a
	single number»), and Start reads «Reset»: the neutral scene again, and
	every state of the lesson's `fields` active now ended, or IGNORED for a
	`never_false` field, which takes no end (`startOver` in tutorial.ts,
	da-uzyih.4880). It deletes nothing and asks nothing. There is no Undo
	and no Start over (da-uzyih.4910, vividn, 2026-10-29: «they are kind of
	useless and don't teach how to do datum»): a mis-tap is the bar's `undo`.

	The lesson's `fields` are TRACKED as a game begins and IGNORED once it
	ends, by its time or «End now», after its own writes have landed (`keep`
	in tutorial.ts, da-uzyih.4882), so Reset changes their status only by
	the IGNORED of a `never_false` field that holds a state. Each of these
	is shown, in the output and the CLI, as the command it is (`loudly`).

	A game that starts itself (the switch lesson's, da-uzyih.4867) gives a
	`cue`, drawn at rest in Start's place, and calls `begin` when its moment
	comes. The game-over screen draws no cue, only the result and Reset
	(da-uzyih.4880): the cue is back with the neutral scene.
-->
<script lang="ts">
	import { onDestroy, type Snippet } from 'svelte';
	import { act } from '#lib/act.ts';
	import { keep, startOver, type Outcome, type Phase } from '#lib/tutorial.ts';

	interface Props {
		/** How long a game lasts, in ms; read as it runs, so `onstart` may set it. */
		duration: number;
		/** The lesson's fields: TRACKED while a game runs, IGNORED after, their active states ended by Reset. */
		fields: string[];
		neutral: Snippet;
		game: Snippet<[number]>;
		/** A game begins, at this instant (`Date.now()`). */
		onstart: (at: number) => void;
		/** The game ended this many ms in: its result. */
		onend: (elapsed: number) => Promise<Outcome[]>;
		/** Before Reset ends the states: the lesson's writes settled, so none starts one after. */
		onreset?: () => Promise<unknown>;
		/** Where the box is, for a lesson whose controls differ at rest. */
		phase?: Phase;
		/** In Start's place at rest, for a game the lesson starts itself, by `begin`. */
		cue?: Snippet;
	}

	let { duration, fields, neutral, game, onstart, onend, onreset, phase = $bindable('neutral'), cue }: Props = $props();
	let start = 0;
	let elapsed = $state(0);
	let outcome: Outcome[] = $state.raw([]);
	let busy = $state(false);
	let failed = $state('');
	let frame = 0;
	/** The game's fields TRACKED, which their IGNORED waits for; the box is busy until they land. */
	let kept: Promise<unknown> = Promise.resolve();
	let keeping = $state(false);

	export function begin(): void {
		if (busy || phase === 'scoring' || phase === 'playing') return;
		cancelAnimationFrame(frame);
		start = Date.now();
		onstart(start);
		elapsed = 0;
		failed = '';
		keeping = true;
		kept = keep(fields, true, start - 1)
			.catch((e) => (failed = (e as Error).message))
			.finally(() => (keeping = false));
		phase = 'playing';
		frame = requestAnimationFrame(step);
	}

	function step(): void {
		elapsed = Date.now() - start;
		if (elapsed >= duration) void end();
		else frame = requestAnimationFrame(step);
	}

	async function end(): Promise<void> {
		cancelAnimationFrame(frame);
		phase = 'scoring';
		try {
			outcome = await onend(Math.min(elapsed, duration));
			await release();
			phase = 'done';
		} catch (e) {
			failed = (e as Error).message;
			phase = 'neutral';
		}
	}

	/** The game's fields IGNORED, once their TRACKED has landed: after every tap, each at or before now. */
	async function release(): Promise<void> {
		await kept;
		await keep(fields, false, Date.now() + 1);
	}

	async function reset(): Promise<void> {
		phase = 'neutral';
		busy = true;
		failed = '';
		try {
			await onreset?.();
			await startOver(fields);
		} catch (e) {
			failed = (e as Error).message;
		} finally {
			busy = false;
		}
	}

	onDestroy(() => cancelAnimationFrame(frame));
</script>

<section class="game" aria-label="the game" data-phase={phase} aria-busy={busy || keeping}>
	<div class="stage">
		{#if phase === 'playing'}
			{@render game(elapsed)}
		{:else if phase === 'done'}
			<dl class="result">
				{#each outcome as { name, value } (name)}
					<div><dt>{name}</dt><dd>{value}</dd></div>
				{/each}
			</dl>
		{:else if phase === 'scoring'}
			<p role="status">Reading your record…</p>
		{:else}
			{@render neutral()}
		{/if}
		<div class="controls">
			{#if phase === 'done'}
				<button type="button" class="command go" use:act={'run'} disabled={busy} onclick={reset}>Reset</button>
			{:else if cue && phase !== 'playing'}
				{#if phase === 'neutral'}{@render cue()}{/if}
			{:else}
				<button
					type="button"
					class="command go"
					use:act={'run'}
					disabled={busy || phase === 'scoring'}
					onclick={phase === 'playing' ? end : begin}
				>
					{#if phase === 'playing'}<span class="spent" aria-hidden="true" style:inline-size="{Math.min(100, (elapsed / duration) * 100)}%"></span>{/if}
					<span class="word">{phase === 'playing' ? 'End now' : 'Start'}</span>
				</button>
			{/if}
		</div>
	</div>
	{#if failed}<p class="failed">{failed}</p>{/if}
</section>

<style>
	/* The scene's height, what the box's pane leaves it past the border and the stage's padding: the controls stand
	   beside the scene, not under it (da-uzyih.4917). */
	.game {
		--stage: calc(100cqb - 2px - 0.5rem);
		border: 1px solid var(--accent);
		border-radius: 0.3rem;
	}

	/* A scene may draw past its own box, as the café's table does; the stage is where that stops, the box's rounded
	   corners included. It is the scene's height and its own padding. */
	.stage {
		position: relative;
		overflow: hidden;
		border-radius: inherit;
		display: grid;
		align-content: center;
		block-size: calc(var(--stage) + 0.5rem);
		box-sizing: border-box;
		padding: 0.25rem;
		/* The scene in the middle track, its sides equal until the right one reaches the controls' column: past that the
		   left gives way alone, so the scene slides left, and only with no left side left does it narrow (da-qx1it). */
		grid-template-columns: minmax(0, 1fr) minmax(0, max-content) minmax(calc(var(--controls, 5rem) + 0.25rem), 1fr);
	}

	.stage > :global(:not(.controls)) {
		grid-column: 2;
		min-inline-size: 0;
		max-inline-size: 100%;
	}

	.controls {
		position: absolute;
		inset-block: 0;
		inset-inline-end: 0.25rem;
		inline-size: var(--controls, 5rem);
		display: flex;
		justify-content: flex-end;
		align-items: center;
	}

	.controls button {
		min-block-size: 2rem;
		padding-inline: 0.25rem;
		border-color: var(--accent);
		color: var(--ink-bright);
	}

	/* The bar is the time gone, filling the button from its start edge under the word. */
	.go {
		position: relative;
		overflow: hidden;
		inline-size: 5rem;
	}

	.spent {
		position: absolute;
		inset-block: 0;
		inset-inline-start: 0;
		background: var(--accent);
		opacity: 0.45;
	}

	.word {
		position: relative;
	}

	.result {
		display: flex;
		justify-content: center;
		gap: 2rem;
		margin: 0;
		text-align: center;
	}

	.result dd {
		margin: 0;
		color: var(--ink-bright);
		font-size: 2rem;
	}

	/* A short pane (a tutorial game's, da-uzyih.4908) sets each number beside its name, on one line. */
	@container (block-size < 10rem) {
		.result {
			flex-wrap: wrap;
			gap: 0 1rem;
		}

		.result > div {
			display: flex;
			align-items: baseline;
			gap: 0.3rem;
		}

		.result dt {
			font-size: 0.8rem;
		}

		.result dd {
			font-size: 1.1rem;
		}
	}

	.failed {
		color: var(--error);
	}
</style>
