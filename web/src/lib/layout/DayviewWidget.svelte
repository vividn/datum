<!--
	The dayview widget: the day strip, or — pivoted (da-a6guc) — datum's own
	`dayview --orientation vertical`, a week of days as columns with time
	running down them, asked for at the pane's size through a CliPane. vividn:
	"most calendars are already vertically oriented", and it may read better
	on a phone than the strip does. The strip's own gestures (zoom, the `+`
	handle, the frozen today) are the horizontal strip's; the pivot is the
	CLI's picture as it draws it, and still in modify mode (da-1tmd1).
-->
<script lang="ts">
	import type { ComponentProps } from 'svelte';
	import { DAYVIEW_VERTICAL } from '#lib/calls.ts';
	import CliPane from '#lib/CliPane.svelte';
	import DayStrip from '#lib/DayStrip.svelte';

	type Props = ComponentProps<typeof DayStrip> & { orientation?: string };

	let { epoch = 0, orientation = 'horizontal', ...rest }: Props = $props();
</script>

{#if orientation === 'vertical'}
	<div class="pivot" class:still={rest.modifying} inert={rest.modifying}><CliPane spec={DAYVIEW_VERTICAL} {epoch} /></div>
{:else}
	<DayStrip {...rest} {epoch} />
{/if}

<style>
	.pivot {
		display: contents;
	}
	.still > :global(*) {
		opacity: 0.45;
	}
</style>
