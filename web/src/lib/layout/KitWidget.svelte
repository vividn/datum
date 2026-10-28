<!--
	A widget a kit shipped (da-gh37k): the kit's `[[widget]]`, read from its
	own entry in this database (store.ts), is a command and a row action and
	nothing else — no code ships in a kit, since a kit is shared text and
	code in it would run in the reader's page.

	The command runs with `--json` on every epoch, as the custom widget's
	line does, and each row it answers is drawn as the CLI's table prints it
	(`row_cells`) in the colour its rule paints it (`row_color`): the page
	places what the CLI rendered and formats nothing. With no rows, the
	command's own words for that, run without `--json` once per command and
	kept, so an empty list costs one run an epoch as a full one does.

	The command only reads: install refuses one that may write (`WIDGET_READS`
	in `crates/datum-core/src/kit.rs`), since it runs on every change. A
	command that writes all the same — an entry edited past install — is
	stopped by the custom widget's own test, the run's `changed`: two runs
	in a row that each wrote, so the one write a read can make (`stats`
	filling its cache) does not count, and it is not run again. A tap runs the row
	action with each `{column}` that row's value — substituted inside the
	words the action splits into, so a value is one word whatever it holds —
	through the bar's one path, so it lands as a press of the fields widget
	does and the epoch it bumps redraws the list.
-->
<script lang="ts">
	import { untrack } from 'svelte';
	import { browser } from '$app/env';
	import Pane from '#lib/Pane.svelte';
	import { kitWidgetArgv, type KitWidgetRow } from '#lib/calls.ts';
	import { run } from '#lib/run.ts';
	import { shellSplit } from '#lib/shellSplit.ts';
	import { pressWithPills } from './engine.js';
	import { kitWidget, refreshKits, type KitWidget } from './store.js';

	let {
		settings = {},
		epoch = 0
	}: {
		settings?: Record<string, unknown>;
		epoch?: number;
	} = $props();

	const kit = $derived(String(settings.kit ?? ''));
	const name = $derived(String(settings.widget ?? ''));

	let def = $state.raw<KitWidget | null>(null);
	let rows = $state.raw<KitWidgetRow[] | null>(null);
	/** What to show instead of rows: the command's words, or why there are none. */
	let words = $state('');
	/** Reads started and the newest one on screen: last started wins (see CliPane.svelte). */
	let started = 0;
	let shown = -1;
	/** The command whose last run wrote, and the one found writing twice: never run again. */
	let wrote = '';
	let writer = '';
	/** The command's words for no rows, and the command they are for. */
	let empty: { command: string; words: string } | null = null;

	$effect.pre(() => {
		const asked = [kit, name, epoch] as const;
		if (browser) untrack(() => void read(...asked));
	});

	async function read(kit: string, name: string, _epoch: number): Promise<void> {
		const at = ++started;
		// A kit installed since the cache was filled is read in once.
		let found = kitWidget(kit, name);
		if (!found) await refreshKits().then(() => (found = kitWidget(kit, name)));
		if (!found) return show(at, null, null, `no widget «${name}» from a kit «${kit}» is installed in this database`);
		const writes = `«${found.command}» writes, and a widget's command may only read`;
		if (writer === found.command) return show(at, found, null, writes);
		const result = await run(kitWidgetArgv(found.command));
		if (result.changed?.length && wrote === found.command) writer = found.command;
		wrote = result.changed?.length ? found.command : '';
		if (writer === found.command) return show(at, found, null, writes);
		if (result.code !== 0) return show(at, found, null, result.stderr.trim());
		let listed: unknown;
		try {
			listed = JSON.parse(result.stdout);
		} catch {
			return show(at, found, null, result.stdout);
		}
		if (!Array.isArray(listed)) return show(at, found, null, result.stdout);
		if (listed.length) return show(at, found, listed as KitWidgetRow[], '');
		if (empty?.command !== found.command) {
			const plain = await run(shellSplit(found.command));
			empty = { command: found.command, words: plain.code === 0 ? plain.stdout.trim() : '' };
		}
		show(at, found, [], empty.words);
	}

	function show(at: number, found: KitWidget | null, listed: KitWidgetRow[] | null, text: string): void {
		if (at < shown) return;
		shown = at;
		def = found;
		rows = listed;
		words = text;
	}

	const cell = (v: unknown): string => (v === null || v === undefined ? '' : typeof v === 'string' ? v : JSON.stringify(v));

	/** The row action's words, each `{column}` this row's value. */
	const actionOf = (action: string, row: KitWidgetRow): string[] =>
		shellSplit(action).map((word) => word.replace(/\{(\w+)\}/g, (_, column: string) => cell(row[column])));

	const columns = $derived(Math.max(0, ...(rows ?? []).map((r) => r.row_cells.length)));
</script>

<Pane title={def?.title ?? name}>
	<div class="kit-widget" data-kit={kit} data-widget={name}>
		{#if rows?.length}
			<div class="rows" style:--columns={columns}>
				{#each rows as row, i (i)}
					{#if def?.row_action}
						<button type="button" class="row" style:color={row.row_color} onclick={() => void pressWithPills(actionOf(def?.row_action ?? '', row))}>
							{#each row.row_cells as c}<span>{c}</span>{/each}
						</button>
					{:else}
						<div class="row" style:color={row.row_color}>
							{#each row.row_cells as c}<span>{c}</span>{/each}
						</div>
					{/if}
				{/each}
			</div>
		{:else if rows || words}
			<p class="quiet">{words}</p>
		{:else}
			<p class="quiet">…</p>
		{/if}
	</div>
</Pane>

<style>
	/* The table's columns, every row on them: each row is a subgrid. */
	.rows {
		display: grid;
		grid-template-columns: repeat(var(--columns), auto);
		justify-content: start;
		gap: 0.15rem 1ch;
		font-family: monospace;
	}
	.row {
		display: grid;
		grid-column: 1 / -1;
		grid-template-columns: subgrid;
		margin: 0;
		padding: 0.2rem 0.4rem;
		min-block-size: 24px;
		align-items: center;
		text-align: start;
		background: none;
		border: 1px solid transparent;
		border-radius: 4px;
		color: var(--ink);
		font: inherit;
	}
	button.row {
		cursor: pointer;
	}
	button.row:hover,
	button.row:focus-visible {
		border-color: var(--rule);
	}
	.quiet {
		margin: 0;
		color: var(--quiet);
		white-space: pre-wrap;
	}
</style>
