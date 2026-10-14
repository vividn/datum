<!--
	The welcome widget (da-u7vz8): what a first visit opens on. vividn,
	2026-10-08: «a welcome widget as first tab that explains what datum is,
	some basic ideas to be aware of, and a basic user flow», and, the tab
	being closeable like any other, «maybe the welcome widget could explain
	how to close tabs, so that users know how to close it and also
	manipulate tabs and widgets in general».

	So it is read in about half a minute, top to bottom: what datum is, a
	field and an entry in a sentence each, the four kinds as the Track form
	names them (`VERBS`, so the two cannot come to disagree), the flow, its
	button, and how tabs and widgets are handled: that the + brings back
	a tab that was closed, this one too, and that the other pages start
	locked (da-gshh), which is why they show no ▾ until the padlock.

	The first button is the field widget's door (`openField`): the Track
	tab while it is open, the place every other door reaches once it is
	closed.

	The last one closes the widget (da-zhz0u). vividn, 2026-10-09: «maybe
	start unlocked. Let's put a button at the bottom of the welcome widget
	that closes the widget too … I am imagining that the welcome widget will
	have some scrolling and reading rather than all fitting on one page».
	So `welcome` opens unlocked (`tabHolding`, shell.ts), and the button
	stands after the text, where the reading ends. It is the frame's own
	`×` (PaneFrame.svelte's `close`), so it is there exactly where that is:
	not under a lock, not on the page's last tab, not in a place's modal.

	Reworked with vividn on 2026-10-14, section by section. The name is bigger
	(«"datum" can be a bit bigger yes»), the opening says why and who
	holds the data, «state» joins field and entry as a word to know, and the
	four kinds became SHAPES you can see: three timelines of one working
	day, labelled by field and lined up (coffee, screentime, and an activity
	that is mostly email and meetings), and a card (a person, vividn's).
-->
<script lang="ts">
	import { getContext } from 'svelte';
	import type { Readable } from 'svelte/store';
	import { act } from '$lib/act.js';
	import Pane from '$lib/Pane.svelte';
	import WelcomeText from './WelcomeText.svelte';

	const close = getContext<{ close: Readable<(() => void) | null> } | undefined>('layout-frame')?.close;
</script>

<Pane title="welcome">
	<WelcomeText />
	{#if $close}
		<button type="button" class="command" use:act={'run'} on:click={$close}>Close welcome</button>
	{/if}
</Pane>
