<!--
	The welcome's words (da-u7vz8), apart from its frame, so the Help place
	can show them as its front page while nothing is explained (vividn,
	2026-10-14). Short since da-uzyih.19, vividn, 2026-10-22: «It can have the
	first bit, a link to the tutorial, and then a link to further
	documentation for those who prefer docs instead of tutorials, but it
	should mostly just welcome you, give a tag line, tell you to point your
	agent there, maybe link to the repo? and give docs». What it taught is
	the tutorial's and docs/basics.md's now, but for local first and
	what a field is, which came back (da-uzyih.4869, vividn, 2026-10-27:
	«The track what matters to you paragraph can go back on the main welcome
	page»).
-->
<script lang="ts">
	import { browser } from '$app/env';
	import { act } from '#lib/act.ts';
	import { labelCheckArgv, renameArgv } from '#lib/calls.ts';
	import { deviceName } from '#lib/clientId.ts';
	import { currentDb } from '#lib/database.ts';
	import { cliError } from '#lib/run.ts';
	import { suggestedLabel } from '#lib/sync.ts';
	import { newInvite, readSync, syncAct } from '#lib/syncView.ts';
	import { openTutorial } from '#lib/tutorial.ts';
	import Sheet from '#lib/Sheet.svelte';
	import AgentForm from './AgentForm.svelte';
	import { open, openPlace } from './shell.js';

	const REPO = 'https://github.com/vividn/datum';
	const GUIDE = 'https://datum.day/agents.md';
	/** Short: the guide does the teaching. */
	const PROMPT = `I track my life data with datum (https://datum.day). Read ${GUIDE} first, then help me with: `;

	let ai = $state(false);
	let copied = $state('');
	/** «Generate an AI code» unfolded (da-f7orx): whether the page's database is in a sync chain, and one with an admin. */
	let agent = $state<{ chained: boolean; locked: boolean } | null>(null);
	const db = browser ? currentDb() : '';

	const generate = async () => {
		const [d] = await readSync([db]);
		agent = { chained: !!d?.chain, locked: !!d?.admins?.length };
	};

	/**
	 * An agent's code needs a chain: a database in none starts one, as it is
	 * first shared, with the name kept under ✎ (da-5wlnr). Only an invite
	 * starts one, and this one is never shown, so it admits no one: a
	 * reader's, so a secret recovered would admit no more (da-ca5r3).
	 */
	async function chain(): Promise<void> {
		if (!agent || agent.chained) return;
		const ok = async (argv: string[]) => {
			const r = await syncAct(db, argv);
			if (r.code !== 0) throw cliError(r.stderr);
		};
		const name = deviceName();
		const named = name && name !== suggestedLabel(db) ? name : undefined;
		if (named) await ok(labelCheckArgv(named));
		await newInvite(db, true);
		// Started, whatever the rename does: a retry makes no second invite.
		agent.chained = true;
		if (named) await ok(renameArgv(named));
	}

	const copy = () =>
		(navigator.clipboard?.writeText(PROMPT) ?? Promise.reject()).then(
			() => (copied = 'Copied.'),
			() => (copied = 'This browser did not let the page copy it: select the prompt instead.')
		);
</script>

<div class="welcome">
	<h1 class="name">datum</h1>
	<p class="motto">Record your life, cultivate awareness, and confront the truth.</p>
	<p>Track what matters to you. Pay attention, record it here, and see your days as they really were.</p>
	<!-- vividn, 2026-11-10: «Above the Tutorial, let's add a filled in button. "track something" that redirects you to
	     the setup tab.» (da-a6xba) -->
	<p>
		<button type="button" class="command track" data-open="setup" use:act={'open'} onclick={() => void open('setup')}>Track something</button>
	</p>
	<p>New here? Learn how by playing in the tutorial db</p>
	<!-- The way into the tutorial (da-6cvrd), a database of its own: practice never lands in yours. -->
	<p>
		<button type="button" class="command" data-open="tutorial" use:act={'open'} onclick={openTutorial}>Tutorial</button>
	</p>
	<!-- The docs, read in the page (da-0d8l6): the docs place, on their contents. -->
	<p>Rather read than practice?</p>
	<p>
		<button type="button" class="command" data-open="docs" use:act={'open'} onclick={() => void openPlace('docs')}>Read the docs</button>
	</p>
	<!-- vividn, 2026-10-21: «It should be clear on the front page of datum.day that you can point your agent there
	     to help you get setup with your data.» Since 2026-11-12 a button, vividn: «replace the ai section with an
	     "integrate with AI" button that pops up a modal with an explanation as well as … a copyable prompt to paste
	     into a chat ai» (da-rlxux). The guide it names is /agents.md, the public one, since the AI is not on this
	     device. «Generate an AI code» makes an agent's code with the Sync widget's own form (da-f7orx). -->
	<p>
		<button type="button" class="command" data-open="ai" use:act={'open'} onclick={() => (ai = true)}>Integrate with AI</button>
	</p>
	<Sheet title="Integrate with AI" bind:open={ai} onclose={() => ((copied = ''), (agent = null))} data-ai>
		<p>
			An AI can help you set up what to track, analyze your data, or build connectors to other data sources. Its guide is
			<a href={GUIDE} target="_blank" rel="noopener">datum.day/agents.md</a>. AI agents love collecting data on themselves too.
		</p>
		<p>Paste this into a chat AI, then say what you want help with:</p>
		<pre class="prompt">{PROMPT}</pre>
		<p>
			<button type="button" class="command" use:act={'run'} onclick={copy}>Copy prompt</button>
			<span class="quiet" role="status">{copied}</span>
		</p>
		<p>Or give an agent a copy of your data to work in: a code it restores datum from each time it starts, as «Add agent» in Sync makes.</p>
		{#if agent}
			{#if !agent.chained}<p class="quiet">This database syncs with nothing yet: making a code starts its sync chain first, as sharing it would.</p>{/if}
			<AgentForm {db} locked={agent.locked} before={chain} onclose={() => (agent = null)} />
		{:else}
			<p><button type="button" class="command" use:act={'show'} onclick={generate}>Generate an AI code</button></p>
		{/if}
	</Sheet>
	<!-- Under the AI button, right above open source (vividn, 2026-11-10, da-a6xba). -->
	<p>Local first. You own your data. Seamless sync between your devices. No one else sees or can access your data.</p>
	<p>Datum is open source! (AGPL-3.0): <a href={REPO} target="_blank" rel="noopener">github.com/vividn/datum</a></p>
</div>

<style>
	/* Read, not scanned: a measure a sentence fits, wrapped like the Help place's. */
	.welcome {
		max-inline-size: 34rem;
		color: var(--ink);
		line-height: 1.45;
	}

	p {
		margin: 0 0 0.5rem;
	}

	/* The one thing to do next, so it is drawn in the accent before it is
	   pointed at: a phone has no hover to light it (vividn, 10-14: «very
	   invisible right now»). */
	button.command {
		border-color: var(--accent);
		color: var(--ink-bright);
	}

	/* Filled, where its siblings are outlined: what to do first. */
	button.track {
		background: var(--accent);
		color: var(--page);
	}
	button.track:hover:not(:disabled),
	button.track:focus-visible {
		background: var(--accent-bright);
		color: var(--page);
	}

	.name {
		margin: 0 0 0.3rem;
		color: var(--ink-bright);
		font-size: 1.6rem;
		letter-spacing: 0.02em;
	}

	a {
		color: var(--accent-bright);
	}

	/* Selectable as one, wrapped to the sheet. */
	.prompt {
		margin: 0 0 0.5rem;
		padding: 0.5rem;
		border: 1px solid var(--rule);
		border-radius: 4px;
		white-space: pre-wrap;
		user-select: all;
	}

	.quiet {
		color: var(--quiet);
	}
</style>
