<!--
	The commands widget as a widget kind (da-ltux), `command`.

	Its own body rather than `Pane.svelte`'s, for the padding and nothing
	else: the BAR is the scroller (CommandsWidget.svelte), because the
	scroll rule is about the bar's rows and the axis it scrolls on.

	A tap that RUNS — `Undo`, its redo icon, and whatever the loaded pills
	complete (`fires` in $lib/commands.ts, da-3lxol: `delete` with an hid
	selected) — goes through `pressWithPills`, which hands the command only
	the pills its grammar takes: a selected hid (`undo <hid>`), a time for
	`retime`. Everything else only writes a line into the bottom bar's
	prompt, unrun, for a person to finish. The loaded pills decide which
	buttons are active and in what order (da-r13j). A line whose next step
	is a tap is focused without a phone's keyboard (`next` in
	$lib/commands.ts, da-k34p).

	`track` opens the setup widget on a new field (da-fu29p), and `track`
	or `setup` with a FIELD pill loaded opens it on that field at once
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
<script lang="ts">
	import CommandsWidget from '$lib/CommandsWidget.svelte';
	import { clearUnlockedPills, pills, prefill, text, type Pill } from '$lib/bar.js';
	import { argumentsOf, opensField, withFlag } from '$lib/commands.js';
	import { openField } from '$lib/fieldWidget.js';
	import { run } from '$lib/run.js';
	import { redoArgv } from '$lib/calls.js';
	import { withPills } from '$lib/selection.js';
	import { conflicts, fields, grammar, pressWithPills, ready } from './engine.js';

	export let epoch = 0;
	/** This widget's session key, which its bar keeps what it shows under (da-ria1a). */
	export let key = '';
	/** The widget's «scrolling sections» option (da-u79wl). */
	export let sections = false;

	let redoable = false;
	let asked = 0;

	/** Would a press on redo move anything? The newest question's answer wins. */
	async function ask(argv: string[], _epoch: number): Promise<void> {
		const mine = ++asked;
		const { code } = await run(argv);
		if (mine === asked) redoable = code === 0;
	}

	$: if ($ready) void ask(withPills($grammar, redoArgv(), $pills).argv, epoch);

	function runNow(argv: string[]): void {
		// A thumb that tapped the command before its pills were all loaded
		// has no Enter: the tap that fires takes back the `<name> ` the
		// first one put there.
		if ($text === `${argv[0]} `) $text = '';
		void pressWithPills(argv);
	}

	function fill({ name, line, keyboard, spend, held }: { name: string; line: string; keyboard: boolean; spend?: Pill[]; held?: boolean }): void {
		const field = argumentsOf($pills).find((p) => p.kind === 'field');
		if (!held && opensField(name, $pills)) {
			if (field) clearUnlockedPills([field]);
			// The widget runs its own save; what it answers is the console's.
			void openField(field?.text);
		} else {
			if (spend) clearUnlockedPills(spend);
			prefill.set({ line, keyboard });
		}
	}
</script>

<div class="body">
	<!-- Keyed: the bar's holds are bound to the element it mounted with. -->
	{#key sections}
		<CommandsWidget
			disabled={!$ready}
			{sections}
			grammar={$grammar}
			pills={$pills}
			fields={$fields}
			{redoable}
			conflicts={$conflicts.length}
			{key}
			on:run={(e) => runNow(e.detail)}
			on:line={(e) => fill(e.detail)}
			on:flag={(e) => prefill.set({ line: withFlag($text, e.detail.command, e.detail.flag), keyboard: e.detail.keyboard })}
		/>
	{/key}
</div>

<style>
	.body {
		flex: 1;
		min-height: 0;
		display: flex;
		padding: 0.5rem 0.6rem;
	}
</style>
