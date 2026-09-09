<!--
	The day frame — the shell's single screen, laid out as the wireframe
	(da-hrz0) draws it:

	    +---------------------------------------------+
	    |                  dayview                    |  full-width strip
	    +---------------------------+-----------------+
	    |                           |     nowview     |
	    |           repl            +-----------------+
	    |                           |      tail       |
	    +---------------------------+-----------------+
	    | widgets: click to log                   (+) |
	    +---------------------------------------------+

	Every region talks to datum through `run(argv)` in $lib/run.ts and
	nothing else — no region parses, formats or touches the database — so
	the wasm CLI (da-1vat) lands behind them without touching this file.
-->
<script lang="ts">
	import { onMount } from 'svelte';
	import CliPane from '$lib/CliPane.svelte';
	import DayStrip from '$lib/DayStrip.svelte';
	import Repl from '$lib/Repl.svelte';
	import WidgetBar from '$lib/WidgetBar.svelte';
	import { initDb, getAllFields, getFieldSpecs, getObservedStates } from '$lib/db.js';
	import { NOWVIEW, TAIL } from '$lib/panes.js';
	import { widgetsFor, type Widget } from '$lib/widgets.js';

	let ready = false;
	/**
	 * Bumped after every command; the panes re-run theirs when it changes.
	 *
	 * After *every* command, not only the ones that mutate: re-running
	 * three read-only commands against a local database costs nothing,
	 * and a list of which verbs write is a rule that would quietly drift
	 * out of step with the CLI and leave a pane showing stale data.
	 */
	let epoch = 0;

	/** The prompt's text — the widget bar (da-nz29) echoes commands here. */
	let command = '';
	let repl: Repl;
	let widgets: Widget[] = [];

	onMount(async () => {
		await initDb();
		ready = true;
		await refresh();
	});

	/**
	 * The bar's buttons are built from what has been logged, so a newly
	 * tracked field has to grow one — hence a rebuild on the same beat the
	 * panes refresh on.
	 */
	async function refresh(): Promise<void> {
		const [fields, specs, observed] = await Promise.all([
			getAllFields(),
			getFieldSpecs(),
			getObservedStates()
		]);
		widgets = widgetsFor(fields, specs, observed);
	}

	async function ran(): Promise<void> {
		epoch += 1;
		await refresh();
	}
</script>

<div class="frame">
	<div class="dayview"><DayStrip {epoch} /></div>

	<div class="repl">
		<Repl bind:this={repl} bind:input={command} disabled={!ready} on:ran={ran} />
	</div>

	<div class="side">
		<CliPane spec={NOWVIEW} {epoch} />
		<CliPane spec={TAIL} {epoch} />
	</div>

	<!--
		A widget press runs through the REPL rather than beside it, so the
		argv it executed lands in the scrollback and in the recall history:
		the bar teaches the CLI instead of hiding it (da-nz29).
	-->
	<div class="widgets">
		<WidgetBar {widgets} disabled={!ready} on:run={(e) => repl.runArgv(e.detail)} />
	</div>
</div>

<style>
	.frame {
		display: grid;
		grid-template-areas:
			'dayview dayview'
			'repl side'
			'widgets widgets';
		/* The dayview strip and the widget bar take what they need; the
		   middle row takes the rest and its panes scroll inside it.

		   The bar's row is `min-content`, not `auto`, because an `auto`
		   track's floor is the item's own `min-height` — and once the
		   tracks overflow the frame there is no free space to grow it
		   back, so the row stays at that floor while a wrapped bar hangs
		   out the bottom of it (da-vyo2). A `min-content` track is the
		   bar's real height however many rows it wraps to. */
		grid-template-rows: minmax(0, auto) minmax(0, 1fr) min-content;
		grid-template-columns: minmax(0, 2fr) minmax(0, 1fr);
		gap: 0.75rem;
		height: 100%;
	}

	.dayview {
		grid-area: dayview;
		/* No cap any more: the strip is a band of a known height plus
		   however much of the past has been pulled down over it, and the
		   stack caps itself (STACK_MAX in DayStrip.svelte). A cap here
		   would clip the days the reader just asked for. */
		display: grid;
	}

	.repl {
		grid-area: repl;
		display: grid;
		min-height: 0;
	}

	.side {
		grid-area: side;
		display: grid;
		grid-template-rows: minmax(0, 1fr) minmax(0, 1fr);
		gap: 0.75rem;
		min-height: 0;
	}

	.widgets {
		grid-area: widgets;
		/* Held open so the panes stop above the bottom edge rather than
		   growing into the row and snapping back once the bar has
		   buttons in it. A floor only — the row above sizes to the bar,
		   so this never caps it. */
		min-height: 2rem;
	}

	/* On a phone the three columns of the wireframe become one: side by
	   side, each pane would be too narrow to read a CLI line in. The
	   column scrolls as a whole, panes keep their own scroll inside it. */
	@media (max-width: 720px) {
		.frame {
			grid-template-areas:
				'dayview'
				'repl'
				'side'
				'widgets';
			grid-template-rows: auto minmax(14rem, 2fr) minmax(16rem, 3fr) min-content;
			grid-template-columns: minmax(0, 1fr);
			overflow-y: auto;
		}

		/* Those row minimums add up to more than a phone's viewport, so
		   the column really does scroll — and the widget bar, being last,
		   was the part that started off the bottom of the screen with no
		   sign that a scroll would find it (da-hkpd). Pin it to the
		   bottom of the scroller: the panes scroll behind it and the way
		   to log without typing is on screen at any viewport height.

		   Pinning the bar only helps if the bar is all in the row being
		   pinned; see the row template above for why it was not. */
		.widgets {
			position: sticky;
			bottom: 0;
			z-index: 1;
			padding-top: 0.5rem;
			background: #0a0a1a;
			border-top: 1px solid #1e1e38;
		}
	}
</style>
