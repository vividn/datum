<script lang="ts">
	import '../app.css';
</script>

<nav>
	<a href="/">datum</a>
	<a href="/now">now</a>
</nav>

<main>
	<slot />
</main>

<style>
	nav {
		background: #0d0d1a;
		padding: 0.5rem 1rem;
		display: flex;
		gap: 1.5rem;
		align-items: center;
		border-bottom: 1px solid #2a2a4a;
	}

	nav a {
		color: #7c7cff;
		text-decoration: none;
		font-family: monospace;
		font-size: 0.9rem;
	}

	nav a:hover {
		color: #adadff;
	}

	main {
		padding: 1rem;
		background: #0a0a1a;
		min-height: calc(100vh - 2.5rem);
	}
</style>
