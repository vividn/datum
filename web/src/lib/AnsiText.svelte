<script lang="ts">
	/*
	 * One run of CLI output, painted.
	 *
	 * `#lib/ansi.ts` decides what a span IS; this decides how it LOOKS, and
	 * it is the only place that does. The loop used to be written out three
	 * times — `CliOutput` once, `TailTable` twice — and the copies had
	 * already drifted apart in which attributes they set: the tail's header
	 * cell painted no underline, neither tail cell painted italic, so one
	 * escape said different things in the CLI widget and in the table (da-mv90).
	 * What is below is the union: every attribute an `AnsiSpan` carries.
	 * One run is drawn other than as its text: a parallel slot's split cell,
	 * `◢` or `◤` (da-pc7x7), is a space its two colours fill (`splitFill`),
	 * since a font draws the triangle smaller than its cell, or at another width.
	 *
	 * Nothing may come between the spans, or before or after them. The
	 * CLI widget's output and the tail's columns are monospace and column-aligned,
	 * and a newline in the markup is a space in the DOM — so the
	 * `>{#each}`…`{/each}<` hugging at each call site is load-bearing, and
	 * so is the shape of this file: the each-block is the whole template,
	 * and this note is INSIDE the script rather than an HTML comment above
	 * it, because Svelte only trims the leading whitespace of a template
	 * that begins with it. With a comment above, the newline that closes the
	 * script tag survives as a text node and every painted cell would gain a
	 * leading space.
	 */
	import { splitFill, type AnsiSpan } from './ansi.js';

	interface Props {
		/**
		 * The runs to paint, as `ansiSpans()` returned them. Immutable: the
		 * same array is the same runs, so a caller that keeps its spans (the
		 * tail's rows, da-j6frl) is not painted again on every update.
		 */
		spans: AnsiSpan[];
	}

	let { spans }: Props = $props();
</script>

{#each spans as span}{@const fill = splitFill(span.text, span.bg, span.fg)}<span
		style:color={span.fg}
		style:background={fill ?? span.bg}
		style:font-weight={span.bold ? 'bold' : null}
		style:opacity={span.dim ? '0.6' : null}
		style:font-style={span.italic ? 'italic' : null}
		style:text-decoration={span.underline ? 'underline' : null}>{fill ? ' ' : span.text}</span
	>{/each}
