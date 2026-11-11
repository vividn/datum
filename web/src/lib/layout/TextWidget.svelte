<!--
	The text widget (da-12308). vividn, 2026-11-11: «Let's make a "text"
	widget, it's normally static text, but during modify mode, you can enter
	in markdown text that gets formatted when not in modify mode».

	Out of modify mode its `text`, drawn by #lib/markdown.ts as the docs are,
	read-only; in it a textarea of the source, each keystroke an `onchange`
	that `modifyWidget` (mode.ts) writes into the layout, so ✓ keeps it, ↺
	takes it back and the tab's Save saves and syncs it.

	Its images load from datum.day's own origin alone, the bucket the site and
	its pictures are published in (deploy.yml); any other is a link to it, so a
	layout synced to someone else carries no image from a host that learns of
	each view (da-ipk3v).

	A link to one of datum's actions (#lib/actions.ts, da-uzl16) is a button
	that does it, the reader's tap its yes, and `act:field/<name>` that
	field's collect button itself (#lib/wireActions.ts).
-->
<script lang="ts">
	import Pane from '#lib/Pane.svelte';
	import { markdown, OUTWARD, type Targets } from '#lib/markdown.ts';
	import { wireActions } from '#lib/wireActions.ts';

	interface Props {
		text?: string;
		modifying?: boolean;
		onchange?: (moved: { text: string }) => void;
	}

	let { text = '', modifying = false, onchange }: Props = $props();

	const TARGETS: Targets = { ...OUTWARD, image: (src) => (new URL(src, location.href).origin === location.origin ? src : null), act: () => true };

	const html = $derived(markdown(text, TARGETS));
</script>

<Pane title="text">
	{#if modifying}
		<textarea
			aria-label="the text, in markdown"
			placeholder={'# A heading\nSome *markdown*: lists, **bold**, [links](https://datum.day)'}
			value={text}
			oninput={(e) => onchange?.({ text: e.currentTarget.value })}
		></textarea>
	{:else if text.trim()}
		<div class="text" use:wireActions={html}>{@html html}</div>
	{:else}
		<p class="quiet">A text widget: Modify, in its menu, to write its markdown.</p>
	{/if}
</Pane>

<style>
	textarea {
		box-sizing: border-box;
		inline-size: 100%;
		block-size: 100%;
		min-block-size: 6rem;
		resize: none;
		padding: 0.4rem;
		border: 1px solid var(--rule);
		border-radius: 4px;
		background: var(--ground);
		color: var(--ink);
		font-family: ui-monospace, SFMono-Regular, Menlo, Consolas, monospace;
		font-size: 0.85rem;
	}
	.text {
		color: var(--ink);
		line-height: 1.45;
		overflow-wrap: anywhere;
	}
	.text :global(:is(h1, h2, h3, h4)) {
		margin: 0.8rem 0 0.4rem;
		color: var(--ink-bright);
	}
	.text :global(:is(p, ul, ol, pre, table)) {
		margin: 0 0 0.5rem;
	}
	.text :global(:is(ul, ol)) {
		padding-inline-start: 1.4rem;
	}
	.text :global(strong) {
		color: var(--ink-bright);
	}
	.text :global(a) {
		color: var(--accent-bright);
	}
	.text :global(code) {
		font-family: ui-monospace, SFMono-Regular, Menlo, Consolas, monospace;
		font-size: 0.9em;
	}
	.text :global(.refused) {
		color: var(--quiet);
		font-style: italic;
	}
	.text :global(img) {
		max-inline-size: 100%;
	}
</style>
