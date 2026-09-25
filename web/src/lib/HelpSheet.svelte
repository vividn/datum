<!--
	The Help button's answer (da-35f23): what was tapped, explained in a
	modal where a newcomer — the one who asked — cannot miss it, rather
	than in the bar's output section. Mounted once by $lib/help.ts and
	opened again for each question.

	The explanation is TEXT, painted as the CLI painted it: a command's is
	`datum help <command>`'s stdout, a widget's or a pill's is plain prose.
	Kept as text so a later pass can link the terms in it; nothing links
	them yet.
-->
<script lang="ts">
	import { ansiSpans } from './ansi.js';
	import AnsiText from './AnsiText.svelte';
	import Sheet from './Sheet.svelte';

	export let title = '';
	export let text = '';
	export let open = false;
</script>

<Sheet {title} bind:open>
	<pre class="help"><AnsiText spans={ansiSpans(text)} /></pre>
</Sheet>

<style>
	/* Wrapped, unlike the REPL's output: this is read, often on a phone,
	   and nothing beside it depends on its columns. */
	.help {
		margin: 0;
		white-space: pre-wrap;
		overflow-wrap: anywhere;
		font-family: ui-monospace, SFMono-Regular, Menlo, Consolas, monospace;
		font-size: 0.8rem;
		color: var(--ink-bright);
	}
</style>
