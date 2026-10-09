<!--
	The welcome widget (da-u7vz8): what a first visit opens on. vividn,
	2026-10-08: «a welcome widget as first tab that explains what datum is,
	some basic ideas to be aware of, and a basic user flow», and, the tab
	being closeable like any other, «maybe the welcome widget could explain
	how to close tabs, so that users know how to close it and also
	manipulate tabs and widgets in general».

	So it is read in about half a minute, top to bottom: what datum is, a
	field and an entry in a sentence each, the four kinds as the Track form
	names them (`VERBS`, so the two cannot come to disagree), the flow, the
	one button, and how tabs and widgets are handled. That last part says
	the padlock before the two acts it stands in front of, because a layout
	that ships opens locked (da-gshh) and a locked tab has no «Close tab» to
	find; and that the + brings back a tab that was closed, this one too.

	The button is the field widget's door (`openField`): the Track tab
	while it is open, the place every other door reaches once it is closed.
-->
<script lang="ts">
	import { act } from '$lib/act.js';
	import { openField } from '$lib/fieldWidget.js';
	import Pane from '$lib/Pane.svelte';
	import { VERBS } from '$lib/track.js';
</script>

<Pane title="welcome">
	<div class="welcome">
		<h3>datum</h3>
		<p>A record of your life, kept on your device. No account.</p>
		<p>A <b>field</b> is one thing you track: mood, coffee, sleep. An <b>entry</b> is one time you logged it.</p>
		<p>Four kinds:</p>
		<ul class="kinds">
			{#each VERBS as { verb, glyph, what } (verb)}
				<li><span aria-hidden="true">{glyph}</span> {what}</li>
			{/each}
		</ul>
		<ol>
			<li>Choose a few things to track.</li>
			<li>Log them: tap, or type in the bar below.</li>
			<li>Watch your day and your stats fill in.</li>
		</ol>
		<button type="button" class="command" use:act={'open'} on:click={() => openField()}>Choose what to track</button>
		<h3>Tabs and widgets</h3>
		<p>Each tab is a page of panes, each holding a widget.</p>
		<ul>
			<li>Move a tab: drag it. On a phone, hold it first.</li>
			<li>Open a widget, or a tab you closed: the + at the top.</li>
		</ul>
		<p>The pages datum comes with start locked. The padlock, top right, unlocks the one in front; then</p>
		<ul>
			<li>Close a tab, this one too: ▾ beside the +, Close tab.</li>
			<li>Split a pane: ▾ on its bar, Split.</li>
		</ul>
	</div>
</Pane>

<style>
	/* Read, not scanned: a measure a sentence fits, wrapped like the Help place's. */
	.welcome {
		max-inline-size: 34rem;
		color: var(--ink);
		line-height: 1.45;
	}

	h3 {
		margin: 1rem 0 0.4rem;
		color: var(--ink-bright);
		font-size: 0.85rem;
	}

	h3:first-child {
		margin-block-start: 0;
		font-size: 1rem;
	}

	p,
	ul,
	ol {
		margin: 0 0 0.5rem;
	}

	ul,
	ol {
		padding-inline-start: 1.4rem;
	}

	b {
		color: var(--ink-bright);
	}

	.kinds {
		list-style: none;
		padding: 0;
	}

	.kinds span {
		display: inline-block;
		inline-size: 1.4rem;
		color: var(--accent-bright);
	}
</style>
