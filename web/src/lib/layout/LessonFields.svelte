<!--
	A game's fields, in a pane of their own under the game (da-uzyih.4908,
	vividn, 2026-10-29: «Let's have the game and the fields widgets be
	separate widgets here»): the lesson's fields alone, as many rows as the
	pane holds, and a tap logs as any field's does, into the round its game
	scores (`roundOf`). A tap that fails is said here (da-uzyih.52).
-->
<script lang="ts">
	import CollectWidget from '#lib/CollectWidget.svelte';
	import type { Lesson } from './defaults.js';
	import { fields as buttons, ready, standings } from './engine.js';
	import { roundOf } from './round.js';

	let { lesson }: { lesson: Lesson } = $props();

	const round = $derived(roundOf(lesson.name));
	const { failed, phase } = $derived(round);
	const shown = $derived($buttons.filter((f) => lesson.fields.includes(f.field)));
</script>

<div class="lesson">
	<div class="inputs">
		<!-- At rest the switch game's cue points at its field. -->
		<div class="controls" class:cued={lesson.name === 'switch' && $phase === 'neutral'}>
			<CollectWidget fields={shown} standings={$standings} showIgnored tapLogs disabled={!$ready} onrun={(argv) => round.tap(argv)} onfill={() => {}} />
		</div>
	</div>
	{#if $failed}<p class="failed">{$failed}</p>{/if}
</div>

<style>
	/* The widget's box, the pane's height (vividn, 2026-10-27: «the fields widget should always be visible on these
	   tutorial pages»): it lays out as many rows as that fits, and a field's states stand around the tap. */
	.inputs {
		display: flex;
	}

	.controls {
		display: flex;
		flex: 1;
		block-size: 100cqb;
	}

	.cued :global(button[data-field]) {
		outline: 2px solid var(--accent);
		outline-offset: 2px;
	}

	.failed {
		color: var(--error);
	}
</style>
