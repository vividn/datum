<!--
	The welcome widget (da-u7vz8): what a first visit opens on. vividn,
	2026-10-08: «a welcome widget as first tab that explains what datum is,
	some basic ideas to be aware of, and a basic user flow», and, the tab
	being closeable like any other, «maybe the welcome widget could explain
	how to close tabs, so that users know how to close it and also
	manipulate tabs and widgets in general».

	Since da-uzyih.19 it is short (WelcomeText.svelte): the name, its
	tagline, the tutorial, the line for an AI agent, the docs and the
	source, on a phone's screen without scrolling. What it taught (fields,
	shapes, the input bar, tabs and widgets) is the tutorial's first tabs.

	The button closes the widget (da-zhz0u). vividn, 2026-10-09: «maybe
	start unlocked. Let's put a button at the bottom of the welcome widget
	that closes the widget too … I am imagining that the welcome widget will
	have some scrolling and reading rather than all fitting on one page».
	So the button stands after the text, where the reading ends. It is the
	frame's own close (PaneFrame.svelte's `close`), which on a root tab's one
	pane is the root tab's, in either mode: not on the page's last tab, not
	in a place's modal.
-->
<script lang="ts">
	import { getContext } from 'svelte';
	import type { Readable } from 'svelte/store';
	import { act } from '#lib/act.ts';
	import Pane from '#lib/Pane.svelte';
	import WelcomeText from './WelcomeText.svelte';

	const close = getContext<{ close: Readable<(() => void) | null> } | undefined>('layout-frame')?.close;
</script>

<Pane title="welcome">
	<WelcomeText />
	{#if $close}
		<button type="button" class="command" use:act={'run'} onclick={$close}>Close welcome</button>
	{/if}
</Pane>
