<!--
	The app shell.

	Fixed chrome — beta banner, header, nav — wrapped around a viewport-height
	main that never scrolls itself. Panes inside a page scroll individually
	(see $lib/Pane.svelte), so the REPL input at the bottom of the frame stays
	put while output accumulates above it.
-->
<script lang="ts">
	import '../app.css';
	import BetaBanner from '$lib/BetaBanner.svelte';
	import { page } from '$app/stores';

	const links = [
		{ href: '/', label: 'day' },
		{ href: '/now', label: 'now' }
	];
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
</style>
