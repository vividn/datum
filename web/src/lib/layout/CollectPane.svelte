<!--
	The collect widget as a widget kind.

	Its own body rather than `Pane.svelte`'s, for the padding and nothing
	else: the BAR is the scroller (CollectWidget.svelte), so the pane keeps
	whatever height its split ratios give it and a bar with more buttons
	than fit is reached by scrolling rather than by the pane growing
	(da-mbz2) — sideways, on as many rows as fit (da-vu7pi, da-arms4,
	`ButtonRows.svelte`). The widget's panels hang UPWARD out of the bar and a
	scroller would clip them, so they are hung against the screen instead
	(`hang` in CollectWidget.svelte). A press goes, with the bottom bar's
	pills after it, to the engine's `press`, which routes it through
	whichever CLI widget is on screen; a record's line, and a log with keys to
	type (da-chml5), go to the bottom bar's prompt, unrun.

	`pending` is the command the line holds (da-ltux): it filters the
	list, and a press under it runs `<command> <field>` at once — with the
	flags the line carries between them (da-bbmpx) — and spends the command:
	the box empties, and the list is whole again.

	The SELECTION is the other thing the widget answers (da-qu16): a length
	pill among the bar's pills leaves the fields a length can be spent on,
	and the lanes a press on a picture landed in bring those fields to the
	front. Both arrive here as stores and go in as plain props, so the
	widget itself asks nothing of the page.

	The NOW STRIP (da-uuzu6) is today's data layer, the one the nowview and
	the day strip hold (#lib/dayLayers.ts), asked again at each write's epoch.
	A press that logs at now adds its mark at the tap's instant at once
	(`pendingOf`), the instant the CLI's clock is frozen at (`gestureAt`), so
	the layer's own mark lands where it stood. A refused press takes it back;
	one that wrote keeps it until a layer asked after the write has come.

	At a SELECTED TIME (da-0qdsp, the cursor's instant, not a length ending
	now) the strip is that day's layer centered on it, still, and the buttons
	stand as `now --json -t <time>` says the fields stood then, a moment
	field whose entry is at that instant among the active ones (`standingAt`,
	`momentsAt`). A tap on that field deletes the entry, with no pill: an hid
	pill on the bar would be deleted too. In 2-click mode the tap opens the
	field, and its screen's primary action, the command pane's ready command,
	is that delete, `moment` beside it (`here` in engine.ts, da-s8gr7).

	`only` (da-gfy7w) is the widget's `fields` option: set, the buttons are
	those fields alone, in order and mode as ever; empty, every field. A name
	no field has is not drawn, and is offered beside the fields there are so
	the menu can take it off again.

	`showIgnored` (da-xhio) arrives as a prop too, though its value is the
	SESSION's rather than the layout's: the renderer merges the two before
	it draws (Layout.svelte, #lib/layout/session.ts), so neither this pane
	nor the widget below it knows the difference.
-->
<script lang="ts">
	import CollectWidget from '#lib/CollectWidget.svelte';
	import { get } from 'svelte/store';
	import { untrack } from 'svelte';
	import { clearPending, dropHids, pending, pills, prefill, text } from '#lib/bar.ts';
	import { pointed } from '#lib/cursor.ts';
	import { run as read } from '#lib/run.ts';
	import { minute } from '#lib/clock.ts';
	import { dateBack, daysBack, layers, standIn, type Layer } from '#lib/dayLayers.ts';
	import { momentsAt, pendingOf, tapped, taps, unheld } from '#lib/nowStrip.ts';
	import { withPills } from '#lib/selection.ts';
	import { flagLine, hasDuration } from '#lib/commands.ts';
	import { advance } from '#lib/keyframes.ts';
	import { offers } from './options.js';
	import { COLLECT_FIELDS_OFFER, standingAt, type Align, type NowActive, type NowListing, type SwitchLabel } from '#lib/fields.ts';
	import { lanes } from '#lib/lanes.ts';
	import { shellJoin, shellSplit } from '#lib/shellSplit.ts';
	import { epoch as written, fields, gestureAt, grammar, here, press, pressWithPills, standings } from './engine.js';

	let {
		switchLabel = 'field',
		/** Where the buttons stand: in rows, or where their fields are drawn (da-omw4n). */
		align = 'rows',
		/**
		 * Whether the ignored fields get a button too (da-xhio) — this
		 * session's option, merged over the tree's settings by the renderer,
		 * so from here it is an ordinary prop like any other.
		 */
		showIgnored = false,
		/** What a tap does: logs what needs nothing more, or opens the field and chooses a state for the command pane (da-u017i, da-gra0z). */
		clicks = 'one',
		/** Which edge the strip and the active section stand at (da-lw7ue). */
		side = 'left',
		/** How large the rows are drawn, which a pinch moves (da-l1z6h); `onchange` hands it to the session. */
		zoom = 1,
		/** The fields shown, or every field where empty (da-gfy7w). */
		only = [],
		epoch = 0,
		onchange
	}: {
		only?: string[];
		switchLabel?: SwitchLabel;
		align?: Align;
		showIgnored?: boolean;
		clicks?: 'one' | 'two';
		side?: 'left' | 'right';
		zoom?: number;
		epoch?: number;
		onchange?: (zoom: number) => void;
	} = $props();

	/** The selected instant, or null at now: a length ending now selects no time. */
	const at = $derived($pointed && $pointed.rel !== 0 ? +$pointed.at : null);
	/** The day the strip draws: the selected instant's, else today. */
	const today = $derived.by(() => {
		void $minute;
		return dateBack(at === null ? 0 : daysBack(at));
	});
	/** `now --json -t <at>`'s active fields, and the instant and epoch they answer. */
	let then = $state.raw<{ at: number; epoch: number; active: NowActive[] } | null>(null);
	/**
	 * How long a newly selected instant rests before `now -t` is asked for it
	 * (da-63yb6): a time pill's drag, or the nowview's, moves it at every
	 * step, and each ask is a CLI process queued ahead of real work. A write
	 * at the instant already answered asks at once.
	 */
	const SETTLE_MS = 150;
	$effect(() => {
		const [t, e] = [at, epoch];
		if (t === null) return void (then = null);
		const ask = () =>
			void read(['now', '--json', '-t', new Date(t).toISOString()]).then((r) => {
				if (r.code !== 0 || at !== t || epoch !== e) return;
				try {
					then = { at: t, epoch: e, active: (JSON.parse(r.stdout) as NowListing).active ?? [] };
				} catch {
					// The bar stays as it was.
				}
			});
		if (untrack(() => then?.at) === t) return ask();
		const resting = setTimeout(ask, SETTLE_MS);
		return () => clearTimeout(resting);
	});
	let layer = $state.raw<Layer>();
	/** The epoch the layer drawn was asked at. */
	let shown = $state(-1);
	$effect(() => {
		const [date, at] = [today, epoch];
		untrack(() => {
			if (!layer) void standIn(date).then((held) => held && !layer && (layer = held));
			void layers([date], at)[0].then((got) => {
				if (date !== today || at !== epoch) return;
				if (!got.error) [layer, shown] = [got, at];
			});
		});
	});
	const doc = $derived(layer?.doc());
	/** The buttons at the selected time by the last `now -t` answered, kept while a moving instant rests, else as they stand now. */
	const standing = $derived.by(() => {
		const all = at !== null && then ? standingAt($fields, then.active, momentsAt(doc, then.at)) : $fields;
		return only.length ? all.filter((f) => only.includes(f.field)) : all;
	});
	$effect.pre(() => {
		const named = [...new Set([...$fields.map((f) => f.field), ...only])];
		const offered = { [COLLECT_FIELDS_OFFER]: named.map((field) => ({ value: field, label: field })) };
		untrack(() => offers.update((o) => ({ ...o, ...offered })));
	});
	// In 2-click mode a tap opens the field, and its screen leads with deleting the entry there (da-s8gr7).
	$effect(() => {
		here.set(new Map(clicks === 'two' ? standing.filter((b) => b.here).map((b) => [b.field, b.here!]) : []));
		return () => here.set(new Map());
	});

	/**
	 * A press under the pending command runs the LINE with the field after it
	 * — `moment -q ` and coffee is `moment -q coffee` — and spends it
	 * (da-bbmpx); a line `flagLine` refuses keeps today's `moment coffee`.
	 */
	function run(argv: string[]) {
		if (argv[0] === 'delete') return void press(argv).then((r) => r.code === 0 && dropHids(argv.slice(1)));
		const line = pendingFlags(argv);
		if (line) $text = '';
		else clearPending();
		const sent = line ? [...line, ...argv.slice(1)] : argv;
		const at = gestureAt() ?? Date.now();
		const mark = pendingOf(withPills($grammar, sent, $pills).argv, $fields, at);
		const settle = mark && tapped(mark);
		void pressWithPills(sent).then((result) => settle?.(result.code === 0 ? get(written) : null));
	}
	const nowPending = $derived(unheld($taps, shown));

	/** The pending line's flags, when `argv` spends its command and it holds only flags. */
	const pendingFlags = (argv: string[]): string[] | null =>
		argv[0] === $pending ? flagLine(shellSplit($text), $grammar?.get(argv[0])) : null;

	/**
	 * The same line, left unrun and straight into its first key's frame
	 * (da-31d1d). The flags go AFTER the field, where the CLI takes them as
	 * well: the frames read the field as the second word. The command is the
	 * word typed, as a run's is: `start ` typed fills `start shift `.
	 */
	function fill(argv: string[]) {
		const [command, ...flags] = pendingFlags(argv) ?? argv.slice(0, 1);
		const line = shellJoin([command, ...argv.slice(1), ...flags]);
		prefill.set({ line: advance(line, $fields) ?? `${line} `, keyboard: true });
	}
</script>

<div class="body">
	<CollectWidget
		fields={standing}
		pending={$pending}
		duration={hasDuration($pills)}
		rank={$lanes}
		standings={$standings}
		{align}
		{switchLabel}
		{showIgnored}
		{clicks}
		{side}
		{zoom}
		onzoom={onchange}
		nowDoc={doc}
		nowAt={at}
		{nowPending}
		onrun={run}
		onfill={fill}
	/>
</div>

<style>
	.body {
		flex: 1;
		min-height: 0;
		display: flex;
		padding: 0.5rem 0.6rem;
	}
</style>
