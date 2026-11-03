<!--
	The sync guide (da-oyvsz): how to use the Sync widget, in plain words from
	the person's side. vividn, 2026-10-15: «a nice sync widget web help text
	should be made as well to describe how to use it», and the CLI's
	subcommands in an expandable section at the end, not in the body.

	The Help place draws it (#lib/help.ts `GUIDES`): the welcome's «Syncing
	your devices», the Help button on the Sync widget and the widget's own `?`
	open it. Every control it names is a `b.ui` in the words the widget shows,
	and `tests/syncguide.test.mjs` walks the widget and finds each one there.
	`widget` is the kind's help, what the widget's lines and marks mean; the
	reference is `datum help sync`, the CLI's own, so it cannot drift.
-->
<script lang="ts">
	import { onMount } from 'svelte';
	import { act } from '#lib/act.ts';
	import { ansiSpans } from '#lib/ansi.ts';
	import AnsiText from '#lib/AnsiText.svelte';
	import { helpArgv } from '#lib/calls.ts';
	import { run } from '#lib/run.ts';
	import { openPlace } from './shell.js';

	interface Props {
		widget?: string;
	}

	let { widget = '' }: Props = $props();

	let reference = $state('');
	onMount(async () => {
		const { stdout, stderr } = await run(helpArgv('sync'));
		reference = stdout || stderr;
	});
</script>

<div class="guide">
	<p>
		Sync keeps the same data on each of your devices: a phone, a laptop's browser, a computer with the datum command.
		Devices talk to each other directly. What they send is encrypted end to end, so a relay that helps them find each
		other passes on only what it cannot read.
	</p>

	<h3>Where it is</h3>
	<p>
		The gear at the top of the page, then <b class="ui">Sync</b>; or type <code>sync</code> in the input bar.
		<button type="button" class="command" use:act={'open'} onclick={() => void openPlace('sync')}>Open Sync</button>
	</p>

	<h3>Connect a second device</h3>
	<ol>
		<li>
			On the device that has your data, open Sync and press <b class="ui">Turn on sync</b> — or
			<b class="ui">Add device</b>, once sync is on.
		</li>
		<li>
			A QR code appears. Scan it with the other device's camera, or press <b class="ui">Copy link</b> and open the link
			in the other device's browser.
		</li>
		<li>
			The other device asks <b class="ui">Join a sync chain?</b> Give it a name you will recognize and press
			<b class="ui">Join</b>.
		</li>
		<li>Each device now lists the other. Press <b class="ui">Done</b> on the first to put the QR away.</li>
	</ol>
	<p>
		A link adds one device, once, and only until the time it says. For the next device, press Add device again.
		On a computer with the datum command, open <b class="ui">join from a terminal</b> under the QR and run what it
		shows there.
	</p>

	<h3>What syncs</h3>
	<p>
		Everything in the database: entries, fields, aliases, and layouts kept with Save layout. A layout you only
		Accept stays on its device. Each database syncs on its own: Sync shows the one this page is on, and your other
		databases under it.
	</p>
	<p>
		Joining merges: the joining device keeps what it had, and after the first sync both hold everything. Devices
		sync whenever two of them are online at once; a browser does while datum.day is open in it.
	</p>

	<h3>When it says a device joined</h3>
	<p>
		Every other device shows a ⚠ in the tab bar: tap it, and it says «Another device joined», names it and when, and offers
		to expel it should it not be yours. Sync lists it too, with <b class="ui">OK</b>. The device that joined is told
		the chain’s members once, plainly.
	</p>
	<ul>
		<li>One you added: press OK.</li>
		<li>
			One you don't know: in Sync press <b class="ui">More</b>, then <b class="ui">Expel</b> beside it, so it can
			no longer sync. <b class="ui">Unjoin</b> also takes back everything it changed.
		</li>
	</ul>
	<p>
		Give each device a name you will know it by with <b class="ui">Rename this device</b>. More also holds
		<b class="ui">Pause sync</b>, and <b class="ui">Leave chain</b>, which stops this device syncing and keeps its
		data.
	</p>

	{#if widget}
		<h3>Reading the widget</h3>
		<p class="widget">{widget}</p>
	{/if}

	<details class="reference">
		<summary>The sync commands, for a terminal</summary>
		<pre><AnsiText spans={ansiSpans(reference)} /></pre>
	</details>
</div>

<style>
	/* Read like the welcome, whose words are the Help place's front page. */
	.guide {
		max-inline-size: 34rem;
		color: var(--ink);
		line-height: 1.45;
	}

	h3 {
		margin: 1rem 0 0.4rem;
		color: var(--ink-bright);
		font-size: 0.85rem;
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

	.widget {
		white-space: pre-line;
	}

	summary {
		color: var(--accent-bright);
		cursor: pointer;
	}

	pre {
		margin: 0.4rem 0 0;
		white-space: pre-wrap;
		overflow-wrap: anywhere;
		font-size: 0.8rem;
		color: var(--ink-bright);
	}
</style>
