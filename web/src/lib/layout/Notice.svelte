<!--
	A quiet notice where a widget would be: what a tab shows when the layout
	it names cannot be inlined — it is already open above it, or nothing is
	saved under that name (`#lib/layout/shell.ts`).
-->
<script lang="ts">
	import Pane from '#lib/Pane.svelte';

	let { settings = {} }: { settings?: Record<string, unknown> } = $props();
	const text = $derived(typeof settings.text === 'string' ? settings.text : '');
</script>

<Pane title="notice">
	<p class="quiet">{text}</p>
</Pane>

<style>
	.quiet {
		margin: 0;
		color: var(--quiet);
		font-style: italic;
	}
</style>
