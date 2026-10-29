<!--
	One document of a command's result (da-xfa4), as the bottom bar's
	status line shows it and as the CLI widget and the `+N` modal show it.

	A FRAME OF ITS OWN inside its command's (da-hxjrk), labelled on its top
	border with the shape of what the entry stores and its hid. vividn,
	2026-10-29: «for each entry, give it a smaller frame inside the cli
	commands frame. Label this frame with the action and the hid … then put
	field/state indicator on the first line, time on the second line, extra
	data below that.» The shape is the CLI's six characters (`shape`:
	HAPPEN, ON/OFF, SWITCH, MOMENT, RECORD), painted in the colour of what
	the command did to it (`action_color`), whose word is the label's title.

	The data is under the lines now, not beside them in a brace (da-f4mw):
	every key where there is room to scroll (`all`), and in the bar's fixed
	band the CLI's inline join on one line, cut where it ends.

	Every string here is datum's own, out of `--show json` (#lib/echo.ts):
	the web places them and lets CSS truncate them, and composes none of
	them (docs/web-conventions.md #6).

	The colours are datum's own too (da-yu9s): the info line arrives as
	`info_segments` — the runs the terminal would have painted, each with
	its hex — so the line is PLACED painted rather than searched for a
	field name (`switch mood mood` has it twice, and only the CLI knows
	which is which). A document without them falls back to the plain line.

	The full id is behind the hid — vividn: "don't show full id (rather
	showing that on hover over the hid)" — as a `title` for a pointer and,
	because touch has no hover, as a TAP that swaps the hid for the id
	until it is tapped again.
-->
<script lang="ts">
	import { splitFill } from '#lib/ansi.ts';
	import type { EchoDoc, InfoSegment } from '#lib/echo.ts';
	import { select, selected } from '#lib/bar.ts';
	import { isSelected } from '#lib/selection.ts';
	import { tapped } from '#lib/tap.ts';

	interface Props {
		doc: EchoDoc;
		/** No band to fit — the CLI widget, the sheet: every line whole, every key. */
		all?: boolean;
	}

	let { doc, all = false }: Props = $props();

	let shown = $state(false);

	/**
	 * The line's runs, or the unpainted line as its one run. A run of split
	 * slot cells is a run per cell, since each fills its own (`splitFill`).
	 */
	const segments: InfoSegment[] = $derived(
		(doc.info_segments?.length ? doc.info_segments : [{ text: doc.info, color: null }]).flatMap((s) =>
			s.fg && /^[◢◤]{2,}$/.test(s.text) ? [...s.text].map((text) => ({ ...s, text })) : [s]
		)
	);

	/**
	 * The line cut where the CLI says its time ends (`info_time`, da-7xp82),
	 * and drawn the field and state first, the time under them (da-hxjrk).
	 * The spaces between the two are the terminal's and go: each is a line.
	 */
	const parts: { part: string; runs: InfoSegment[] }[] = $derived.by(() => {
		const time = doc.info_time;
		if (!time || !doc.info.startsWith(time)) return [{ part: 'what', runs: segments }];
		const gap = time.length + (/^ */.exec(doc.info.slice(time.length))?.[0].length ?? 0);
		const cut = (from: number, to: number): InfoSegment[] => {
			let at = 0;
			return segments.flatMap((s) => {
				const text = s.text.slice(Math.max(0, from - at), Math.max(0, to - at));
				at += s.text.length;
				return text ? [{ ...s, text }] : [];
			});
		};
		return [
			{ part: 'what', runs: cut(gap, doc.info.length) },
			{ part: 'when', runs: cut(0, time.length) }
		];
	});

	/** The CLI's `key: value` strings — `data`, not the terminal's join. */
	const rows: string[] = $derived(doc.data ?? []);

	/**
	 * A tap on the document selects its entry (da-r13j); the hid is a
	 * button of its own and keeps its tap. A range runs over the documents
	 * drawn around it — the pane's, the bar's, the sheet's — in the order
	 * they are drawn.
	 */
	function tap(e: MouseEvent): void {
		const el = e.currentTarget as HTMLElement;
		if ((e.target as HTMLElement).closest('button') || !tapped(e)) return;
		const scope = el.closest('dialog, section, .prompt-bar') ?? document.body;
		const order = [...scope.querySelectorAll<HTMLElement>('.doc[data-hid]')].map((d) => d.dataset.hid ?? '');
		select(doc.hid, e, order);
	}
</script>

<!-- svelte-ignore a11y_click_events_have_key_events, a11y_no_static_element_interactions -->
<div
	class="doc"
	class:all
	class:selected={isSelected($selected, doc.hid)}
	data-hid={doc.hid}
	onmousedown={(e) => e.shiftKey && e.preventDefault()}
	onclick={tap}
>
	<div class="receipt">
		<span class="action" style:color={doc.action_color} title={doc.action}>{doc.shape ?? doc.action}</span>
		<span class="dot" aria-hidden="true">·</span>
		<button
			type="button"
			class="hid"
			title={doc.id}
			aria-label="full id {doc.id}"
			aria-expanded={shown}
			onclick={() => (shown = !shown)}>{shown ? doc.id : doc.hid}</button
		>
	</div>
	<!-- Nothing may come between the spans: the line is monospace and
	     laid out with runs of spaces, and a newline in the markup is a
	     space in the DOM — so the hugging here is load-bearing, as it is
	     in AnsiText.svelte. -->
	<div class="info">{#each parts as { part, runs }}<span class={part}
				>{#each runs as segment}{@const fill = splitFill(segment.text, segment.color, segment.fg)}<span
						style:color={segment.color}
						style:background={fill}>{fill ? ' ' : segment.text}</span
					>{/each}</span
			>{/each}</div>
	{#if rows.length}
		<div class="rows">
			{#if all}
				{#each rows as row, i (i)}
					<div class="row">{row}</div>
				{/each}
			{:else}
				<div class="row" title={rows.join('\n')}>{doc.data_inline}</div>
			{/if}
		</div>
	{/if}
</div>

<style>
	/* A FRAME, its label standing on its top border as a fieldset's legend
	   does: the border is the box's, and the label's half above it is the
	   margin, so the frame is drawn through the middle of the label. */
	.doc {
		position: relative;
		min-width: 0;
		margin-top: calc(var(--label) / 2);
		padding: calc(var(--label) / 2) 0.45em 0;
		border: 1px solid var(--tint);
		border-radius: 0.35em;
		font-family: ui-monospace, SFMono-Regular, Menlo, Consolas, monospace;
		font-size: 1rem;
		line-height: 1.35;
		cursor: pointer;
		/* The label's height: its type, a step down, on one line. */
		--label: calc(0.7em * 1.2);
	}

	/* A selected entry (da-r13j): the accent the hid pill is filled with. */
	.doc.selected {
		background: color-mix(in srgb, var(--accent) 22%, transparent);
	}

	/* Each line is one line: the band is a fixed height, so a string too
	   long for it is cut rather than allowed to push the dashboard.
	   `pre`, not `nowrap`: datum lays its info line out with runs of
	   spaces, and a browser that collapses them has changed what the CLI
	   printed. */
	.receipt,
	.what,
	.when,
	.row {
		display: block;
		overflow: hidden;
		text-overflow: ellipsis;
		white-space: pre;
	}

	/* A SURFACE WITH NO BAND TO FIT CUTS NOTHING (da-5iyo). The CLI widget and the
	   sheet have the vertical room the bar's fixed band has not, so a line
	   too long for its column wraps there instead of ellipsizing. That is
	   what lets an entry stay whole in a column narrow enough for two of
	   them on a phone; the bar's row still clips, because a fixed-height
	   band that wrapped would push the dashboard. `pre-wrap`, for the
	   spaces the CLI laid out. */
	.doc.all .what,
	.doc.all .when,
	.doc.all .row {
		overflow: visible;
		text-overflow: clip;
		white-space: pre-wrap;
	}

	/* ON THE BORDER, at its left: the shape and the hid, a step down. Its
	   ground hides the border behind it; `max-width` keeps it inside the
	   frame's corners, where it is cut rather than past them. */
	.receipt {
		position: absolute;
		top: 0;
		left: 0.4em;
		max-width: calc(100% - 0.8em);
		box-sizing: border-box;
		padding: 0 0.25em;
		transform: translateY(-50%);
		font-size: 0.7em;
		line-height: 1.2;
		/* Not a token: the same step between `--quiet` and `--ink` the
		   command's echoed line uses (CliEntry.svelte). */
		color: #8888aa;
		background: var(--ground);
	}

	/* datum's own colour for what the command did (da-ckw3); the page's
	   accent when the module carries none. */
	.action {
		color: var(--accent-bright);
	}

	.hid {
		background: none;
		border: none;
		padding: 0;
		font: inherit;
		color: inherit;
		cursor: pointer;
		text-decoration: underline dotted;
		text-underline-offset: 2px;
	}

	/* Cut at the label's end, not past it: a button is an atomic inline, and
	   one wider than its line is hidden whole, so the full id tapped open in
	   a narrow frame (da-qua5g) left only the `…`. 9ch is the shape, its
	   dot and their spaces. */
	.hid {
		max-width: calc(100% - 9ch);
		overflow: hidden;
		text-overflow: ellipsis;
		vertical-align: top;
	}

	.hid:hover,
	.hid:focus-visible {
		color: var(--ink-bright);
	}

	.info {
		color: var(--ink-bright);
	}

	/* The time under the field and state, as quiet as the time in a row of
	   `tail` beside its field. */
	.when {
		color: var(--ink);
	}

	/* The entry's own data, a step down from its lines and under them. */
	.rows {
		font-size: 0.8em;
		line-height: 1.3;
		color: var(--ink);
	}
</style>
