<!--
	The app shell.

	The root tab bar above, the fixed bottom bar below, and between them a
	viewport-height main that never scrolls itself (da-0a5o). Panes inside
	the page scroll individually (see $lib/Pane.svelte), so the bottom bar
	— the prompt, the brand, the receipt — stays put whatever the page
	does, on a phone too: `100dvh`, so the browser's own chrome sliding
	in and out does not push it off.
-->
<script lang="ts">
	import { onMount } from 'svelte';
	import '../app.css';
	import { installConsole } from '$lib/console.js';
	import PromptBar from '$lib/PromptBar.svelte';
	import RootBar from '$lib/layout/RootBar.svelte';
	import { boot as bootEngine } from '$lib/layout/engine.js';
	import { boot as bootShell } from '$lib/layout/shell.js';
	import { currentDb, DEFAULT_DB, INTERNAL_DB, refusedDb } from '$lib/database.js';
	import { page } from '$app/stores';

	// `?db=<name>` (da-vk1x). Off `$page.url` rather than `location`, so a
	// client-side navigation that changes the query re-renders the title and
	// the refusal with it; the seam reads the same query for itself.
	$: db = $page.url ? currentDb() : DEFAULT_DB;
	$: refused = $page.url ? refusedDb() : null;

	// `window.datum` — the command seam at the browser's own prompt. Mounted
	// here rather than at module scope because there is no window during
	// prerendering. See $lib/console.ts. Then the two boots: the engine's
	// (the database, the bar's fields) and the shell's (the root tabs),
	// which every route needs — a `/<name>` opens a tab into a booted shell.
	onMount(() => {
		installConsole(window as unknown as Record<string, unknown>);
		void bootEngine();
		void bootShell();
		reloadOnceForIsolation();
	});

	/**
	 * The service worker adds the isolation headers to everything it serves —
	 * but the first document of a visit was fetched before any worker
	 * controlled the page, so on a host that sends no headers of its own that
	 * document is not isolated. Reload once when the worker takes control,
	 * and only then; from the second load on, isolation is simply there.
	 *
	 * Guarded three ways, because a reload is the one thing this page must
	 * never do twice or at the wrong moment: a session flag so it happens at
	 * most once per tab; only while the prompt is still empty, so nothing
	 * typed is lost; and never when already isolated, which is every load on
	 * a deployment whose host sends the headers itself.
	 */
	function reloadOnceForIsolation() {
		if (crossOriginIsolated || !('serviceWorker' in navigator)) return;
		const KEY = 'datum-reloaded-for-isolation';
		const reload = () => {
			if (sessionStorage.getItem(KEY) === '1') return;
			const prompt = document.querySelector<HTMLInputElement>('input');
			if (prompt?.value) return;
			sessionStorage.setItem(KEY, '1');
			location.reload();
		};
		if (navigator.serviceWorker.controller) reload();
		else navigator.serviceWorker.addEventListener('controllerchange', reload, { once: true });
	}
</script>

<svelte:head>
	<title>{db === DEFAULT_DB ? 'datum' : `datum · ${db}`}</title>
</svelte:head>

<div class="shell">
	<RootBar />

	<main>
		{#if refused !== null}
			<!--
				The one database the page may not be pointed at is the app's own
				(da-vk1x): `_internal` holds the saved layouts, and a frame drawn
				on it would be reading its own furniture as data. A path is
				refused for a plainer reason — there is no filesystem out there
				for it to be a path into. Either way the app underneath is the
				default database, so the prompt still works while this is read.
			-->
			<p class="notice">
				{refused === INTERNAL_DB
					? `“${INTERNAL_DB}” is datum.day's own database — the saved layouts live there, not your entries.`
					: `“${refused}” is not a database name.`}
				showing <code>{DEFAULT_DB}</code> instead. <a href="/">home</a>
			</p>
		{:else}
			<slot />
		{/if}
	</main>

	<PromptBar />
</div>

<style>
	.shell {
		display: flex;
		flex-direction: column;
		/* Dynamic viewport units so mobile browser chrome sliding in and out
		   doesn't push the bottom bar off-screen. */
		height: 100dvh;
	}

	.notice {
		margin: 1rem 0;
		color: var(--quiet);
		font-style: italic;
	}

	.notice a {
		color: var(--accent);
	}

	main {
		flex: 1;
		min-height: 0;
		/* The frame fills this box exactly and scrolls its own panes. */
		overflow: auto;
		padding: 0.75rem;
	}
</style>
