<!--
	The tutorial's own widget (da-6cvrd), one per lesson tab: the lesson's
	game in its box and its fields (da-uzyih.4864). What a game
	logs stays: its box's Reset ends the game's states and deletes
	nothing.

	The Introduction says what the tutorial is and, plainly, how to
	get back (vividn, 2026-10-19: «make sure to indicate on the first page how
	to switch back to the "datum" database»): the switcher is the database's
	name in the bottom bar. Its first paragraph says so, and its «where?»
	sends an arrow down to it and POINTS AT it for a moment — a ring
	(`data-point-at` on the root, Brand.svelte's style) — and nothing rings
	it until asked (da-uzyih.4869, vividn, 2026-10-27: «It doesn't need to be
	constantly flashing like right now»). Its end goes on, to the next tab
	(da-uzyih.4871, vividn, 2026-10-27: «a button to continue to the next tab
	rather than go back to datum»), as every lesson's does but the last's
	(da-uzyih.4892, vividn, 2026-10-28: «advancement buttons at the bottom of
	every page on the tutorial like there is on the introduction»). Under
	that, the Introduction's last button resets the tutorial
	(`resetTutorial`, shell.ts; da-uzyih.4893, vividn, 2026-10-28: «add a
	button at the bottom of the introduction page "reset the tutorial"»).

	A lesson with no fields is a page of text, with nothing to start over.

	A game's tab is five panes, taller than the screen (da-zb1dr): its
	title and what its shape of data is (`part: 'about'`), its pane its
	text's height (da-83y6w), short enough to leave the game and collect in
	sight («it doesn't need to describe the game like this. Instead it should
	be about what occur data is»; his
	whole words are docs/basics.md); the nowview; the game
	(GameWidget.svelte, da-2chdp), which this kind still draws for a lesson
	with fields and no part, as a tab saved before that kind did; and the
	collect widget filtered to its fields (da-2s9im), which this kind still
	draws for `part: 'fields'`, as a tab saved before did; and under them what
	just happened, at more length, and the way on (`part: 'discuss'`), its
	text's height too. The game
	scores what is logged, whatever logged it (`roundOf`).
-->
<script lang="ts">
	import { onDestroy } from 'svelte';
	import { act } from '#lib/act.ts';
	import Pane from '#lib/Pane.svelte';
	import { VERBS } from '#lib/track.ts';
	import { lessonNamed } from '#lib/tutorial.ts';
	import { LESSONS } from './defaults.js';
	import { open, resetTutorial } from './shell.js';
	import GameWidget from './GameWidget.svelte';
	import CollectPane from './CollectPane.svelte';

	let { settings = {} }: { settings?: Record<string, unknown> } = $props();

	const lesson = $derived(lessonNamed(settings.lesson) ?? LESSONS[0]);
	const intro = $derived(lesson === LESSONS[0]);
	const next = $derived(LESSONS[LESSONS.indexOf(lesson) + 1]);

	/** A game's pane: its title and what its shape of data is, or its fields; neither, the game itself. */
	const part = $derived(lesson.fields.length ? settings.part : undefined);
	/** The game's field kind, named as the setup widget names it. */
	const shape = $derived(VERBS.find((v) => v.what === lesson.shape));

	let resetting = $state(false);
	/** Why the last reset failed, said under its button: a refusal before any command runs shows nowhere else (da-uzyih.4913). */
	let failed = $state('');
	const reset = (): void => {
		resetting = true;
		failed = '';
		resetTutorial()
			.catch((e) => (failed = e instanceof Error ? e.message : String(e)))
			.finally(() => (resetting = false));
	};

	let pointing: ReturnType<typeof setTimeout> | undefined;
	/** The arrow in flight, on `document.body`, outside the component: gone with it, or with the next «where?». */
	let falling: HTMLElement | undefined;
	const unpoint = (): void => {
		clearTimeout(pointing);
		falling?.remove();
		delete document.documentElement.dataset.pointAt;
	};
	onDestroy(unpoint);

	/** «where?»: ring the switcher for a moment, an arrow falling to it first where motion is welcome. */
	function where(event: MouseEvent): void {
		unpoint();
		document.documentElement.dataset.pointAt = 'switcher';
		pointing = setTimeout(unpoint, 2400);
		const brand = document.querySelector('.brand-menu .brand');
		if (!brand || matchMedia('(prefers-reduced-motion: reduce)').matches) return;
		const from = (event.currentTarget as Element).getBoundingClientRect();
		const to = brand.getBoundingClientRect();
		const arrow = (falling = document.body.appendChild(document.createElement('span')));
		arrow.className = 'point-arrow';
		arrow.textContent = '↓';
		arrow.setAttribute('aria-hidden', 'true');
		const x = to.left + to.width / 2;
		arrow
			.animate(
				[
					{ transform: `translate(${x}px, ${from.bottom}px) translate(-50%, 0)`, opacity: 0 },
					{ opacity: 1, offset: 0.15 },
					{ transform: `translate(${x}px, ${to.top}px) translate(-50%, -100%)`, opacity: 1, offset: 0.8 },
					{ transform: `translate(${x}px, ${to.top}px) translate(-50%, -100%)`, opacity: 0 }
				],
				{ duration: 1100, easing: 'ease-in' }
			)
			.finished.finally(() => arrow.remove());
	}
</script>

{#if lesson.fields.length && !part}
	<GameWidget {settings} />
{:else}
<Pane title={lesson.title}>
	<div class="tutorial" class:fill={part === 'fields'}>
		{#if part === 'about'}
			<!-- A line, so the game and collect keep the screen under it; the terms are the discussion's, vividn's whole words on each shape docs/basics.md. -->
			<h1 class="title">{lesson.title}</h1>
			<p>
				<span class="glyph" aria-hidden="true">{shape?.glyph}</span> <b>{shape?.what}</b>:
				{#if lesson.name === 'moment'}
					a dot on the timeline.
				{:else if lesson.name === 'on-off'}
					a period of time.
				{:else}
					a color per state.
				{/if}
			</p>
		{:else if part === 'fields'}
			<CollectPane only={lesson.fields} showIgnored />
		{:else if part === 'discuss'}
			<!-- Under the game and collect, scrolled to once a round is played: what it logged, and why. -->
			<h2>What just happened</h2>
			{#if lesson.name === 'moment'}
				<p>
					Each tap logged an <b>entry</b> of its field at that instant, a dot on the timeline. A moment has no state and no
					duration: it says that something happened, and when.
				</p>
				<p>Later, datum counts them: how often, at what hours, how many a day. Ex. medication, weight, coffee.</p>
			{:else if lesson.name === 'on-off'}
				<p>
					Each tap turned its field on or off. While on, its <b>state</b> is true and its period grows on the timeline, so
					the period's length is how long it lasted. You log only the changes, and several fields can be on at once.
				</p>
				<p>Later, datum adds the periods up: how long, how often, at what hours. Ex. sleep, screen time, work.</p>
			{:else}
				<p>
					Each tap changed the field to a <b>state</b> you named, and its period changed color. It is in one state at a
					time, until the next entry: entering a room leaves the last, so you log only the changes.
				</p>
				<p>Later, datum says how long you spent in each state, and when. Ex. where you are, your project.</p>
			{/if}
		{:else if intro}
			<h1>The tutorial</h1>
			<p>
				Welcome to datum's tutorial. This is a practice database of its own. Nothing you log here reaches your own data, and it never syncs.
				Notice how the page changed colors? Different dbs have different background colors so you always know where you are.
			</p>
			<p>
				Move through the tutorial by swiping between its tabs, or tapping them above. Each tab teaches you how to use datum to collect
				and analyze data on your life.
			</p>
			<p>
				To leave the tutorial, use the menu at the bottom to go to another database
				(<button type="button" class="where" onclick={where}>where?</button>): tap its name, <b>tutorial</b>, and
				choose <b>datum</b>, your own. You can also make new databases as you like. Come back the same way; what you did here is kept.
			</p>
		{:else if !lesson.fields.length}
			<h1>{lesson.title}</h1>
			{#if lesson.name === 'input-bar'}
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
			{:else if lesson.name === 'tabs'}
				<p>Tabs run across the top of the page; each is a page of widgets. In layout mode you can rearrange a page or build your own.</p>
				<ul>
					<li>Move a tab: drag it. On a phone, hold it first.</li>
					<li>Open a widget, or a tab you closed: the + beside the tabs.</li>
					<li>Change a page: hold its tab, or right-click it, for layout mode.</li>
					<li>In layout mode a pane's name splits, changes or closes it.</li>
					<li>Leave by the tab's name: Accept, Save layout (every device), or Discard changes.</li>
				</ul>
			{/if}
		{/if}
		<!-- A game's way on is at the very bottom of its tab, under its discussion (da-zb1dr). -->
		{#if next && part !== 'fields' && part !== 'about'}
			<button type="button" class="command next" use:act={'open'} onclick={() => void open(next.name)}
				>Next: {next.title}</button
			>
		{/if}
		{#if intro}
			<button type="button" class="command reset" use:act={'run'} disabled={resetting} onclick={reset}>Reset the tutorial</button>
			{#if failed}<p class="error" role="alert">The tutorial was not reset: {failed}</p>{/if}
		{/if}
	</div>
</Pane>
{/if}

<style>
	/* Clear of the frame's name over the pane's top edge: the game is the first thing under it. */
	.tutorial {
		max-inline-size: 34rem;
		padding-block-start: 0.5rem;
		line-height: 1.45;
	}

	/* A game's fields take their pane's height and no more, so they are wholly in sight (da-uzyih.4908): its
	   parts are sized from it (`cqb`). */
	.fill {
		box-sizing: border-box;
		block-size: 100%;
		container-type: size;
	}

	h1,
	h2 {
		margin: 0 0 0.3rem;
		color: var(--ink-bright);
		font-size: 1.4rem;
	}

	b,
	code {
		color: var(--ink-bright);
	}

	.glyph {
		color: var(--accent-bright);
	}

	/* A game's title, small over its description (vividn, 2026-10-29: «make the title text a lot smaller»), and its discussion's. */
	h1.title,
	h2 {
		font-size: 0.95rem;
	}

	ul {
		padding-inline-start: 1.4rem;
	}

	/* The intro's line is its last: no margin under it to scroll to. */
	h1.title + p {
		margin-block-end: 0;
	}

	/* «where?» reads as a link in the sentence; it acts on this page, so it is a button. */
	button.where {
		padding: 0;
		border: none;
		background: none;
		color: var(--accent-bright);
		font: inherit;
		text-decoration: underline;
		cursor: pointer;
	}

	/* The arrow «where?» drops, on the body: no pane's box clips or offsets it. */
	:global(.point-arrow) {
		position: fixed;
		inset: 0 auto auto 0;
		z-index: 1000;
		color: var(--accent-bright);
		font-size: 1.6rem;
		line-height: 1;
		pointer-events: none;
	}

	button.command {
		border-color: var(--accent);
		color: var(--ink-bright);
	}

	button.next {
		margin-block: 0.8rem 0.5rem;
	}

	button.reset {
		display: block;
		margin-block: 1.6rem 0.5rem;
	}

	/* A line of words, so the bar's lighter red rather than `--error` (RootBar.svelte). */
	.error {
		color: #ff7b7b;
	}
</style>
