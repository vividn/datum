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
	import Repl from '$lib/Repl.svelte';
	import WidgetBar from '$lib/WidgetBar.svelte';
	import { initDb, getAllFields, getFieldSpecs, getObservedStates } from '$lib/db.js';
	import { DAYVIEW, NOWVIEW, TAIL } from '$lib/panes.js';
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
	<div class="dayview"><CliPane spec={DAYVIEW} {epoch} /></div>

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
		   middle row takes the rest and its panes scroll inside it. */
		grid-template-rows: minmax(0, auto) minmax(0, 1fr) auto;
		grid-template-columns: minmax(0, 2fr) minmax(0, 1fr);
		gap: 0.75rem;
		height: 100%;
	}

	.dayview {
		grid-area: dayview;
		/* Tall enough to be a strip, short enough to stay one. */
		max-height: 9rem;
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
		   buttons in it. */
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
			grid-template-rows: auto minmax(14rem, 2fr) minmax(16rem, 3fr) auto;
			grid-template-columns: minmax(0, 1fr);
			overflow-y: auto;
		}
	}
</style>
