<!--
	The colour control (da-ci85o): the swatch IS it. vividn, 2026-09-23 —
	«I would show the color swatch, and then if the user taps on it, they
	can easily select a new one». No hex box: a tap opens the set below it,
	a tap on one of those sets it and folds the set away.

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
<script context="module" lang="ts">
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
	/** The colour, `#rrggbb`; empty for `auto`. */
	export let value = '';
	/** What it is the colour of, for a reader that cannot see it. */
	export let label: string;
	/** What `auto` paints: `defaultColor` of the field's or the state's name. */
	export let fallback: string;

	let open = false;

	function pick(next: string) {
		value = next;
		open = false;
	}
</script>

<span class="swatch-control">
	<button
		type="button"
		class="swatch"
		class:auto={!value}
		style:background={value || fallback}
		aria-label="{label}: {value || `auto, ${fallback}`}"
		aria-expanded={open}
		data-color={value}
		on:click={() => (open = !open)}
	></button>
	{#if open}
		<span class="set" role="listbox" aria-label="{label}, choose">
			<button
				type="button"
				class="swatch auto"
				role="option"
				aria-selected={!value}
				aria-label="auto, {fallback}"
				data-color=""
				style:background={fallback}
				on:click={() => pick('')}
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
					on:click={() => pick(color)}
				></button>
			{/each}
		</span>
	{/if}
</span>

<style>
	.swatch-control {
		display: flex;
		flex-wrap: wrap;
		align-items: center;
		gap: 0.4rem;
	}

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

	.swatch:focus-visible,
	.swatch[aria-selected='true'] {
		outline: 2px solid var(--accent);
		outline-offset: 1px;
	}

	.set {
		display: flex;
		flex-wrap: wrap;
		gap: 0.25rem;
		inline-size: 100%;
	}
</style>
