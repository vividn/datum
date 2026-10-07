<!--
	What the page is waiting for, when it has waited a while (da-bhudz).

	vividn, 2026-10-07: «There should at least be some sort of loading spinner
	or explanation of why the screen is staying blank.» The shell is drawn
	at once from the last visit, so the screen is not blank; this is the
	explanation. Shown once the CLI has gone a second without its first
	answer, or a second into a schema upgrade, with the seconds counting:
	an upgrade says so, because it is the one wait that is not the page's
	fault and will not happen again.
-->
<script lang="ts">
	import { onDestroy } from 'svelte';
	import { browser } from '$app/environment';
	import { first, upgrading } from './waiting.js';

	/** When the wait began, on `performance.now()`: the navigation, or the upgrade. */
	$: since = $upgrading ?? ($first ? null : 0);

	let now = browser ? performance.now() : 0;
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
		{$upgrading !== null ? 'Upgrading your data to this version (one time)…' : 'Opening your data…'}
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
