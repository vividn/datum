<!--
	One tab's content inside its frame, mounted whether or not it is the tab
	in front (da-rjmg: "preload and keep fresh the display of all tabs in a
	widget even when not displayed so that swiping looks nicer and all the
	data is ready to go").

	The tab in front is `display: contents`, so the frame's content box lays
	its widget out exactly as it did when it was the only one mounted. A
	tab behind it keeps a box of the same size — laid over the content,
	`visibility: hidden` rather than `display: none`, since a drawn widget
	measures its box to render — and is `inert`, so nothing in it takes
	focus, a key or a pointer. It hears the epoch and the minute tick as the
	tab in front does, so a swipe shows a picture already drawn.

	The frame's context is passed down through a gate: what a widget sends
	up to its frame — its own title controls (da-ndjo's `.own` box) and its
	alarm — reaches the frame from the tab in front only. Every tab is
	mounted, but only one is ACTIVE, and the portal keys on that.
-->
<script lang="ts">
	import { getContext, setContext } from 'svelte';
	import { derived, writable, type Writable } from 'svelte/store';

	export let active: boolean;

	interface Frame {
		alarm: Writable<boolean>;
		titleHost: Writable<HTMLElement | null>;
		hosted: Writable<boolean>;
	}

	const frame = getContext<Frame>('layout-frame');
	const front = writable(active);
	const alarm = writable(false);
	const hosted = writable(false);
	$: front.set(active);
	$: if (active) frame.alarm.set($alarm);
	$: if (active) frame.hosted.set($hosted);
	setContext('layout-frame', {
		alarm,
		hosted,
		titleHost: derived([frame.titleHost, front], ([host, on]) => (on ? host : null))
	});
</script>

<div class="tab-body" class:behind={!active} inert={!active}><slot /></div>

<style>
	.tab-body {
		display: contents;
	}
	.behind {
		position: absolute;
		inset: 0;
		display: flex;
		flex-direction: column;
		visibility: hidden;
	}
</style>
