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

	A game's tab is four panes (da-uzyih.4908, vividn, 2026-10-29): its
	title and what its shape of data is (`part: 'about'`), short enough for
	its third of the tab («it doesn't need to describe the game like this.
	Instead it should be about what occur data is»; his whole words are
	docs/basics.md), and its way on; the nowview; the game; and its fields
	with Undo (`part: 'fields'`, LessonFields.svelte), the two sharing the
	round (`roundOf`).
-->
<script lang="ts">
	import { onDestroy } from 'svelte';
	import { act } from '#lib/act.ts';
	import Pane from '#lib/Pane.svelte';
	import { VERBS } from '#lib/track.ts';
	import { lessonNamed } from '#lib/tutorial.ts';
	import { LESSONS, type Lesson } from './defaults.js';
	import { open, resetTutorial } from './shell.js';
	import LessonFields from './LessonFields.svelte';
	import OccurLesson from './OccurLesson.svelte';
	import SwitchLesson from './SwitchLesson.svelte';
	import OnOffLesson from './OnOffLesson.svelte';

	let { settings = {} }: { settings?: Record<string, unknown> } = $props();

	const lesson = $derived(lessonNamed(settings.lesson) ?? LESSONS[0]);
	const intro = $derived(lesson === LESSONS[0]);
	const next = $derived(LESSONS[LESSONS.indexOf(lesson) + 1]);

	/** A game's pane: its title and what its shape of data is, or its fields; neither, the game itself. */
	const part = $derived(lesson.fields.length ? settings.part : undefined);
	/** A lesson as vividn names it: the word its title leads with («on/off»), or the whole of a title with none. */
	const label = (l: Lesson): string => l.title.split(':')[0];
	/** The game's field kind, named as the setup widget names it, by the word its title leads with. */
	const shape = $derived(VERBS.find((v) => v.what === label(lesson)));

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

<Pane title={lesson.title}>
	<div class="tutorial" class:fill={lesson.fields.length && part !== 'about'}>
		{#if part === 'about'}
			<!-- Short enough for its third of the tab; vividn's whole words on each shape are docs/basics.md. -->
			<h1 class="title">{lesson.title}</h1>
			<p>
				<span class="glyph" aria-hidden="true">{shape?.glyph}</span> <b>{shape?.what}</b>:
				{#if lesson.name === 'moment'}
					something that happens or is measured at a single point in time, a dot on the timeline. Each one logged is an
					<b>entry</b>, a timestamped log of the field.
				{:else if lesson.name === 'on-off'}
					something that happens for a while and then stops, a period on the timeline. Its <b>state</b> is true or false, and
					lasts until an entry changes it.
				{:else}
					something that passes through phases or modes, periods that change color on the timeline. Each <b>state</b> is
					one you name, and lasts until the next.
				{/if}
			</p>
		{:else if part === 'fields'}
			<LessonFields {lesson} />
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
		{:else}
			<!-- The game and its fields alone: the title and what the shape is stand over the nowview (da-uzyih.4908). -->
			{#if lesson.name === 'moment'}
				<OccurLesson fields={lesson.fields} />
			{:else if lesson.name === 'switch'}
				<SwitchLesson fields={lesson.fields} />
			{:else if lesson.name === 'on-off'}
				<OnOffLesson fields={lesson.fields} />
			{:else}
				<h1>{lesson.title}</h1>
				<p>This lesson is coming soon.</p>
			{/if}
		{/if}
		{#if next && !(lesson.fields.length && part !== 'about')}
			<button type="button" class="command next" use:act={'open'} onclick={() => void open(next.name)}
				>Next: {label(next)}</button
			>
		{/if}
		{#if intro}
			<button type="button" class="command reset" use:act={'run'} disabled={resetting} onclick={reset}>Reset the tutorial</button>
			{#if failed}<p class="error" role="alert">The tutorial was not reset: {failed}</p>{/if}
		{/if}
	</div>
</Pane>

<style>
	/* Clear of the frame's name over the pane's top edge: the game is the first thing under it. */
	.tutorial {
		max-inline-size: 34rem;
		padding-block-start: 0.5rem;
		line-height: 1.45;
	}

	/* A game and its fields take their pane's height and no more, so each is wholly in sight (da-uzyih.4908): its
	   parts are sized from it (`cqb`). */
	.fill {
		box-sizing: border-box;
		block-size: 100%;
		container-type: size;
	}

	h1 {
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

	/* A game's title, small over its description (vividn, 2026-10-29: «make the title text a lot smaller»). */
	h1.title {
		font-size: 0.95rem;
	}

	ul {
		padding-inline-start: 1.4rem;
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
