<!--
	The logo, which is also the database switcher (da-y992).

	vividn, 2026-09-16 22:26: "The datum logo, we should remove the '.day' and
	give it a nice font, maybe we start with Geo. then make this logo
	actually a dropdown for selecting different dbs […] And the base color
	of the app changes depending on the db you select. That way it is both a
	logo and functional."

	vividn, on PR #25: "the whole logo should actually just be the database
	name. So when you're in the finance db, there is no datum word down
	there at all". So the mark is the NAME of the database the page is on,
	and the pieces are one thing:

	- it READS the database's name, in Gemunu Libre Light, 300 (vividn on
	  PR #25: "let's use Gemunu Libre", then "increase the font weight to
	  300";
	  SIL OFL, vendored — see app.css). On the default database that is `datum`, for no reason but
	  that the default database is called `datum`: `currentDb()` is the
	  text, with no special case. The product's own name lives in the tab
	  title and in the gear menu's beta note.
	- a name can be long, and it must never cost the prompt beside it any
	  room. So the name does not size the bottom bar's brand block at all —
	  the block is a size container of a FIXED width (PromptBar.svelte) —
	  and the name's type is fitted INTO that width:
	  `100cqi` shared across the name's length, between a floor and the full
	  size. Past the floor the name ellipsizes; the whole name is in the
	  button's title, its label and the menu.
	- a small DOWN ARROW after the name says it opens, on hover (vividn: "a
	  little down arrow … but only show when hovering over it") and on
	  keyboard focus, which is the keyboard's hover. Its box is always
	  there and only its ink comes and goes, so the name never moves; and it
	  sits OUTSIDE the ellipsizing box, so a cut name and the arrow never
	  compete for the same pixels.
	- it OPENS the pool: every database this device has ($lib/databases.ts),
	  the page's own marked, and `new database` under them. `_internal` is
	  not offered — it is the app's own furniture, and `?db` refuses it
	  anyway ($lib/database.ts).
	- it is DRAWN IN the database's colour, which is the whole page's accent
	  and the tint of its grounds (`--hue`, routes/+layout.svelte, app.css).

	Choosing is a FULL navigation, not `goto`. Two listeners are keyed to
	the page's database once, at mount — the root frame's and the shell's
	(`onExternalChange(…, currentDb())`) — so a tab that changed database
	under its own feet would go on hearing the one it left and ignoring the
	one it is on (vremya on #22). The URL is the whole of the state; a load
	is the honest way to change it, and it costs one wasm compile the
	browser has cached anyway.

	The pool is re-read whenever the logo is pointed at or focused, which is
	just before it can be pressed: a database another tab (or the prompt)
	made a moment ago is in the list, and the list an OPEN menu is showing
	updates under the reader rather than going stale.
-->
<script lang="ts">
	import { onMount } from 'svelte';
	import PaneMenu from '$lib/layout/PaneMenu.svelte';
	import type { Item } from '$lib/layout/menu.js';
	import { currentDb, DEFAULT_DB, INTERNAL_DB, withDb } from '$lib/database.js';
	import { ask } from '$lib/confirm.js';
	import { listDatabases } from '$lib/databases.js';
	import { page } from '$app/stores';

	$: db = $page.url ? currentDb() : DEFAULT_DB;

	/** The pool, as the menu last read it. */
	let pool: string[] = [];

	onMount(refresh);

	async function refresh(): Promise<void> {
		pool = await listDatabases();
	}

	/**
	 * The names offered, in one sorted order: the pool, less the app's own,
	 * and always the default and the page's own — the pool is the files
	 * that EXIST, and a database named in the URL has none until something
	 * writes to it.
	 */
	$: names = [...new Set([DEFAULT_DB, db, ...pool])]
		.filter((name) => name !== INTERNAL_DB)
		.sort();

	/** The tick is the paint; `checked` is the fact (PaneMenu's `menuitemradio`). */
	$: items = [
		...names.map(
			(name): Item => ({
				label: name,
				glyph: name === db ? '✓' : ' ',
				checked: name === db,
				run: () => open(name)
			})
		),
		{ label: 'new database', glyph: ' ', act: 'open' as const, run: () => void create() }
	] satisfies Item[];

	/** This page, on `name`. A load, not a `goto` — see the note above. */
	function open(name: string): void {
		if (name === db) return;
		location.assign(withDb(location.pathname, name));
	}

	/**
	 * Ask for a name — in the app's own dialog (vividn on PR #25: "lets use
	 * our own dialog") — and go there. The CLI creates the database on the
	 * first command the page runs against it; a name that is not one —
	 * `_internal`, a path — is refused by the page it lands on, which says
	 * so rather than being silently ignored here (routes/+layout.svelte).
	 */
	async function create(): Promise<void> {
		const name = await ask('Name the new database:', 'open it');
		if (name) open(name);
	}
</script>

<div class="brand-menu">
	<PaneMenu
		{items}
		look="plain"
		class="brand"
		label="databases"
		title="database: {db} — choose a database"
		aria-label="database: {db} — choose a database"
		style="--len: {db.length}"
		on:pointerenter={refresh}
		on:focus={refresh}><span class="name">{db}</span></PaneMenu
	>
</div>

<style>
	/* A fingertip's 24px, on every screen (conventions #12,
	   reachable.test.mjs) — the logo is a control now, not a word. */
	.brand-menu {
		display: flex;
		min-width: 0;
	}

	.brand-menu :global(.brand) {
		min-height: 24px;
		position: relative;
		display: flex;
		align-items: center;
		min-width: 0;
		max-width: 100%;
		color: var(--accent);
		font-family: 'Gemunu Libre', ui-rounded, 'Trebuchet MS', Verdana, sans-serif;
		/* Asked for by weight: the vendored face is 300 alone, and a rule
		   that asked for 400 would have the browser synthesise or reach past
		   it into the fallback. */
		font-weight: 300;
		/* The width the block gives (`100cqi`) shared across the name: the
		   face's glyphs average 0.44em at weight 300 (`datum` is 2.2em,
		   measured in Chromium), a little more for a name heavy with `m`
		   and `w`, and the arrow after the name takes most of another em. Full size for a short name, a floor for
		   a long one — and past the floor, the ellipsis below.

		   The full size is 1.15rem rather than the 1.45rem it was: da-ckw3
		   put the mark on ONE row beside the clock, and vividn — "The size of
		   the db/logo can be reduced some to save some space" — asked that
		   row's space back for the box. It went back up on 2026-09-18:
		   "Increase the size of the db/logo, you can sacrifice the margin in
		   between it and the clock more."

		   No `+ 0.8` for the arrow any more: it is out of the flow below, so
		   the name has the whole block and ends where the block ends — which
		   is what puts it beside the clock rather than an arrow's width short
		   of it. */
		font-size: clamp(0.85rem, 100cqi / (var(--len) * 0.46), 1.5rem);
		line-height: 1;
		letter-spacing: 0.02em;
	}

	.brand-menu :global(.brand:hover),
	.brand-menu :global(.brand[aria-expanded='true']) {
		color: var(--accent-bright);
	}

	.brand-menu :global(.brand:focus-visible) {
		outline: 1px solid var(--accent);
		outline-offset: 2px;
	}

	/* Cut here, not on the button: the arrow after it keeps its own box. */
	.name {
		min-width: 0;
		overflow: hidden;
		text-overflow: ellipsis;
		white-space: nowrap;
	}

	/* The arrow: generated content with an empty alternative (`/ ''`), so
	   it is neither in the name's text nor in the button's label. In a face
	   of its own on purpose: Gemunu Libre's latin subset has no `▾`
	   (measured), so asking the name's face for it would hand the glyph to
	   whichever fallback happened to have one. It is a control's mark, not
	   a letter of the name, and a monospace triangle is the same shape
	   everywhere. */
	/* OUT OF THE FLOW, so it reserves no width: it is invisible until a
	   pointer is over the name, and a phone has no pointer at all — an arrow
	   nobody will ever see should not push the mark away from the clock
	   (vividn, 2026-09-18). It hangs just past the name, where it was. */
	.brand-menu :global(.brand::after) {
		position: absolute;
		left: 100%;
		top: 50%;
		transform: translateY(-50%);
		content: '\25BE' / '';
		margin-left: 0.15em;
		font-family: ui-monospace, SFMono-Regular, Menlo, Consolas, monospace;
		font-size: 0.6em;
		opacity: 0;
		transition: opacity 120ms ease-out;
	}

	.brand-menu :global(.brand:hover::after),
	.brand-menu :global(.brand:focus-visible::after) {
		opacity: 1;
	}

	@media (prefers-reduced-motion: reduce) {
		.brand-menu :global(.brand::after) {
			transition: none;
		}
	}
</style>
