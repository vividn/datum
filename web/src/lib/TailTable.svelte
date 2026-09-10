<!--
	`datum tail` as a real table.

	The one pane whose content is a grid, not a block. On a terminal datum
	draws its table by padding every cell to a column width it measured
	against `$COLUMNS`; a browser has no column count, and a pane showing
	that padded text scrolled sideways over spaces while its own columns
	sat unused (da-6ec2).

	So the geometry — and only the geometry — moves to the browser. The
	cells come from `datum tail --json`'s `columns`, which is
	`table_columns_with` in crates/datum-commands/src/render/mod.rs — the
	same cell-building `format_table_with` pads: the same columns in the same order, the same header words
	(including the `field`→`f` shrink), the same `_n` ordinals, the same
	colours, each cell painted with the CLI's own ANSI and rendered
	through `ansiSpans` exactly as a line of REPL output is. Nothing about
	what a cell SAYS is decided here.

	Newest at the bottom, as `datum tail` prints.
-->
<script lang="ts">
	import { ansiSpans } from './ansi.js';
	import { tailListing, type TailColumn, type TailRow } from './tail.js';

	/** Any change re-reads the listing. The pane bumps it after each run. */
	export let epoch = 0;
	/** Shown when the listing is empty. */
	export let empty = '';

	let columns: TailColumn[] = [];
	let rows: TailRow[] = [];
	let error = '';
	/** How many reads have been started, and the newest one on screen. */
	let started = 0;
	let shown = -1;

	$: void refresh(epoch);

	async function refresh(_epoch: number): Promise<void> {
		const at = ++started;
		try {
			const table = await tailListing();
			// Two refreshes can be in flight at once and finish out of
			// order; the loser must not paint the older listing over the
			// newer one. Last started wins, not last to return.
			if (at < shown) return;
			shown = at;
			({ columns, rows } = table);
			error = '';
		} catch (e) {
			if (at < shown) return;
			shown = at;
			error = e instanceof Error ? e.message : String(e);
		}
	}
</script>

{#if shown < 0}
	<p class="quiet">…</p>
{:else if error}
	<p class="error">{error}</p>
{:else if rows.length === 0}
	<p class="quiet">{empty}</p>
{:else}
	<table>
		<thead>
			<tr>
				{#each columns as column}
					<th class={column.key} data-key={column.key}
						>{#each ansiSpans(column.header) as span}<span
								style:color={span.fg}
								style:background={span.bg}
								style:font-weight={span.bold ? 'bold' : null}
								style:opacity={span.dim ? '0.6' : null}>{span.text}</span
							>{/each}</th
					>
				{/each}
			</tr>
		</thead>
		<tbody>
			{#each rows as row, i (row.hid)}
				<tr>
					{#each columns as column}
						<td class={column.key} data-key={column.key}
							>{#each ansiSpans(column.cells[i] ?? '') as span}<span
									style:color={span.fg}
									style:background={span.bg}
									style:font-weight={span.bold ? 'bold' : null}
									style:opacity={span.dim ? '0.6' : null}
									style:text-decoration={span.underline ? 'underline' : null}>{span.text}</span
								>{/each}</td
						>
					{/each}
				</tr>
			{/each}
		</tbody>
	</table>
{/if}

<style>
	table {
		/* Fill the pane and never exceed it: the cells that can wrap do,
		   and the pane does not scroll sideways at any width. */
		width: 100%;
		border-collapse: collapse;
		font: inherit;
		text-align: left;
	}

	th {
		font-weight: normal;
		/* The CLI's header is plain text over the columns; a rule under it
		   does the work the terminal's blank line does. */
		border-bottom: 1px solid #2a2a44;
		padding-bottom: 0.15em;
	}

	th,
	td {
		padding: 0.1em 0.4em 0.1em 0;
		vertical-align: baseline;
	}

	th:last-child,
	td:last-child {
		padding-right: 0;
	}

	/* The columns a terminal keeps on one line, kept on one line: a clock
	   broken across two rows is unreadable, and the handles (`_3`, the
	   short hid) are things you type back verbatim. */
	.time,
	.dur,
	._n,
	.hid {
		white-space: nowrap;
		width: 1px; /* shrink-to-fit; the state column takes the slack. */
	}

	/* Right-aligned, as the terminal right-aligns them on the clock. */
	.time,
	.dur {
		text-align: right;
	}

	/* The two that carry arbitrary text. A long state wraps inside its own
	   cell rather than pushing the table past the pane. */
	.field,
	.state {
		overflow-wrap: anywhere;
	}

	.quiet {
		margin: 0;
		color: #444466;
		font-style: italic;
	}

	.error {
		margin: 0;
		color: #f4645a;
	}
</style>
