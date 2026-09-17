<!--
	ONE pill, wherever a pill is drawn.

	Lifted out of Prompt.svelte (da-rskp) so the nowview's margin can show
	the time and `dur=` pills beside the instants they name without copying
	the markup or the CSS: there is one store of pills, two places that draw
	them, and a press in either is the same press.

	vividn, on the deployed build: "click anywhere except lock to dismiss (so
	no x)" and then "maybe doch keep the x icon, but still click anywhere to
	dismiss" (da-tnv3). So the × is a MARK, not a target: the pill is
	sibling buttons — a nested one would be invalid HTML — and the remove
	button is every pixel that is not the lock, padding and × alike.

	A pill drawn in SECTIONS (da-ui4i: the time cursor's date and its time)
	is that same row of buttons, one per section, divided by a hairline: a
	section that leaves something behind presses down to its `rest`, and the
	one that does not — the last — carries the × and takes the pill away. A
	pill with no sections IS one section, so there is one path here and the
	plain pill is drawn exactly as it was. The lock is a flat SVG in the
	pill's own ink: the emoji clashed with everything around it, and its two
	states differ by one path.

	Its size is the command line's (`--input-font`), because beside the
	caret that is what a pill has to read at; a caller that draws pills
	somewhere else sets that number for itself.
-->
<script lang="ts">
	import { dropSection, sectionsOf, toggleLockPill, type Pill } from './bar.js';

	export let pill: Pill;
</script>

<span class="pill {pill.kind}" class:locked={pill.locked}
	>{#each sectionsOf(pill) as section, i}<button
			type="button"
			class="remove"
			aria-label={section.rest ? `drop ${section.text}` : `remove ${pill.text}`}
			on:click={() => dropSection(pill.id, i)}
			><span class="label">{section.text}</span
			>{#if !section.rest}<span class="x" aria-hidden="true">×</span>{/if}</button
		>{/each}<button
		type="button"
		class="lock"
		aria-label="{pill.locked ? 'unlock' : 'lock'} {pill.text}"
		aria-pressed={!!pill.locked}
		on:click={() => toggleLockPill(pill.id)}
		><svg class="glyph" viewBox="0 0 16 16" aria-hidden="true" focusable="false"
			><rect x="3" y="7" width="10" height="7" rx="1.5" /><path
				d={pill.locked ? 'M5.5 7V5a2.5 2.5 0 0 1 5 0v2' : 'M10.5 7V5a2.5 2.5 0 0 1 5 0'}
			/></svg
		></button
	></span
>

<style>
	/* SOLID, and the size of what is typed beside them. vividn, on the
	   deployed build: "We need to make the pills much more prominent, they
	   are almost invisible right now. Solid background color… increase
	   their font size to be the same as the input bar." So the kind's
	   colour — the colour the outline used to be — FILLS the pill and the
	   page's own ground is the ink on it (`--page`'s contract), which is
	   the one pairing the palette already names.

	   `--pill` is that colour, and the only thing a kind sets: the fill,
	   the ink and the locked ring are all derived from it here, so a new
	   kind stays one line. */
	.pill {
		--pill: var(--accent);
		display: inline-flex;
		align-items: center;
		border: none;
		border-radius: 999px;
		/* So a button's hover wash is clipped to the pill's own edge. */
		overflow: hidden;
		color: var(--page);
		background: var(--pill);
		font-family: ui-monospace, SFMono-Regular, Menlo, Consolas, monospace;
		font-size: var(--input-font, 1rem);
		line-height: 1;
		white-space: nowrap;
	}

	/* Two siblings dividing the pill between them: the remove button takes
	   every pixel that is not the lock, so a press on the text, on the ×
	   or on the padding is the same press. */
	.pill button {
		display: flex;
		align-items: center;
		gap: 0.3em;
		background: none;
		border: none;
		padding: 0.25rem 0.3rem 0.25rem 0.55rem;
		font: inherit;
		color: inherit;
		cursor: pointer;
	}

	.pill button.lock {
		padding: 0.25rem 0.5rem 0.25rem 0.25rem;
	}

	/* The divider between two sections of one pill: a hairline in the
	   pill's own ink, thin enough to read as a seam in one pill rather than
	   as the gap between two. */
	.pill button.remove + button.remove {
		border-left: 1px solid color-mix(in srgb, var(--page) 45%, transparent);
		padding-left: 0.45rem;
	}

	/* A mark, not a target — the whole of the button beside it removes. */
	.pill .x {
		opacity: 0.65;
	}

	/* Flat and one colour, drawn in the pill's ink: a filled body and a
	   shackle that is the only thing the two states differ by. */
	.glyph {
		display: block;
		width: 0.8em;
		height: 0.8em;
		fill: currentColor;
	}

	.glyph path {
		fill: none;
		stroke: currentColor;
		stroke-width: 1.7;
		stroke-linecap: round;
	}

	/* A locked pill stands out: it will not clear on its own after a press.
	   An inset ring in the ink, so «will not clear» needs no second colour. */
	.pill.locked {
		box-shadow: inset 0 0 0 2px var(--page);
	}

	/* Which half the pointer is over — the only thing that says the lock is
	   a target of its own now that the rest of the pill is one press. */
	.pill button:hover {
		background: color-mix(in srgb, var(--page) 20%, transparent);
	}
</style>
