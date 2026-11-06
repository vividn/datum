<!--
	The game widget (da-2chdp, vividn, 2026-11-05: «game (should be called game
	widget)»): a lesson's game in its box, Start, the round, its score and Reset
	(GameBox.svelte), taking its pane's height and no more, so it is wholly in
	sight (da-uzyih.4908): its parts are sized from it (`cqb`). Its title and
	what its shape of data is, and its fields, are the `tutorial` kind's
	(TutorialWidget.svelte), which draws this for a lesson with no part, as a
	tab saved before this kind did.
-->
<script lang="ts">
	import Pane from '#lib/Pane.svelte';
	import { lessonNamed } from '#lib/tutorial.ts';
	import { LESSONS } from './defaults.js';
	import OccurLesson from './OccurLesson.svelte';
	import SwitchLesson from './SwitchLesson.svelte';
	import OnOffLesson from './OnOffLesson.svelte';

	let { settings = {} }: { settings?: Record<string, unknown> } = $props();

	const lesson = $derived(lessonNamed(settings.lesson) ?? LESSONS[1]);
</script>

<Pane title={lesson.title}>
	<div class="game-widget">
		{#if lesson.name === 'moment'}
			<OccurLesson fields={lesson.fields} />
		{:else if lesson.name === 'change'}
			<SwitchLesson fields={lesson.fields} />
		{:else if lesson.name === 'on-off'}
			<OnOffLesson fields={lesson.fields} />
		{:else}
			<p>{lesson.title} has no game.</p>
		{/if}
	</div>
</Pane>

<style>
	/* Clear of the frame's name over the pane's top edge: the game is the first thing under it. */
	.game-widget {
		box-sizing: border-box;
		block-size: 100%;
		max-inline-size: 34rem;
		padding-block-start: 0.5rem;
		line-height: 1.45;
		container-type: size;
	}
</style>
