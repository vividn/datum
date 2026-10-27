<!--
	`bar` — the bottom bar's feel, as two live knobs (da-o2wm8). The one
	tool the debug widget holds today; see DebugPane.svelte.

	vividn, 2026-09-21, having landed the bar's three-phase drag: "the bar
	doesn't feel quite right yet. […] Allow me to change these values you
	mentioned TAU and DETENT, so that I can try different values easily
	and report back which feels nice."

	Each knob gets a SLIDER and a NUMBER, side by side, both editing one
	value: the slider is how a thumb FEELS the sweep, which is the whole
	point, and the number is how it is REPORTED BACK, which is the other
	whole point. The ranges are wide enough that the ends are boring, so
	the good value is not at an edge — and `DETENT = 0` is reachable,
	because "no detents at all" is a feel he has to be able to try.

	Beside them: the live pair as ONE COPYABLE LINE (the artifact he
	sends back), the default beside each current value so he can see he
	has moved and by how much, and one tap back to the defaults, because
	he will get lost and getting back must be free.

	A change lands on the very NEXT release and needs no reload: `settle`
	in $lib/log.ts reads the store at the moment a finger lets go. That
	loop — slide, throw, slide, throw — is the instrument.
-->
<script lang="ts">
	import { DEFAULT_TUNING, type Tuning } from './detent.js';
	import { RANGE, resetTuning, setTuning, tuning } from './tuning.js';

	/**
	 * The knobs, in the order they are read.
	 *
	 * The RANGES are not here: they live beside the clamp that enforces
	 * them ($lib/tuning.ts), so what this offers and what the store allows
	 * are one pair of numbers. `meaning` is the sentence the name carries
	 * as its tooltip; `unit` is what the number is counted in.
	 */
	const KNOBS: { key: keyof Tuning; name: string; unit: string; meaning: string }[] = [
		{ key: 'tau', name: 'TAU', unit: 'ms', meaning: 'how long a release keeps gliding' },
		{ key: 'detent', name: 'DETENT', unit: 'px', meaning: 'how near a detent takes a glide' }
	];

	/** The artifact: the live pair, as one line to select and send back. */
	const pair = $derived(`TAU=${$tuning.tau} DETENT=${$tuning.detent}`);
	const moved = $derived($tuning.tau !== DEFAULT_TUNING.tau || $tuning.detent !== DEFAULT_TUNING.detent);

	/** A clipboard is a convenience; the line is selectable either way. */
	const canCopy = $derived(typeof navigator !== 'undefined' && !!navigator.clipboard);
	let copied = $state(false);

	function set(knob: (typeof KNOBS)[number], value: string): void {
		// An empty box is a box being retyped, not a request for zero. What
		// is out of range is brought in by the store, not here.
		if (value.trim() === '') return;
		setTuning({ [knob.key]: Math.round(Number(value)) });
	}

	async function copy(): Promise<void> {
		try {
			await navigator.clipboard.writeText(pair);
			copied = true;
			setTimeout(() => (copied = false), 1200);
		} catch {
			// Denied or insecure: the line is still there to be selected.
		}
	}
</script>

<div class="knobs">
	{#each KNOBS as knob (knob.key)}
		<div class="knob">
			<div class="head">
				<span class="name" title={knob.meaning}>{knob.name}</span>
				<input
					class="number"
					type="number"
					inputmode="numeric"
					min={RANGE[knob.key].min}
					max={RANGE[knob.key].max}
					step="1"
					value={$tuning[knob.key]}
					aria-label={knob.name}
					onchange={(e) => set(knob, e.currentTarget.value)}
				/>
				<span class="unit">{knob.unit}</span>
				<span class="was">default {DEFAULT_TUNING[knob.key]}</span>
			</div>
			<input
				class="slide"
				type="range"
				min={RANGE[knob.key].min}
				max={RANGE[knob.key].max}
				step="1"
				value={$tuning[knob.key]}
				aria-label="{knob.name} slider"
				oninput={(e) => set(knob, e.currentTarget.value)}
			/>
		</div>
	{/each}
</div>

<p class="report">
	<code class="pair">{pair}</code>
	{#if canCopy}
		<button type="button" class="tool" onclick={copy}>{copied ? 'copied' : 'copy'}</button>
	{/if}
	<button type="button" class="tool" disabled={!moved} onclick={resetTuning}>reset to defaults</button>
</p>

<style>
	/* THE SWEEP GETS THE WIDTH. The number, its unit and its default sit on
	   a line above, and the slider has the whole of the next one: squeezed
	   in beside them it was 80px on a 390px phone — 14 units of `TAU` per
	   pixel, which is not a thing a thumb can feel its way along, and
	   feeling it is the entire point. 22rem stops a laptop giving it a
	   thousand pixels instead, where a thumb's own width is a whole step. */
	.knob {
		margin-bottom: 0.5rem;
	}
	.head {
		display: flex;
		flex-wrap: wrap;
		align-items: center;
		gap: 0.1rem 0.45rem;
	}
	/* Both names to one width, so the two rows read as a pair. */
	.name {
		min-width: 4.5rem;
		color: var(--accent);
		letter-spacing: 0.06em;
	}
	.slide {
		display: block;
		box-sizing: border-box;
		width: 100%;
		max-width: 22rem;
		margin: 0.1rem 0 0;
		accent-color: var(--accent);
	}
	/* 24px tall, the house rule for a control a finger hits (web-conventions
	   #12): this whole widget is for a thumb on a phone. */
	.number {
		box-sizing: border-box;
		width: 3.7rem;
		min-height: 24px;
		padding: 0.1rem 0.2rem;
		font: inherit;
		color: var(--ink);
		background: var(--page);
		border: 1px solid var(--rule);
		border-radius: 4px;
	}
	.unit,
	.was {
		color: var(--quiet);
	}
	.report {
		display: flex;
		flex-wrap: wrap;
		align-items: center;
		gap: 0.4rem;
		margin: 0.6rem 0 0;
	}
	/* `user-select: all`: one tap takes the WHOLE line, which is what a
	   thumb needs — selecting a run of monospace by dragging two handles
	   on a phone is the reason a reader gives up and retypes it wrong. */
	.pair {
		user-select: all;
		-webkit-user-select: all;
		padding: 0.15rem 0.3rem;
		background: var(--page);
		border: 1px solid var(--rule);
		border-radius: 4px;
	}
	.tool {
		min-height: 24px;
		padding: 0.1rem 0.5rem;
		font: inherit;
		color: var(--accent);
		background: none;
		border: 1px solid var(--rule);
		border-radius: 4px;
		cursor: pointer;
	}
	.tool[disabled] {
		color: var(--quiet);
		cursor: default;
	}
</style>
