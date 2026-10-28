<!--
	`datum.day/<name>` (da-0a5o): a saved layout, a built-in one, or a
	widget kind — opened as a root tab (added if absent, brought to the
	front) and then the page is `/` again, so the URL is the way IN and the
	root bar is where you are. A reload lands on the same tab: which tabs
	are open, and which is active, is this device's (#lib/layout/draft.ts).

	`/now` and `/stats` still work: they are the layouts of those names,
	and `/input` — a widget that went (da-22vg) — is the REPL, which is
	what it became. A name that is none of these is a quiet notice with a
	link home.

	`?from=<layout>` (da-xamd) seeds a name nothing answers to yet: the
	tab opens as a PREVIEW of that layout's tree, italic in the strip and
	written nowhere, and `Keep tab` in its menu is what makes it real. An
	existing name wins and `?from=` is simply ignored, so a link can never
	seed over a layout somebody has; a `from` that is nothing is the same
	notice as any other missing name, naming the one that is missing.

	An EMPTY `?from=` is no `from` at all — a link with the value trimmed
	off it names a layout and nothing else, so it opens `<name>` plainly
	rather than asking for a preview of the layout called "".

	`?db=<name>` (da-vk1x) rides alongside both and is untouched by either:
	it is the page's whole state, so `home` below carries it and a preview
	is seeded, kept and closed inside whatever database the reader came in
	with.

	One catch-all route, not a route per name, so the deployed site — where
	every path is served by adapter-static's fallback (vite.config.ts) —
	answers any name without a file per name.
-->
<script lang="ts">
	import { onMount } from 'svelte';
	import { goto } from '$app/navigation';
	import { page } from '$app/state';
	import { arriving, open, openPreview } from '#lib/layout/shell.ts';
	import { withDb } from '#lib/database.ts';

	// Every way out of this page stays in the database the reader is in
	// (da-vk1x): `?db` is the page's whole state, so a navigation that drops
	// it is a navigation into another database.
	const home = withDb('/');

	let unknown: string | null = $state(null);

	onMount(() => arriving(enter()));

	async function enter(): Promise<void> {
		const name = page.params.name ?? '';
		// `|| null`: an empty `?from=` is no `from`, not a layout called "".
		const from = page.url.searchParams.get('from') || null;
		// Either door answers with the name that is NOTHING, or null.
		const missing = from === null ? ((await open(name)) ? null : name) : await openPreview(name, from);
		if (missing === null) await goto(home, { replace: true });
		else unknown = missing;
	}
</script>

{#if unknown !== null}
	<p class="notice">
		nothing is called “{unknown}” — no layout, no widget. <a href={home}>home</a>
	</p>
{/if}

<style>
	.notice {
		margin: 1rem 0;
		color: var(--quiet);
		font-style: italic;
	}

	.notice a {
		color: var(--accent);
	}
</style>
