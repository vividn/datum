<!--
	The commands widget as a widget kind (da-ltux), `command`.

	Its own body rather than `Pane.svelte`'s, for the padding and nothing
	else: the BAR is the scroller (CommandsWidget.svelte), because the
	scroll rule is about the bar's rows and the axis it scrolls on.

	A tap that RUNS — `Undo`, its redo icon, and whatever the loaded pills
	complete (`fires` in #lib/commands.ts, da-3lxol: `delete` with an hid
	selected) — goes through `pressWithPills`, which hands the command only
	the pills its grammar takes: a selected hid (`revert <hid>`), a time for
	`retime`. Everything else only writes a line into the bottom bar's
	prompt, unrun, for a person to finish. The loaded pills decide which
	buttons are active and in what order (da-r13j). A line whose next step
	is a tap is focused without a phone's keyboard (`next` in
	#lib/commands.ts, da-k34p).

	The command the pills make READY leads it (da-h3jcu): a tap runs it
	through `pressWithPills` too, which places and spends its field and
	state pills as any press does.

	`setup` opens the setup widget on a new field, and with a FIELD pill
	loaded on that field at once
	(da-ybai9, vividn: «no need for sub commands or pressing enter. It should
	just work»): the pill named the subject, so it is spent at the TAP — not
	on success, as a press spends its pills — because the bead asked that it
	not sit in the bar once the widget is open; leaving it without a save
	leaves it spent too. There is only ever one field pill (`setFieldPill`).
	Without one, `setup ` goes on the line as ever. A HOLD is the bare
	command and its flags, as on any button.

	`occur`/`start`/`end` under a FIELD pill run it (da-lxqqz); when the
	field wants keys typed, the tap writes `occur <field> ` instead — never
	for `end`, which always runs (da-o4jaa) — and
	the pill, now on the line, is spent here.

	A FLAG from a held command's list (da-st2xl) is appended to the line when
	the line is that command's, so taps compose; otherwise it starts the line
	afresh as `<command> <flag> ` (`withFlag`).

	Undo's REDO icon is lit by the CLI (da-zq91a): `redo --dry-run`, with
	the pills a press would carry, re-asked on every epoch — which a write
	in another tab bumps too — so an undo typed anywhere lights it here,
	and the page never re-derives what counts as something to redo.
-->
<script lang="ts" module>
	import { run } from '#lib/run.ts';

	/**
	 * One `redo --dry-run` for every pane asking the same at the same epoch:
	 * every root tab keeps its command pane mounted (TabBody.svelte), and a
	 * write would otherwise ask once per tab (da-h3jcu).
	 */
	let latest: { key: string; answer: Promise<boolean> } | null = null;
	const redoes = (argv: string[], epoch: number): Promise<boolean> => {
		const key = `${epoch} ${JSON.stringify(argv)}`;
		if (latest?.key !== key) latest = { key, answer: run(argv).then(({ code }) => code === 0) };
		return latest.answer;
	};
</script>

<script lang="ts">
	import { untrack } from 'svelte';
	import CommandsWidget from '#lib/CommandsWidget.svelte';
	import { clearUnlockedPills, commandsOpen, pills, prefill, text, type Pill } from '#lib/bar.ts';
	import { argumentsOf, opensField, withFlag, type Axis, type Suggestion } from '#lib/commands.ts';
	import { openField } from '#lib/fieldWidget.ts';
	import { redoArgv } from '#lib/calls.ts';
	import { withPills } from '#lib/selection.ts';
	import { conflicts, fields, grammar, pressWithPills, ready } from './engine.js';

	let {
		epoch = 0,
		/** This widget's session key, which its bar keeps what it shows under (da-ria1a). */
		key = '',
		/** The axis its buttons are reached along: up and down over the bar's output (da-m9zo7). */
		axis = 'x'
	}: {
		epoch?: number;
		key?: string;
		axis?: Axis;
	} = $props();

	let redoable = $state(false);
	let asked = 0;

	/** Would a press on redo move anything? The newest question's answer wins. */
	async function ask(argv: string[], epoch: number): Promise<void> {
		const mine = ++asked;
		const answer = await redoes(argv, epoch);
		if (mine === asked) redoable = answer;
	}

	$effect.pre(() => {
		if (!$ready) return;
		const argv = withPills($grammar, redoArgv(), $pills).argv;
		const at = epoch;
		untrack(() => void ask(argv, at));
	});

	function runNow(argv: string[]): void {
		// A thumb that tapped the command before its pills were all loaded
		// has no Enter: the tap that fires takes back the `<name> ` the
		// first one put there.
		if ($text === `${argv[0]} `) $text = '';
		void pressWithPills(argv);
	}

	/** The ready command (da-h3jcu): it runs, with the pills it takes. */
	const suggest = ({ argv }: Suggestion) => runNow(argv);

	function fill({ name, line, keyboard, spend, held }: { name: string; line: string; keyboard: boolean; spend?: Pill[]; held?: boolean }): void {
		// A choice closes the widget over the output, as a run does (da-xw655);
		// a hold stays, for the flags it opens.
		if (!held) commandsOpen.set(false);
		const field = argumentsOf($pills).find((p) => p.kind === 'field');
		if (!held && opensField(name)) {
			if (field) clearUnlockedPills([field]);
			// The widget runs its own save; what it answers is the console's.
			// A field held, then the setup command: brought, so Save goes back (da-wz84y).
			void openField(field?.text, null, { brought: !!field });
		} else {
			if (spend) clearUnlockedPills(spend);
			prefill.set({ line, keyboard });
		}
	}
</script>

<div class="body">
	<CommandsWidget
		disabled={!$ready}
		{axis}
		grammar={$grammar}
		pills={$pills}
		fields={$fields}
		{redoable}
		conflicts={$conflicts.length}
		{key}
		onrun={runNow}
		onsuggest={suggest}
		online={fill}
		onflag={(flag) => prefill.set({ line: withFlag($text, flag.command, flag.flag), keyboard: flag.keyboard })}
	/>
</div>

<style>
	.body {
		flex: 1;
		min-height: 0;
		min-width: 0;
		display: flex;
		padding: 0.5rem 0.6rem;
	}
</style>
