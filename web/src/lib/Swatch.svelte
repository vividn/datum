<!--
	The colour control (da-ci85o): the swatch IS it. vividn, 2026-09-23 —
	«I would show the color swatch, and then if the user taps on it, they
	can easily select a new one». No hex box: a tap opens the set, a tap on
	one of those sets it and closes the set. The set is a sheet over the
	form, not a band that pushes it down (da-rpfws), vividn, 2026-10-19:
	«open up a color picker popup, rather than moving everything down».

	One component for the field's colour and every state's, so both are
	picked the same way and from the same set. The set is the CLI's twelve
	named hues (`NAMED_HUES` in datum-core's kit_color.rs) at three
	lightnesses, and `auto` — no colour stored, which the renderer paints
	with the name's md5 colour as datum always has.

	`auto` is drawn AS that colour, in a dashed border (da-jf6l2). vividn,
	2026-09-26 — «actually show the default color, maybe show that it is
	the default somehow else (dashed border perhaps)». Dashed is what the
	bar already says «not the field's own» with; a picked colour is solid.
-->
<script module lang="ts">
	/** `hsl(h, s%, l%)` as `#rrggbb`, which is what a spec stores. */
	function hex(h: number, s: number, l: number): string {
		const a = s * Math.min(l, 1 - l);
		const f = (n: number) => {
			const k = (n + h / 30) % 12;
			const c = l - a * Math.max(-1, Math.min(k - 3, 9 - k, 1));
			return Math.round(c * 255).toString(16).padStart(2, '0');
		};
		return `#${f(0)}${f(8)}${f(4)}`;
	}

	/** Mid-tones first, then the light row, then the dark one: a row wraps as a band of one lightness. */
	export const SWATCHES: string[] = [0.5, 0.72, 0.3].flatMap((l) =>
		Array.from({ length: 12 }, (_, i) => hex(i * 30, 0.75, l))
	);
</script>

<script lang="ts">
	import { act } from './act.js';
	import Sheet from './Sheet.svelte';

	interface Props {
		/** The colour, `#rrggbb`; empty for `auto`. */
		value?: string;
		/** What it is the colour of, for a reader that cannot see it. */
		label: string;
		/** What `auto` paints: `defaultColor` of the field's or the state's name, `transparent` for no name. */
		fallback: string;
	}

	let { value = $bindable(''), label, fallback }: Props = $props();

	let open = $state(false);

	function pick(next: string) {
		value = next;
		open = false;
	}
</script>

<button
	type="button"
	class="swatch"
	class:auto={!value}
	style:background={value || fallback}
	aria-label="{label}: {value || `auto, ${fallback}`}"
	aria-expanded={open}
	aria-haspopup="dialog"
	data-color={value}
	use:act={'open'}
	onclick={() => (open = true)}
></button>
<Sheet title={label} bind:open>
	<span class="set" role="listbox" aria-label="{label}, choose">
		<button
			type="button"
			class="swatch auto"
			role="option"
			aria-selected={!value}
			aria-label="auto, {fallback}"
			data-color=""
			style:background={fallback}
			onclick={() => pick('')}
		></button>
		{#each SWATCHES as color (color)}
			<button
				type="button"
				class="swatch"
				role="option"
				aria-selected={value === color}
				aria-label={color}
				data-color={color}
				style:background={color}
				onclick={() => pick(color)}
			></button>
		{/each}
	</span>
</Sheet>

<style>
	.swatch {
		box-sizing: border-box;
		inline-size: 28px;
		block-size: 28px;
		padding: 0;
		border: 1px solid var(--rule);
		border-radius: 4px;
		cursor: pointer;
	}

	/* No colour stored: the colour it falls back to, dashed as not its own. */
	.swatch.auto {
		border: 2px dashed var(--ink-bright);
	}

	/* The `›` app.css draws on an opener, beside the box rather than in it. */
	.swatch:global([data-act='open']) {
		position: relative;
		margin-inline-end: 0.6em;
	}

	.swatch:global([data-act='open'])::after {
		position: absolute;
		inset-block-start: 50%;
		inset-inline-start: 100%;
		margin-inline-start: 0.15em;
		transform: translateY(-50%);
		color: var(--muted);
		font-size: 1rem;
		line-height: 1;
	}

	.swatch:focus-visible,
	.swatch[aria-selected='true'] {
		outline: 2px solid var(--accent);
		outline-offset: 1px;
	}

	.set {
		display: flex;
		flex-wrap: wrap;
		gap: 0.25rem;
	}
</style>
