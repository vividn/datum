<!--
	The custom widget: a pane whose command the reader types (da-wb2n) — a
	prototype, on the fixed grid, of a pane that can run any datum command.

	It is a CliPane whose argv is state rather than a constant: ✎ in the
	title bar opens the command for editing, Enter applies it, Escape leaves
	it be. The output is the raw CLI's on purpose, `tail` included — the
	table is the tail pane's. A command that writes runs once and then only
	on ▶ (`guard` in CliPane.svelte).

	The state is the command line, one string in localStorage, and beside it
	the line last seen to write — so a restored command is on ▶ from the
	start instead of writing once per reload to find out (da-v36y). It can
	become a widget setting when there is a layout to hold one.
-->
<script lang="ts">
	import { onMount, tick } from 'svelte';
	import CliPane from './CliPane.svelte';
	import { commandArgv, type PaneSpec } from './panes.js';

	export let epoch = 0;

	// The keys predate the name: the kind was `command` until da-zqop
	// renamed it `custom`, and a reader's saved line must survive that.
	const STORE_KEY = 'datum.command-pane.v1';
	const WRITES_KEY = 'datum.command-pane.writes.v1';
	const HINT = 'type a datum command, e.g. tail -n 5 — nowview runs as nowview -o -, the picture';

	/** The command as typed. */
	let line = '';
	let spec = specFor(line);
	/** The line being edited, or null when the title bar shows the command. */
	let draft: string | null = null;
	let edit: HTMLButtonElement | undefined;

	// A fresh object every time, the same line re-applied included: a new
	// spec is what tells CliPane it is a new command, due one run.
	function specFor(line: string): PaneSpec {
		return { title: line || 'custom', argv: commandArgv(line), empty: line ? 'no output' : HINT };
	}

	function apply(next: string): void {
		line = next;
		spec = specFor(line);
	}

	/** The spec CliPane has seen write; see `guard` there. */
	let writer: PaneSpec | null = null;

	// A store the browser blocks throws on the property itself; the pane
	// then starts empty and keeps its command for the visit.
	onMount(() => {
		try {
			const saved = localStorage.getItem(STORE_KEY);
			if (saved) apply(saved);
			if (saved && localStorage.getItem(WRITES_KEY) === saved) writer = spec;
		} catch {
			/* blocked: nothing to restore */
		}
	});

	function save(key: string, value: string): void {
		try {
			localStorage.setItem(key, value);
		} catch {
			/* blocked: it lasts the visit */
		}
	}

	$: if (writer === spec) save(WRITES_KEY, line);

	async function close(next: string | null): Promise<void> {
		if (next !== null) {
			apply(next.trim());
			save(STORE_KEY, line);
		}
		draft = null;
		await tick();
		edit?.focus();
	}

	// The key is spent here: focus is back on ✎ before the browser gets to
	// the Enter's keypress, which would press it and open the editor again.
	function key(event: KeyboardEvent): void {
		if (event.key !== 'Enter' && event.key !== 'Escape') return;
		event.preventDefault();
		void close(event.key === 'Enter' ? draft : null);
	}

	const focus = (el: HTMLInputElement) => el.focus();
</script>

<CliPane {spec} {epoch} guard bind:writer>
	<svelte:fragment slot="title">
		{#if draft === null}
			{spec.title}
			<button
				type="button"
				class="edit"
				title="edit the command"
				aria-label="edit the command"
				bind:this={edit}
				on:click={() => (draft = line)}
				>✎</button
			>
		{:else}
			<input
				class="command"
				aria-label="datum command"
				placeholder="tail -n 5"
				spellcheck="false"
				autocomplete="off"
				bind:value={draft}
				on:keydown={key}
				use:focus
			/>
		{/if}
	</svelte:fragment>
</CliPane>

<style>
	.edit {
		font: inherit;
		color: inherit;
		background: none;
		border: none;
		padding: 0 0.25rem;
		cursor: pointer;
	}
	/* Inside the title's row, whose outline is the field's (da-om9v): no
	   line of its own, and nothing across the row but the words. */
	.command {
		box-sizing: border-box;
		width: 100%;
		padding-block: 0;
		font: inherit;
		font-family: ui-monospace, SFMono-Regular, Menlo, Consolas, monospace;
		color: var(--ink);
		background: var(--page);
		border: none;
		border-radius: 4px;
	}
</style>
