<!--
	A pane that renders one datum command.

	Thin on purpose: it runs `spec.argv` through the `run(argv)` seam and
	shows the text that comes back. No parsing, no formatting, no database
	— everything it displays was produced by datum itself.
-->
<script module lang="ts">
	let panes = 0;
</script>

<script lang="ts">
	import { onMount, untrack, type Snippet } from 'svelte';
	import { browser } from '$app/env';
	import Pane from './Pane.svelte';
	import { act } from './act.js';
	import CliOutput from './CliOutput.svelte';
	import TailTable from './TailTable.svelte';
	import { asking, UNANSWERED } from './dbio.js';
	import { record } from './replStore.js';
	import { run, type RunResult } from './run.js';
	import { shellJoin } from './shellSplit.js';
	import { minute, tenSeconds } from './clock.js';
	import { dependsOnNow, paneArgv, wantsBox, type PaneBox, type PaneSpec } from './panes.js';
	import { settle } from './layout/resizing.js';

	let {
		spec,
		/** Any change re-runs the command. The frame bumps it after each run. */
		epoch = 0,
		/**
		 * Stop re-running a command once a run of it has WRITTEN (da-wb2n).
		 *
		 * For a pane whose command the reader types: `occur coffee` there would
		 * log a coffee on every epoch, and with other tabs refreshing on a write,
		 * two tabs would feed each other. So the first run is the one the reader
		 * asked for, and a run the worker reports as `changed` puts the pane on
		 * ▶ from then on. By effect, never by verb — the frame's own rule
		 * (+page.svelte) — so `get`, which moves the last-docs pointer, is on ▶
		 * too. A new `spec` is a new command, and runs once again. So is a run
		 * the CLI stopped to ask about (da-f5pg2, a stale `_LAST` under `retime`):
		 * ▶ asks it in a confirm, which a refresh must not do on every epoch.
		 */
		guard = false,
		/**
		 * The body's type, as a scale on the pane's 0.85rem (da-hkjp). The tail
		 * kind declares it as an option and the registry resolves the name to
		 * this number; every other CLI pane is 1.
		 */
		scale = 1,
		/**
		 * The spec whose run wrote: while it is `spec`, the pane runs on ▶ only.
		 * A prop so a pane that keeps its command across a mount can keep this
		 * with it — known, not learned again by writing once more (da-v36y).
		 */
		writer = $bindable(null),
		/**
		 * What the page lays over or under the CLI's own answer before it is
		 * shown: the nowview's crop of the day's data layer (da-0e06p).
		 */
		compose = undefined,
		/**
		 * Whether a picture catching up is dimmed until the new one lands. The
		 * nowview's is not (da-ygpto): it holds the picture a gesture left on
		 * screen exactly as it is, and only the answer replaces it.
		 */
		dims = true,
		/** The caller's title controls, forwarded as the pane's `heading`. */
		title: titled
	}: {
		spec: PaneSpec;
		epoch?: number;
		guard?: boolean;
		scale?: number;
		writer?: PaneSpec | null;
		compose?: (stdout: string, argv: string[]) => string;
		dims?: boolean;
		title?: Snippet;
	} = $props();

	let result = $state.raw<RunResult | null>(null);
	/** The argv on screen, as its words. */
	let shown = $state('');
	/** This pane in the worker's queue: its newer run drops its older, unstarted one. */
	const key = `pane:${++panes}`;

	let body: HTMLDivElement | undefined = $state();
	/** The body's content box, once measured. See `paneArgv`. */
	let box = $state.raw<PaneBox | null>(null);

	// The content box, not the element's own width: the body is padded, and
	// a band authored 19px wider than the box it is shown in is scaled back
	// down — which is the bug (da-m0cj). ResizeObserver reports the content
	// box directly, and reports it again on every resize, so the band is
	// re-drawn at the new size rather than stretched to it. Observed whatever
	// the command, since a typed one can become `dayview` (da-wb2n).
	//
	// Through `settle`: the observer fires on every frame of a border drag,
	// and a redraw per frame was the jitter (da-jgrn). The box lands once
	// the drag is dropped, or once a burst of resizes has been over for a
	// beat; until then the last picture stays, scaled to the box.
	onMount(() => {
		if (!body) return;
		const settled = settle<PaneBox>((next) => {
			if (box?.width !== next.width || box?.height !== next.height) box = next;
		});
		const observer = new ResizeObserver(([entry]) => {
			// Whole pixels, rounded down: a band a fraction of a pixel wider
			// than its box is a band the browser scales, and the text with it.
			// Not the tail's: its rows are fractions of a pixel tall and are
			// counted against the box as it is, and a floored box drops the
			// row its fraction holds (da-zs3t9).
			const { width, height } = entry.contentRect;
			settled.push(table ? { width, height } : { width: Math.floor(width), height: Math.floor(height) });
		});
		observer.observe(body);
		return () => {
			observer.disconnect();
			settled.stop();
		};
	});

	/** Whether a run is out, and whether a newer one was asked for meanwhile. */
	let out = false;
	let waiting = false;

	/**
	 * Run the newest request: at most one run out and one waiting (da-eoq3).
	 *
	 * A burst of clicks on the dayview asks for a nowview per click, and a
	 * process cannot be stopped once started, so the burst costs the run
	 * already out plus the LAST one asked for — never the ones between, and
	 * the run that comes back behind a newer request is not painted. The
	 * same rule is the write guard's (da-wb2n): a second run sent before the
	 * first came back would be the second write. Keyless while guarded, so
	 * the worker never drops a command that may write.
	 */
	async function refresh(): Promise<void> {
		if (out) return void (waiting = true);
		out = true;
		const of = spec;
		const argv = request.argv!;
		const next = await run(argv, undefined, undefined, guard ? undefined : key, undefined, undefined, !guard);
		out = false;
		if (guard && (next.changed?.length || next.code === UNANSWERED)) writer = of;
		if (waiting) {
			waiting = false;
			if (request.argv && !(guard && writer === spec)) return refresh();
		}
		if (next.superseded) return;
		result = next;
		shown = JSON.stringify(argv);
	}

	// A press is a command the reader asked for, like a line typed: into
	// the shared history and the bottom bar's receipt (da-0a5o). The
	// automatic runs are the pane's own business and record nothing.
	async function press(): Promise<void> {
		const argv = request.argv;
		if (!argv) return;
		const ranAt = Date.now();
		result = await asking(run)(argv);
		shown = JSON.stringify(argv);
		record(shellJoin(argv), ranAt, result);
	}
	// Not on mount: the static build prerenders this page, and there is no
	// database (or `run`) outside the browser. The request is rebuilt from
	// the epoch and the box together, so a resize re-runs a drawn view the
	// same way a write does — and only a drawn view: for any other command
	// `sized` stays null, and an unchanged null invalidates nothing.
	// The tail table reads its own listing, sized to the box (see
	// TailTable.svelte); every other pane is one command through the seam.
	//
	// By the spec's WORDS, not the spec: the layout hands a pane its spec
	// afresh on every change to the tree (a spread prop), and Svelte counts
	// an object handed over again as changed — so a request built from
	// `spec` itself re-ran every drawn pane on every nudge of a border
	// (da-jgrn). Strings and booleans are compared by value, and stay put.
	let words = $derived(JSON.stringify(spec.argv));
	let table = $derived(spec.view === 'tail-table');
	let drawn = $derived(wantsBox(spec.argv));
	let sized = $derived(drawn ? box : null);
	// A pane whose picture is of NOW re-runs when the minute turns
	// (da-r61i): nowview's durations grow, dayview's now-line moves, and
	// until this they stood still between writes. The clock is the app's
	// one timer (#lib/clock.ts) and every such pane shares it; a pane
	// running anything else — the tail, a `get` — folds in a constant and
	// so is not invalidated by the tick at all. nowview's snapshot moves
	// on a ten-second beat instead (da-fprk0); a nowview with a window is
	// drawn `--ahead` and slid by its widget, and ticks not at all (da-hklaz).
	let ticks = $derived(!dependsOnNow(spec.argv) ? 0 : spec.argv[0] === 'nowview' ? $tenSeconds : $minute);
	let request = $derived({ argv: table ? null : paneArgv(JSON.parse(words) as string[], sized), epoch, ticks });
	let manual = $derived(guard && writer === spec);
	$effect.pre(() => {
		const asked = request;
		if (browser && asked.argv && !manual) untrack(() => void refresh());
	});
	// The picture on screen is not the one asked for: the cursor moved, and
	// the new one is on its way. Dimmed rather than presented as current.
	let behind = $derived(dims && !!result && !manual && !!request.argv && JSON.stringify(request.argv) !== shown);
</script>

<!--
	This pane forwards its caller's title controls, and has none of its own —
	its name is the frame's to draw (da-ndjo).
-->
<Pane title={spec.title} heading={titled} {scale} bind:body>
	{#if manual}
		<p class="quiet">
			this command writes — it runs only when you press
			<button type="button" class="play" use:act={'run'} title="run it" aria-label="run it" onclick={press}>▶</button>
		</p>
	{/if}
	{#if spec.view === 'tail-table'}
		<!-- The one pane whose content is a grid: `datum tail` IS a table,
		     and this lays the CLI's own cells out in a real one rather than
		     showing the padding a terminal would need (da-6ec2). -->
		<!-- The box says how many rows fit and the body is where the reader
		     scrolls: `tail -w` in a pane (da-lan0). -->
		<TailTable {epoch} empty={spec.empty ?? ''} {box} scroller={body} />
	{:else if !result}
		<p class="quiet">…</p>
	{:else if result.stdout}
		<CliOutput text={compose ? compose(result.stdout, result.argv) : result.stdout} {behind} />
	{:else if result.stderr}
		<CliOutput text={result.stderr} stream="stderr" error {behind} />
	{:else if spec.empty}
		<p class="quiet">{spec.empty}</p>
	{/if}
</Pane>

<style>
	.quiet {
		margin: 0;
		color: var(--quiet);
		font-style: italic;
	}
	.play {
		font: inherit;
		font-style: normal;
		color: var(--accent);
		background: none;
		border: 1px solid var(--rule);
		border-radius: 4px;
		cursor: pointer;
	}
	/* It runs the command at once, so the double outline (#lib/act.ts). */
	.play:global([data-act='run']) {
		border: 3px double var(--accent);
	}
</style>
