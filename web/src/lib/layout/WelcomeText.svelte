<!--
	The welcome's words (da-u7vz8), apart from its frame, so the Help place
	can show them as its front page while nothing is explained (vividn,
	2026-10-14: «this welcome screen could just be the front page of the
	help widget in general … after all the preamble, maybe having links to
	follow for more information»). The links open `datum help <topic>` in
	the Help place, the same path the Help button takes, so what they say is
	the CLI's own help and cannot drift from it — but sync's, which opens the
	page's own guide to the Sync widget (da-oyvsz), the CLI's help folded at
	its end.
-->
<script lang="ts">
	import { act } from '$lib/act.js';
	import { openField } from '$lib/fieldWidget.js';
	import { explainCommand, explainGuide, GUIDES } from '$lib/help.js';
	// Each shape's name and line are `VERBS`'s, shared with the Track form.
	import { VERBS } from '$lib/track.js';

	/*
	 * One working day, 0 to 120, shared by the three timelines so they line up (vividn, 2026-10-14: «a bit more
	 * tongue in cheek about business»). Coffee at arrival, at two short breaks (the first right before the first
	 * meeting) and straight after lunch; the screen off for lunch and for those breaks; and the day's activity all
	 * email and meetings, with a sliver of deep work. Colours are chosen here, not `defaultColor`: hashed, deepwork came out navy on the dark ground.
	 */
	const COFFEE = { field: 'coffee', color: '#c69c6d', at: [3, 16, 60, 89] };
	const SCREEN = { field: 'screentime', color: '#8fa3bf', on: [[0, 14], [18, 30], [58, 29], [91, 29]] as [number, number][] };
	const STATES: Record<string, string> = { email: '#4f8fe8', meeting: '#d48788', deepwork: '#7bd389' };
	const ACTIVITY = {
		field: 'activity',
		color: '#c8c8c8',
		spans: [
			[0, 18, 'email'], [18, 22, 'meeting'], [40, 8, 'email'],
			[58, 24, 'meeting'], [82, 14, 'email'], [96, 4, 'deepwork'], [100, 20, 'meeting']
		] as [number, number, string][]
	};

	/** «Learn more»: a command or topic of `datum help`, or a guide of the page's own, and what to call it here. Each is a real page. */
	const MORE: [string, string][] = [
		['logging', 'Logging an entry'],
		['time', 'Times, dates and durations'],
		['setup', 'Fields and their shapes'],
		['alias', 'Aliases'],
		['sync', 'Syncing your devices'],
		['db', 'Backups and databases'],
		['stats', 'Stats']
	];
</script>

<div class="welcome">
		<h1 class="name">datum</h1>
		<p class="motto">Record your life, cultivate awareness, and confront the truth.</p>
		<p>Local first. You own your data. Seamless sync between your devices. No one else sees or can access your data.</p>
		<p>
			Track what matters to you. Each <b>field</b> is one thing you track and pay attention to (sleep, mood,
			activity). For each field you log <b>entries</b>, timestamped logs documenting the field in your life. Entries can
			change a field's <b>state</b>, which persists until the next state change.
		</p>
		<h3>Field shapes</h3>
		<p>Fields can take different shapes:</p>
		<ul class="shapes">
			{#each VERBS as { verb, glyph, what, line } (verb)}
				<li>
					<span class="head"><span class="glyph" aria-hidden="true">{glyph}</span> <b>{what}</b>: {line}</span>
					{#if verb === 'add'}
						<fieldset class="card">
							<legend>person:vr520</legend>
							<pre>id: vr520
name: vremya
city: cantown
comment: obsessed with time</pre>
						</fieldset>
					{:else}
						{@const { field, color } = verb === 'occur' ? COFFEE : verb === 'start' ? SCREEN : ACTIVITY}
						<div class="timeline">
							<span class="field" style:color>{field}</span>
							{#if verb === 'switch'}
								<ul class="legend">
									{#each Object.entries(STATES) as [state, color] (state)}
										<li><span class="swatch" style:background={color}></span>{state}</li>
									{/each}
								</ul>
							{/if}
							<svg viewBox="0 0 120 12" preserveAspectRatio="none" aria-hidden="true">
								<line x1="0" y1="6" x2="120" y2="6" />
								{#if verb === 'occur'}
									{#each COFFEE.at as x}<path class="dot" d="M{x} 6h0" stroke={COFFEE.color} />{/each}
								{:else if verb === 'start'}
									{#each SCREEN.on as [x, w]}<rect {x} y="2" width={w} height="8" fill={SCREEN.color} />{/each}
								{:else}
									{#each ACTIVITY.spans as [x, w, state]}<rect {x} y="2" width={w} height="8" fill={STATES[state]} />{/each}
								{/if}
							</svg>
						</div>
					{/if}
				</li>
			{/each}
		</ul>
		<h3>How to track</h3>
		<ol>
			<li>
				Choose one or two fields to track.
				<button type="button" class="command" use:act={'open'} on:click={() => openField()}>Setup</button>
				(or swipe to the next tab)
			</li>
			<li>Log them as they happen: tap, or type in the input bar.</li>
			<li>Pay attention. Make noticing and recording a habit before you add more fields.</li>
			<li>Find your patterns: visualize and analyze all that beautiful data.</li>
		</ol>
		<h3>The input bar</h3>
		<p>It rests at the bottom of the screen, always there. Everything datum does, you can type into it:</p>
		<ul>
			<li>log data: <code>coffee</code>, or an alias of your own</li>
			<li>manage your databases: sync, backups, <code>db</code></li>
			<li>get help: <code>help</code>, or <code>help</code> and a topic</li>
		</ul>
		<p>
			Every tap shows the command it ran, so the bar teaches you as you go. Swipe up on it for the history and each
			command's full output.
		</p>
		<h3>Tabs and widgets</h3>
		<p>Tabs run across the top; each is a page of widgets. In layout mode you can rearrange a page or build your own.</p>
		<ul>
			<li>Move a tab: drag it. On a phone, hold it first.</li>
			<li>Open a widget, or a tab you closed: the + at the top.</li>
			<li>Change a page: hold its tab, or right-click it, for layout mode.</li>
			<li>In layout mode a pane's name splits, changes or closes it.</li>
			<li>Leave by the tab's name: Accept, Save layout (every device), or Discard changes.</li>
		</ul>
		<h3>Learn more</h3>
		<ul class="more">
			{#each MORE as [name, what] (name)}
				<li><button type="button" class="link" on:click={() => (GUIDES[name] ? explainGuide(name) : explainCommand(name))}>{what}</button></li>
			{/each}
		</ul>
</div>

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

	/* The one thing to do next, so it is drawn in the accent before it is
	   pointed at: a phone has no hover to light it (vividn, 10-14: «very
	   invisible right now»). */
	button.command {
		border-color: var(--accent);
		color: var(--ink-bright);
	}

	.name {
		margin: 0 0 0.3rem;
		color: var(--ink-bright);
		font-size: 1.6rem;
		letter-spacing: 0.02em;
	}

	.motto {
		color: var(--ink-bright);
	}

	.shapes {
		list-style: none;
		padding: 0;
		display: grid;
		gap: 1rem;
	}

	.shapes .glyph {
		color: var(--accent-bright);
	}

	/*
	 * The timeline takes the whole width, the field's name over it in its data's colour, and a switch's states
	 * beside the name. The day stretches to the width; its dots and its rule are drawn in screen pixels so they don't.
	 */
	.timeline {
		display: flex;
		flex-wrap: wrap;
		align-items: center;
		gap: 0.2rem 0.8rem;
		margin-block-start: 0.2rem;
	}

	.timeline .field {
		font-size: 0.8rem;
	}

	.timeline svg {
		display: block;
		inline-size: 100%;
		block-size: 1.4rem;
	}

	.timeline line {
		stroke: var(--rule);
		vector-effect: non-scaling-stroke;
	}

	.timeline .dot {
		stroke-width: 0.6rem;
		stroke-linecap: round;
		vector-effect: non-scaling-stroke;
	}

	.legend {
		display: flex;
		flex-wrap: wrap;
		gap: 0.2rem 0.8rem;
		margin: 0;
		padding: 0;
		list-style: none;
		color: var(--muted);
		font-size: 0.8rem;
	}

	.swatch {
		display: inline-block;
		inline-size: 0.6rem;
		block-size: 0.6rem;
		margin-inline-end: 0.3rem;
	}

	/* The card's full id stands on its frame, as a fieldset's legend does. */
	.card {
		margin: 0.2rem 0 0 0.6rem;
		padding: 0 0.5rem 0.3rem;
		inline-size: fit-content;
		min-inline-size: 0;
		border: 1px solid var(--rule);
	}

	.card legend {
		padding: 0 0.3rem;
		color: var(--muted);
		font-size: 0.7rem;
	}

	.card pre {
		margin: 0;
		color: var(--ink);
		font-size: 0.8rem;
	}

	/* «Learn more»: links into the Help place, one per line on a phone, a row where there is room. */
	.more {
		list-style: none;
		padding: 0;
		display: flex;
		flex-wrap: wrap;
		gap: 0.3rem 1rem;
	}

	.link {
		padding: 0;
		border: none;
		background: none;
		color: var(--accent-bright);
		font: inherit;
		text-decoration: underline;
		text-underline-offset: 0.15em;
		cursor: pointer;
	}

	code {
		color: var(--ink-bright);
	}
</style>
