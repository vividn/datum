<!--
	A field's collect button, embedded where a text widget's markdown says
	`[coffee](act:field/coffee)` (da-uzl16): the collect widget's own button
	(FieldChip.svelte), in its color and state, a tap and a hold doing what
	they do there in 1-click mode. What collect does in its own view, opening
	the field, here puts its pill on the bar and opens the collect widget,
	which opens on the pill's field.
-->
<script lang="ts">
	import { onMount } from 'svelte';
	import FieldChip from './FieldChip.svelte';
	import { fieldPill, prefill, setFieldPill } from './bar.js';
	import { pressKind, type PressKind } from './commands.js';
	import { lookOf, pressArgv } from './fields.js';
	import { holds } from './hold.js';
	import { advance } from './keyframes.js';
	import { shellJoin } from './shellSplit.js';
	import { fields, pressWithPills } from './layout/engine.js';
	import { openPlace } from './layout/shell.js';

	let { name }: { name: string } = $props();

	const button = $derived($fields.find((f) => f.field === name));
	const tap = $derived(button && pressKind(button, null, true));
	const held = $derived(button && pressKind(button, null, false));
	const does = (kind?: PressKind) => (kind === 'run' ? 'log it' : kind === 'fill' ? 'fill the prompt' : 'open it');

	let host: HTMLElement;
	onMount(() => holds(host, { within: 'button[data-field]', run: () => press(held) }).destroy);

	function press(kind?: PressKind) {
		if (!button || !kind) return;
		const argv = pressArgv(button);
		if (kind === 'show') {
			setFieldPill(button.field, button.color);
			void openPlace('widgetbar');
		} else if (kind === 'fill') {
			const line = shellJoin(argv);
			prefill.set({ line: advance(line, $fields) ?? `${line} `, keyboard: true });
		} else void pressWithPills(argv);
	}
</script>

<span class="embed" data-embed="field" bind:this={host}>
	{#if button && tap}
		<FieldChip
			{button}
			look={lookOf(button)}
			kind={tap}
			pilled={$fieldPill === button.field}
			title="{button.field} · press to {does(tap)} · hold to {does(held)}"
			onclick={() => press(tap)}
		/>
	{:else}
		<span class="quiet">{name}: no field of that name</span>
	{/if}
</span>

<style>
	.embed {
		display: inline-flex;
		max-inline-size: 100%;
		vertical-align: middle;
	}
	.quiet {
		color: var(--quiet);
		font-style: italic;
	}
</style>
