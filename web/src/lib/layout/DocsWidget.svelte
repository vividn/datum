<!--
	The docs widget (da-0d8l6), a place: datum's docs read inside the page.
	vividn, 2026-11-05: «you should be able to explore the docs without leaving
	the app, so make "read the docs" a button that opens some documentation
	widget that let's you explore». The welcome's «Read the docs» opens it
	(`openPlace('docs')`), a modal on a desktop and a tab on a phone.

	The pages are the repo's `docs/` as written, bundled at build (each its own
	chunk, fetched when first opened), never a copy kept here, so they cannot
	drift from the repo's; `docs/README.md` is the contents. They are drawn by
	#lib/markdown.ts. A link to another doc opens it here, «← back» returning;
	any other link opens in a new tab, a file of the repo on GitHub.

	Under the contents, the guides (da-oefd0, #lib/guides.ts): the page's and
	each widget's, from the registry, as pages `widgets/<kind>.md` of their own.
-->
<script lang="ts">
	import Pane from '#lib/Pane.svelte';
	import { act } from '#lib/act.ts';
	import { PAGE, guideOf, guided } from '#lib/guides.ts';
	import { markdown } from '#lib/markdown.ts';

	const REPO = 'https://github.com/vividn/datum/blob/main';
	const CONTENTS = 'README.md';
	/** Each doc by its path under `docs/`, read when it is first opened. */
	const DOCS = Object.fromEntries(
		Object.entries(import.meta.glob<string>('../../../../docs/**/*.md', { query: '?raw', import: 'default' })).map(([file, load]) => [
			file.replace('../../../../docs/', ''),
			load
		])
	);
	/** Each guide, as a doc of its own. */
	const guidePage = (name: string) => {
		const guide = guideOf(name);
		return `# ${guide?.title ?? name}\n\n${guide?.text ?? ''}\n`;
	};
	for (const name of [PAGE, ...guided()]) DOCS[`widgets/${name}.md`] = async () => guidePage(name);
	/** The contents, and the guides under them. */
	const GUIDES = [
		'',
		'## Using the page',
		'',
		`- [${guideOf(PAGE)?.title}](widgets/${PAGE}.md): tabs and layouts, widgets and their menus, the bar.`,
		...guided().map((name) => `- [${guideOf(name)?.title}](widgets/${name}.md)`),
		''
	].join('\n');

	/** The docs' pictures, by the same paths: URLs of the build's own copies. */
	const IMAGES = Object.fromEntries(
		Object.entries(import.meta.glob<string>('../../../../docs/**/*.{svg,png}', { query: '?url', import: 'default', eager: true })).map(
			([file, url]) => [file.replace('../../../../docs/', ''), url]
		)
	);

	/** The doc shown, and the docs before it, newest last. */
	let path = $state(CONTENTS);
	let trail: string[] = $state([]);
	let html = $state('');
	let failed = $state(false);
	let body: HTMLDivElement | undefined = $state();

	/** `href`, as `from` wrote it, as a path from the repo's root, and its anchor. */
	function resolve(href: string, from: string): { file: string; hash: string } {
		const url = new URL(href, `https://repo/docs/${from}`);
		return { file: decodeURIComponent(url.pathname.slice(1)), hash: url.hash.slice(1) };
	}

	const attr = (text: string) => text.replace(/&/g, '&amp;').replace(/"/g, '&quot;');

	/** A link in `from`: another doc stays here, an anchor scrolls, anything else is a new tab. */
	function link(href: string, from: string): string {
		if (href.startsWith('#')) return `href="${attr(href)}" data-anchor="${attr(href.slice(1))}"`;
		if (/^[a-z][a-z+.-]*:/i.test(href)) return `href="${attr(href)}" target="_blank" rel="noopener"`;
		const { file, hash } = resolve(href, from);
		const doc = file.replace(/^docs\//, '');
		if (file.startsWith('docs/') && doc in DOCS)
			return `href="${attr(`${REPO}/${file}`)}" data-doc="${attr(doc)}" data-anchor="${attr(hash)}"`;
		return `href="${attr(`${REPO}/${file}`)}" target="_blank" rel="noopener"`;
	}

	function image(src: string, from: string): string {
		if (/^[a-z][a-z+.-]*:/i.test(src)) return src;
		const { file } = resolve(src, from);
		return IMAGES[file.replace(/^docs\//, '')] ?? `${REPO}/${file}?raw=true`;
	}

	/** Draw `doc`, at its heading `hash` or its top; the latest asked wins. */
	let asked = 0;
	async function show(doc: string, hash = ''): Promise<void> {
		const at = ++asked;
		let drawn: string | null = null;
		try {
			drawn = markdown((await DOCS[doc]()) + (doc === CONTENTS ? GUIDES : ''), { link: (href) => link(href, doc), image: (src) => image(src, doc) });
		} catch {
			// Not fetched (offline before it was cached), or not drawn (a target `decodeURIComponent` refuses).
			drawn = null;
		}
		if (at !== asked) return;
		path = doc;
		failed = drawn === null;
		html = drawn ?? '';
		requestAnimationFrame(() => scrollTo(hash));
	}

	function scrollTo(hash: string): void {
		if (!body) return;
		const heading = hash ? body.querySelector(`[id="${CSS.escape(hash)}"]`) : null;
		if (heading) heading.scrollIntoView({ block: 'start' });
		else body.scrollTop = 0;
	}

	function go(doc: string, hash: string): void {
		trail = [...trail, path];
		void show(doc, hash);
	}

	function back(): void {
		const doc = trail.at(-1);
		if (doc === undefined) return;
		trail = trail.slice(0, -1);
		void show(doc);
	}

	/** A click on a link: a doc or an anchor is ours; a modified click, or any other link, the browser's. */
	function clicked(event: MouseEvent): void {
		const a = (event.target as Element).closest?.('a[data-anchor]');
		if (!(a instanceof HTMLAnchorElement) || event.button !== 0 || event.metaKey || event.ctrlKey || event.shiftKey || event.altKey) return;
		event.preventDefault();
		const doc = a.dataset.doc;
		if (doc === undefined) scrollTo(a.dataset.anchor ?? '');
		else if (doc === path) scrollTo(a.dataset.anchor ?? '');
		else go(doc, a.dataset.anchor ?? '');
	}

	void show(CONTENTS);
</script>

<Pane title="docs" bind:body>
	<nav class="docs-nav">
		<button type="button" class="back" use:act={'show'} disabled={trail.length === 0} onclick={back}>← back</button>
		{#if path !== CONTENTS}
			<button type="button" class="contents" use:act={'show'} onclick={() => go(CONTENTS, '')}>contents</button>
		{/if}
		<span class="path">docs/{path}</span>
	</nav>
	{#if failed}
		<p class="quiet">docs/{path} could not be read.</p>
	{/if}
	<!-- The clicks are its links', which a key reaches as a link. -->
	<!-- svelte-ignore a11y_click_events_have_key_events, a11y_no_noninteractive_element_interactions -->
	<article class="doc" data-doc={path} onclick={clicked}>{@html html}</article>
</Pane>

<style>
	.docs-nav {
		display: flex;
		align-items: baseline;
		gap: 0.5rem;
		margin-bottom: 0.5rem;
	}
	.path {
		color: var(--quiet);
		font-size: 0.8rem;
		overflow-wrap: anywhere;
	}
	.quiet {
		color: var(--quiet);
		font-style: italic;
	}
	/* Read, not scanned: a measure a sentence fits, as the welcome's. */
	.doc {
		max-inline-size: 46rem;
		color: var(--ink);
		line-height: 1.45;
		overflow-wrap: anywhere;
	}
	.doc :global(h1),
	.doc :global(h2),
	.doc :global(h3),
	.doc :global(h4) {
		margin: 1rem 0 0.4rem;
		color: var(--ink-bright);
		line-height: 1.25;
	}
	.doc :global(h1) {
		margin-top: 0;
		font-size: 1.4rem;
	}
	.doc :global(h2) {
		font-size: 1.15rem;
	}
	.doc :global(h3),
	.doc :global(h4) {
		font-size: 1rem;
	}
	.doc :global(p),
	.doc :global(ul),
	.doc :global(ol),
	.doc :global(pre),
	.doc :global(table) {
		margin: 0 0 0.6rem;
	}
	.doc :global(ul),
	.doc :global(ol) {
		padding-inline-start: 1.4rem;
	}
	.doc :global(li > ul),
	.doc :global(li > ol) {
		margin: 0;
	}
	.doc :global(a) {
		color: var(--accent-bright);
	}
	.doc :global(code) {
		font-family: ui-monospace, SFMono-Regular, Menlo, Consolas, monospace;
		font-size: 0.9em;
		color: var(--ink-bright);
	}
	/* A command's lines keep their columns, so a wide one scrolls on its own. */
	.doc :global(pre) {
		padding: 0.5rem;
		overflow-x: auto;
		background: var(--raised);
		border: 1px solid var(--rule);
		overflow-wrap: normal;
	}
	.doc :global(table) {
		display: block;
		overflow-x: auto;
		border-collapse: collapse;
		overflow-wrap: normal;
	}
	.doc :global(th),
	.doc :global(td) {
		padding: 0.2rem 0.5rem;
		border: 1px solid var(--rule);
		text-align: start;
		vertical-align: top;
	}
	.doc :global(img) {
		max-inline-size: 100%;
	}
	.doc :global(hr) {
		border: 0;
		border-top: 1px solid var(--rule);
	}
</style>
