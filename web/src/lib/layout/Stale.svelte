<!--
	A widget's output from the last visit, laid over it until the fresh one
	lands (da-bhudz).

	vividn, 2026-10-07: «ideally there would be no loading spinner at all. Just
	get the most recent data painted immediately somehow.» So every widget's
	drawn content is kept when the page is hidden or left — the database's
	last left, by widget id — and drawn over the widget, dimmed, with the tree. The widget
	itself mounts once the bar can take a command (`live`, Layout.svelte);
	from then, the first change to its content after the CLI has answered
	anything takes the copy away, and so does the CLI's queue running dry:
	a widget whose fresh answer draws exactly what it drew before has
	nothing to change. A queue ALREADY dry two frames after the widget went
	live is the same news (da-qhu1g): a REPL asks the CLI nothing, and on a
	tab opened through `/repl` the tree mounts after the boot's commands have
	answered, so nothing was coming to run dry — unless the tab led the sync,
	whose own read took the copy down by accident, which is why it stayed
	only beside another tab. Two frames, because a widget that measures its
	box before it asks (the tail, the strip) asks in the first.

	The copy is in a CLOSED shadow root, adopting the page's own rules:
	nothing that queries the document — the app, or a test's locator — can find a stale row and take it for a live one, and the
	copy's ids cannot collide with the live widget's. `inert`, so nothing in
	it takes a pointer or the focus.
-->
<script context="module" lang="ts">
	import { get } from 'svelte/store';
	import { currentDb } from '$lib/database.js';
	import { dry } from '$lib/db.js';
	import { first, idle } from '$lib/waiting.js';
	import { STALE } from './draft.js';

	/**
	 * The most the copies may take of `localStorage`, in characters, the largest
	 * going first: one database's at a time, under one key, so this is the
	 * whole of the cache — and `draft.ts` drops it when the reader's own
	 * layout state cannot be written (vremya on #485).
	 */
	const CAP = 1_500_000;

	/** The copies as last read, and the database they are of: `?db=` may change without a reload. */
	let kept: { db: string; copies: Record<string, string> } | null = null;

	function load(): Record<string, string> {
		const db = currentDb();
		if (kept?.db !== db) {
			let copies: Record<string, string> = {};
			try {
				const stored = JSON.parse(localStorage.getItem(STALE) ?? 'null');
				if (stored?.db === db) copies = stored.copies ?? {};
			} catch {
				// Refused or unreadable: nothing from the last visit.
			}
			kept = { db, copies };
		}
		return kept.copies;
	}

	/** Every widget's content as drawn now, for the next load: a copy still showing keeps its own. */
	function keep(): void {
		if (!get(first)) return;
		const next: Record<string, string> = {};
		for (const body of document.querySelectorAll<HTMLElement>('[data-kept]')) {
			const id = body.dataset.kept as string;
			const html = body.querySelector(':scope > [data-stale]') ? load()[id] : body.innerHTML;
			if (html) next[id] = html;
		}
		let size = Object.values(next).reduce((sum, html) => sum + html.length, 0);
		for (const [id, html] of Object.entries(next).sort(([, a], [, b]) => b.length - a.length)) {
			if (size <= CAP) break;
			delete next[id];
			size -= html.length;
		}
		kept = { db: currentDb(), copies: next };
		try {
			localStorage.setItem(STALE, JSON.stringify(kept));
		} catch {
			// Refused or full: the next load draws each widget when it answers.
		}
	}

	/** The page's own rules, for a shadow root to adopt: read again only when a sheet is added. */
	let sheets: { count: number; adopted: CSSStyleSheet[] } | null = null;

	function styles(): CSSStyleSheet[] {
		const count = document.styleSheets.length;
		if (sheets?.count !== count) {
			const own = new CSSStyleSheet();
			own.replaceSync('.stale-copy { display: flex; flex-direction: column; flex: 1; min-height: 0; opacity: 0.5; }');
			const adopted = [...document.styleSheets].flatMap((sheet) => {
				try {
					const copy = new CSSStyleSheet();
					copy.replaceSync([...sheet.cssRules].map((rule) => rule.cssText).join('\n'));
					return [copy];
				} catch {
					return []; // Another origin's (a font's): its faces reach the shadow tree anyway.
				}
			});
			sheets = { count, adopted: [...adopted, own] };
		}
		return sheets.adopted;
	}

	if (typeof document !== 'undefined') {
		addEventListener('pagehide', keep);
		document.addEventListener('visibilitychange', () => document.hidden && keep());
	}
</script>

<script lang="ts">
	import { onMount, tick } from 'svelte';

	/** The widget's id, which is what its copy is kept under. */
	export let id: string;
	/** Whether the widget is mounted: a change to its content counts from then on. */
	export let live = false;
	let watch: (() => void) | undefined;
	$: if (live && watch) void tick().then(watch);

	let host: HTMLElement;
	let gone = false;

	onMount(() => {
		const body = host.parentElement as HTMLElement;
		body.dataset.kept = id;
		const forget = () => delete body.dataset.kept;
		const html = load()[id];
		if (!html) {
			gone = true;
			return forget;
		}
		const root = host.attachShadow({ mode: 'closed' });
		root.adoptedStyleSheets = styles();
		root.append(Object.assign(document.createElement('div'), { className: 'stale-copy', innerHTML: html }));

		const observer = new MutationObserver(() => get(first) && done());
		let unsubscribe: (() => void) | undefined;
		watch = () => {
			if (unsubscribe) return;
			observer.observe(body, { subtree: true, childList: true, characterData: true });
			const drained = get(idle);
			unsubscribe = idle.subscribe((n) => n !== drained && done());
			requestAnimationFrame(() => requestAnimationFrame(() => dry() && done()));
		};
		function done() {
			gone = true;
			observer.disconnect();
			unsubscribe?.();
		}
		return () => {
			done();
			forget();
		};
	});
</script>

{#if !gone}
	<div bind:this={host} class="stale" data-stale inert aria-hidden="true" title="from your last visit"></div>
{/if}

<style>
	.stale {
		position: absolute;
		inset: 0;
		z-index: 1;
		display: flex;
		flex-direction: column;
		background: var(--ground);
		overflow: hidden;
	}
</style>
