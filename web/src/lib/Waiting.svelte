<!--
	What the page is waiting for, when it has waited a while (da-bhudz).

	vividn, 2026-10-07: «There should at least be some sort of loading spinner
	or explanation of why the screen is staying blank.» The shell is drawn
	at once from the last visit, so the screen is not blank; this is the
	explanation. Shown once the CLI has gone a second without its first
	answer, or a second into a schema upgrade, with the seconds counting:
	an upgrade says so, because it is the one wait that is not the page's
	fault and will not happen again.

	vividn, 2026-10-07 (da-89kom): a phone hung on «opening database…» after
	a reload, «completely unable to proceed». The bar is not ready until the
	CLI has answered, so the line stays up until it is, not merely until a
	first answer. And the one wait with no deadline is another tab of the
	site holding the files — a long process there, or, suspected and not
	reproduced, a tab the phone froze in the background mid-burst — which
	the line names, at any time, with
	the way out: that tab closed, this one opens, and nothing is lost.
-->
<script lang="ts">
	import { onDestroy } from 'svelte';
	import { browser } from '$app/environment';
	import { blocked, first, upgrading } from './waiting.js';
	import { ready } from './layout/engine.js';

	/** When the wait began, on `performance.now()`: another tab's hold, the upgrade, or the navigation. */
	$: since = $blocked ?? $upgrading ?? ($first && $ready ? null : 0);

	let now = browser ? performance.now() : 0;
	// Read again when the wait's start moves: the tick's `now` is up to
	// 250 ms old, which put a wait dated a second back under a second and
	// blinked the line out as its words changed (kimi-k3 on #493).
	$: if (browser && since !== null) now = performance.now();
	let timer: ReturnType<typeof setInterval> | undefined;
	$: if (browser && since !== null) timer ??= setInterval(() => (now = performance.now()), 250);
	$: if (since === null && timer) {
		clearInterval(timer);
		timer = undefined;
	}
	onDestroy(() => clearInterval(timer));

	$: seconds = since === null ? 0 : Math.floor((now - since) / 1000);
</script>

{#if since !== null && seconds >= 1}
	<p class="waiting" role="status">
		{$blocked !== null
			? `Another datum.day tab or window is using your data. This one ${$ready ? 'waits until' : 'opens when'} it is done, or closed.`
			: $upgrading !== null
				? 'Upgrading your data to this version (one time)…'
				: 'Opening your data…'}
		<span class="seconds">{seconds} s</span>
	</p>
{/if}

<style>
	.waiting {
		position: absolute;
		top: 2.6rem;
		left: 50%;
		transform: translateX(-50%);
		z-index: 20;
		margin: 0;
		padding: 0.3rem 0.8rem;
		max-width: calc(100% - 2rem);
		font-size: 0.8rem;
		color: var(--ink);
		background: var(--tint);
		border: 1px solid var(--rule);
		border-radius: 999px;
	}
	.seconds {
		color: var(--muted);
		font-variant-numeric: tabular-nums;
	}
</style>
