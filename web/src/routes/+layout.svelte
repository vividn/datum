<!--
	The app shell.

	Fixed chrome — beta banner, header, nav — wrapped around a viewport-height
	main that never scrolls itself. Panes inside a page scroll individually
	(see $lib/Pane.svelte), so the REPL input at the bottom of the frame stays
	put while output accumulates above it.
-->
<script lang="ts">
	import { onMount } from 'svelte';
	import '../app.css';
	import BetaBanner from '$lib/BetaBanner.svelte';
	import { installConsole } from '$lib/console.js';
	import { downloadBackup } from '$lib/backup.js';
	import { page } from '$app/stores';

	const links = [
		{ href: '/', label: 'day' },
		{ href: '/now', label: 'now' },
		{ href: '/stats', label: 'stats' }
	];

	let exporting = false;

	/**
	 * `datum db backup -`, handed over as a file: every entry, in the JSONL
	 * `datum db restore` reads back — the one copy of the data that is not
	 * in this browser, until there is sync.
	 */
	async function exportBackup() {
		exporting = true;
		try {
			await downloadBackup();
		} catch (e) {
			alert(`export failed: ${e instanceof Error ? e.message : String(e)}`);
		} finally {
			exporting = false;
		}
	}

	// `window.datum` — the command seam at the browser's own prompt. Mounted
	// here rather than at module scope because there is no window during
	// prerendering. See $lib/console.ts.
	onMount(() => {
		installConsole(window as unknown as Record<string, unknown>);
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

<div class="shell">
	<BetaBanner />

	<header>
		<a class="brand" href="/">datum<span class="tld">.day</span></a>
		<nav>
			{#each links as link (link.href)}
				<a href={link.href} aria-current={$page.url.pathname === link.href ? 'page' : undefined}>
					{link.label}
				</a>
			{/each}
		</nav>
		<button
			type="button"
			class="export"
			disabled={exporting}
			title="download every entry as `datum db backup` JSONL"
			on:click={exportBackup}>export</button
		>
		<!--
			Which build is actually in front of you. Deliberately dim and last:
			it is a developer's sanity check during early deployment, not
			something a reader of the app needs. `title` carries the same thing
			for a hover, so the visible form can stay short.
		-->
		<span class="build" title="datum.day {__DATUM_VERSION__} · commit {__DATUM_COMMIT__}"
			>{__DATUM_VERSION__} · {__DATUM_COMMIT__}</span
		>
	</header>

	<main>
		<slot />
	</main>
</div>

<style>
	.shell {
		display: flex;
		flex-direction: column;
		/* Dynamic viewport units so mobile browser chrome sliding in and out
		   doesn't push the REPL input off-screen. */
		height: 100dvh;
	}

	header {
		display: flex;
		align-items: baseline;
		gap: 1.5rem;
		flex-shrink: 0;
		padding: 0.4rem 0.75rem;
		background: #0d0d1a;
		border-bottom: 1px solid #2a2a4a;
	}

	.brand {
		color: #e0e0ff;
		text-decoration: none;
		font-size: 0.95rem;
	}

	.tld {
		color: #55558a;
	}

	nav {
		display: flex;
		gap: 1rem;
	}

	nav a {
		color: #7c7cff;
		text-decoration: none;
		font-size: 0.85rem;
	}

	nav a:hover {
		color: #adadff;
	}

	nav a[aria-current='page'] {
		color: #e0e0ff;
		border-bottom: 1px solid #7c7cff;
	}

	main {
		flex: 1;
		min-height: 0;
		/* The day frame fills this box exactly and scrolls its own panes; a
		   taller page (e.g. /now) scrolls here instead of spilling. */
		overflow: auto;
		padding: 0.75rem;
	}

	.export {
		background: none;
		border: none;
		padding: 0;
		font: inherit;
		font-size: 0.85rem;
		color: #7c7cff;
		cursor: pointer;
	}

	.export:hover:not(:disabled) {
		color: #adadff;
	}

	.export:disabled {
		opacity: 0.5;
		cursor: default;
	}

	/* Dim on purpose: present for whoever is looking for it, quiet for
	   everyone else. `ch` units so the two ids do not reflow the nav as the
	   hash changes between builds. */
	.build {
		margin-left: auto;
		/* The one item that gives way on a narrow screen, so the header
		   never pushes the page sideways. */
		min-width: 0;
		overflow: hidden;
		text-overflow: ellipsis;
		font-size: 0.7rem;
		font-variant-numeric: tabular-nums;
		color: color-mix(in srgb, currentColor 35%, transparent);
		white-space: nowrap;
	}
</style>
