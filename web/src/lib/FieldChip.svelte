<!--
	One field's collect button (da-w6437), wherever it is drawn: in the collect
	widget's rows, and alone on a text widget's line (`act:field/<name>`,
	da-uzl16), so the two are one button, its shape by kind and its fill by
	state (`lookOf` in fields.ts). What a press and a hold DO is the caller's;
	the hold is found by `data-field` (#lib/hold.ts).
-->
<script lang="ts">
	import { act, type ActKind } from './act.js';
	import type { FieldButton, Look } from './fields.js';

	let {
		button,
		look,
		kind,
		pilled = false,
		disabled = false,
		title,
		style = '',
		onclick
	}: {
		button: FieldButton;
		look: Look;
		/** What a press does (`pressKind`), which draws its outline. */
		kind: ActKind;
		/** Its pill is on the bar (da-8810n). */
		pilled?: boolean;
		disabled?: boolean;
		title: string;
		/** More of its style: a height the picture gives it. */
		style?: string;
		onclick: () => void;
	} = $props();

	/** The look as the custom properties the button is painted from. */
	const vars = $derived(`--fill: ${look.fill ?? 'transparent'}; --line: ${look.line}; --text: ${look.text}`);
</script>

{#if button.shape.kind === 'record'}
	<!-- A PRESS runs nothing: the line goes to the bar's prompt. The
	     HOLD is every field's (da-8810n) — a record is a field like
	     any other, and only what a press MEANS is its own. -->
	<button
		class="record"
		use:act={kind}
		class:ignored={button.ignored}
		class:pilled
		role="link"
		data-field={button.field}
		data-ignored={button.ignored ? '' : undefined}
		data-pilled={pilled ? '' : undefined}
		style={vars + style}
		{disabled}
		{title}
		{onclick}>{button.field}</button
	>
{:else}
	<button
		class="widget {button.shape.kind.replace('/', '')}"
		use:act={kind}
		class:on={look.fill !== null}
		class:ignored={button.ignored}
		class:pilled
		style={vars + style}
		{disabled}
		data-field={button.field}
		data-ignored={button.ignored ? '' : undefined}
		data-pilled={pilled ? '' : undefined}
		{title}
		{onclick}
	>{#if look.glyph}<span class="glyph" aria-hidden="true">{look.glyph}</span>{/if}{look.label}{button.shape.kind === 'change' ? ':' : ''}</button>
{/if}

<style>
	/* Never wider than the pane (da-hygvc): a longer name is cut, and the
	   rest of it is not left under the pane beside, which a tap there
	   reaches instead. */
	.widget,
	.record {
		max-width: 100cqw;
		overflow: hidden;
		text-overflow: ellipsis;
		white-space: nowrap;
		/* A held press must not also select the label or raise the platform's
		   text callout, on touch least of all. */
		user-select: none;
		-webkit-user-select: none;
		-webkit-touch-callout: none;
		touch-action: manipulation;
		min-height: 24px;
		font: inherit;
		cursor: pointer;
	}

	/* Shape by kind, fill by state (lookOf in fields.ts): the colours
	   arrive as --fill / --line / --text. */
	.widget {
		background: var(--fill);
		border: 1.5px solid var(--line);
		border-radius: 3px;
		padding: 0.35rem 0.85rem;
		color: var(--text);
		font-size: 0.85rem;
	}

	.widget.moment {
		border-radius: 999px;
	}

	/* The state the field is in now: bold. */
	.widget.change.on {
		font-weight: 700;
	}

	/* A point's mark (da-h2io6): what an occur of it draws, in its colour. */
	.glyph {
		margin-right: 0.35em;
		color: var(--line);
	}

	.widget:hover:not(:disabled) {
		filter: brightness(1.2);
	}

	.record {
		background: none;
		border: none;
		padding: 0.35rem 0.3rem;
		color: var(--ink);
		font-size: 0.85rem;
		text-decoration: underline 2px var(--line);
		text-underline-offset: 0.3em;
	}

	/* The field whose PILL is on the bar (da-8810n): a ring in the app's
	   ink, so a reader can see where the pill came from. In the ink rather
	   than in the field's colour, because the button is already drawn in
	   that and a ring of it would read as part of the shape. */
	.widget.pilled,
	.record.pilled {
		box-shadow: 0 0 0 2px color-mix(in srgb, var(--ink-bright) 55%, transparent);
	}

	/* A record has no box of its own, so its ring needs one to sit on. */
	.record.pilled {
		border-radius: 3px;
	}

	/* An IGNORED field, drawn because `showIgnored` asked for it (da-xhio).
	   It keeps its colour and its shape — it is still that field — and is
	   faded and dashed, which is how the widget already says «not kept»
	   (`.track-this`). A tracked field is never either, so the two cannot
	   be read for one another. */
	.widget.ignored,
	.record.ignored {
		opacity: 0.45;
	}

	.widget.ignored {
		border-style: dashed;
	}

	.record.ignored {
		text-decoration-style: dashed;
	}

	/* A button a tap RUNS (`pressKind`, da-z8z40) is drawn in a DOUBLE
	   outline, as the commands widget's are (#lib/act.ts) — in the
	   field's own colour, since that is already the button's line. The
	   padding gives back what the wider border takes. The fill stops at
	   the border (da-0lmb7): a double's gap shows the background, and a
	   button FILLED in its own line's colour would paint the two lines and
	   the gap as one edge. Clipped, the gap shows the bar behind it, as it
	   does on a button that is not filled. */
	.widget:global([data-act='run']):not(.ignored) {
		border: 3px double var(--line);
		background-clip: padding-box;
		padding: calc(0.35rem - 1.5px) calc(0.85rem - 1.5px);
	}

	.record:global([data-act='run']):not(.ignored) {
		border: 3px double var(--line);
		border-radius: 3px;
		padding: calc(0.35rem - 3px) calc(0.3rem - 3px);
	}

	/* An IGNORED field that runs keeps its dashes, which are what tell it
	   from a tracked one (da-xhio), and draws them DOUBLE (da-nmna0): CSS
	   has no double-dashed border, so the second line is a dashed outline
	   a border's gap inside the dashed border, following its radius. It
	   gives way to the focus ring, which is an outline too. */
	.widget.ignored:global([data-act='run']):not(:focus-visible),
	.record.ignored:global([data-act='run']):not(:focus-visible) {
		outline: 1.5px dashed var(--line);
		outline-offset: -4.5px;
	}

	.record.ignored:global([data-act='run']) {
		border: 1.5px dashed var(--line);
		border-radius: 3px;
		padding: calc(0.35rem - 1.5px) calc(0.3rem - 1.5px);
	}

	.widget:disabled,
	.record:disabled {
		opacity: 0.4;
		cursor: default;
	}
</style>
