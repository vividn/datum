<!--
	The day frame — the shell's single screen, laid out as the wireframe
	(da-hrz0) draws it:

	    +---------------------------------------------+
	    |                  dayview                    |  full-width strip
	    +---------------------------+-----------------+
	    |                           |     nowview     |
	    |           repl            +-----------------+
	    |                           |      tail       |
	    |                           +-----------------+
	    |                           |     command     |
	    +---------------------------+-----------------+
	    | widgets: click to log                   (+) |
	    +---------------------------------------------+

	Every region talks to datum through `run(argv)` in $lib/run.ts and
	nothing else — no region parses, formats or touches the database. Behind
	the seam is the real CLI (da-g8o8).
-->
<script lang="ts">
	import { onMount, onDestroy } from 'svelte';
	import { page } from '$app/stores';
	import CliPane from '$lib/CliPane.svelte';
	import CommandPane from '$lib/CommandPane.svelte';
	import DayStrip from '$lib/DayStrip.svelte';
	import Repl from '$lib/Repl.svelte';
	import WidgetBar from '$lib/WidgetBar.svelte';
	import { initDb } from '$lib/db.js';
	import { DEFAULT_DAY_HEIGHT, NOWVIEW, TAIL } from '$lib/panes.js';
	import { run } from '$lib/run.js';
	import { widgetsFor, type FieldListing, type Widget } from '$lib/widgets.js';
	import { onExternalChange } from '$lib/changes.js';

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

	/**
	 * The day strip's one setting, held here until the dashboard engine
	 * (da-jdop) owns the strip and keeps it in the widget's settings
	 * (da-zb0d). The strip hands it in as a prop and hears `change` when
	 * the notch moves it; `?dayHeight=` seeds it from the URL, standing in
	 * for the saved setting the engine will pass. Not remembered between
	 * visits: that is the layout tree's job, not this page's.
	 */
	let dayHeight = Number($page.url.searchParams.get('dayHeight')) || DEFAULT_DAY_HEIGHT;

	onMount(async () => {
		await initDb();
		ready = true;
		await refresh();
	});

	// Another tab wrote. Same treatment as a local command finishing — bump
	// the epoch the panes hang off and rebuild the widget bar — so there is
	// one refresh path rather than two that can disagree.
	//
	// Subscribed here rather than from `onMount`: Svelte takes a teardown from
	// a SYNCHRONOUS onMount only, and an async one returns a promise it cannot
	// call. Safe during init because `onExternalChange` opens nothing outside
	// a browser (see $lib/changes.ts) and returns an empty teardown there.
	onDestroy(onExternalChange(() => void ran()));

	/**
	 * The bar's buttons are built from `datum fields` — what has been logged
	 * and what has been declared — so a newly tracked field has to grow one:
	 * hence a rebuild on the same beat the panes refresh on.
	 */
	async function refresh(): Promise<void> {
		const listed = await run(['fields', '--json']);
		if (listed.code === 0) widgets = widgetsFor(JSON.parse(listed.stdout) as FieldListing[]);
	}

	async function ran(): Promise<void> {
		epoch += 1;
		await refresh();
	}
</script>

<div class="frame">
	<div class="dayview" data-day-height={dayHeight}>
		<DayStrip {epoch} {dayHeight} on:change={(e) => (dayHeight = e.detail)} />
	</div>

	<div class="repl">
		<Repl bind:this={repl} bind:input={command} disabled={!ready} on:ran={ran} />
	</div>

	<div class="side">
		<CliPane spec={NOWVIEW} {epoch} />
		<CliPane spec={TAIL} {epoch} />
		<CommandPane {epoch} />
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
		display: grid;
		/* The strip takes its height from its box, not from a handle of
		   its own (da-zb0d), so until the dashboard engine (da-jdop) puts
		   it in a pane whose border is the control, this fixed row is the
		   box: today's band and whatever of the past fits above it.
		   Enough for today at the default day height on any screen, never
		   more than a laptop can spare above the REPL. */
		height: clamp(9rem, 28vh, 20rem);
		min-height: 0;
	}

	.repl {
		grid-area: repl;
		display: grid;
		min-height: 0;
	}

	/* Each pane has a floor: datum draws nowview no shorter than a 50px
	   band plus its axis (92px of picture), and 9rem is that plus the
	   pane's title bar and padding. A screen too short for three panes at
	   their floor scrolls this column instead of squeezing nowview into a
	   scrollbar of its own (da-wb2n). */
	.side {
		grid-area: side;
		display: grid;
		grid-template-rows: repeat(3, minmax(9rem, 1fr));
		gap: 0.75rem;
		min-height: 0;
		overflow-y: auto;
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
			grid-template-rows: auto minmax(14rem, 2fr) minmax(28.5rem, 4.5fr) min-content;
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
