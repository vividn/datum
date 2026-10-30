<!--
	The app shell.

	The root tab bar above, and under it the PAGE: a scroller holding three
	boxes — the dashboard, the log, and the bottom bar at the foot, which is
	the output over the input (da-ckw3, da-mwp4m). The document itself still never scrolls, and neither does
	this box under any gesture of the browser's: it is `overflow: hidden`
	and the only thing that moves it is the bar's own drag and wheel
	(#lib/log.ts, which has the arithmetic that makes the two rest states
	exact). Panes inside the page scroll individually (see #lib/Pane.svelte),
	so at rest the bottom bar — the prompt, the brand, the receipt — stays
	put whatever the page does, on a phone too.

	The shell is FIXED to the viewport the browser paints, not sized in
	viewport units (da-31ts): an installed PWA resolved `100dvh` before its
	system bars settled, too tall, and hung the bottom bar below the screen
	until a rotation re-resolved it. A fixed box follows the browser's own
	chrome sliding in and out, which is what `100dvh` was there for, and
	`followViewport` narrows it to what is VISIBLE — above an on-screen
	keyboard that only shrinks the visual viewport (iOS) — measured on every
	viewport change and once more after load.
-->
<script lang="ts">
	import { onMount, type Snippet } from 'svelte';
	import '../app.css';
	import { schedule } from '#lib/backups.ts';
	import { installConsole } from '#lib/console.ts';
	import BarLog from '#lib/BarLog.svelte';
	import { attach, glideEnds, logHeight, shown } from '#lib/log.ts';
	import PromptBar from '#lib/PromptBar.svelte';
	import Waiting from '#lib/Waiting.svelte';
	import Damaged from '#lib/Damaged.svelte';
	import { damaged } from '#lib/damaged.ts';
	import RootBar from '#lib/layout/RootBar.svelte';
	import PlaceModal from '#lib/layout/PlaceModal.svelte';
	import Tour from '#lib/Tour.svelte';
	import { boot as bootEngine } from '#lib/layout/engine.ts';
	import { boot as bootShell, openPlace } from '#lib/layout/shell.ts';
	import { approving, joining } from '#lib/syncView.ts';
	import { get, toStore } from 'svelte/store';
	import { currentDb, DEFAULT_DB, hueOf, INTERNAL_DB, refusedDb } from '#lib/database.ts';
	import { browser } from '$app/env';
	import { page } from '$app/state';

	let { children }: { children?: Snippet } = $props();

	// `?db=<name>` (da-vk1x). Off `page.url` rather than `location`, so a
	// client-side navigation that changes the query re-renders the title and
	// the refusal with it; the seam reads the same query for itself.
	const url = toStore(() => page.url);
	const db = $derived($url ? currentDb() : DEFAULT_DB);
	const refused = $derived($url ? refusedDb() : null);

	// The page's base colour says which database it is (da-y992): one hue,
	// derived from the name, and every accent in app.css is drawn from it.
	//
	// Written on the DOCUMENT rather than on `.shell`, because the things
	// that most need to agree with the page are not inside it: a menu, a
	// chooser and a confirm dialog are all moved out to `document.body`
	// (PaneMenu's `portal`), where a custom property set on the shell does
	// not reach.
	$effect.pre(() => {
		if (browser) document.documentElement.style.setProperty('--hue', String(hueOf(db)));
	});

	// `window.datum` — the command seam at the browser's own prompt. Mounted
	// here rather than at module scope because there is no window during
	// prerendering. See #lib/console.ts. Then the two boots: the engine's
	// (the database, the bar's fields) and the shell's (the root tabs),
	// which every route needs — a `/<name>` opens a tab into a booted shell.
	onMount(() => {
		installConsole(window as unknown as Record<string, unknown>);
		void bootEngine();
		void bootShell();
		// Backup plans run while a tab is open (da-9ow8m, #lib/backups.ts).
		schedule();
		reloadOnceForIsolation();
		// A join or approve link still waiting for its card (da-amhrv): the
		// `/sync/…` route opens the place itself, but the isolation reload
		// lands on `/` with the link held in the tab alone.
		if (!location.pathname.startsWith('/sync/') && (get(joining) || get(approving))) void openPlace('sync');
		return followViewport();
	});

	/**
	 * Hand the page's scroller to the bar, which is the only thing that
	 * moves it (da-ckw3, #lib/log.ts).
	 */
	const page_scroller = (el: HTMLElement) => ({ destroy: attach(el) });

	/**
	 * Hand the shell the visible viewport's height and top as
	 * `--viewport-height` / `--viewport-top` (da-31ts), on every change a
	 * phone makes to it — the visual viewport's own `resize` and `scroll`
	 * (a keyboard, the URL bar), `orientationchange`, `pageshow` (a PWA
	 * brought back) — and once more a frame after mounting, because a launch that
	 * measured wrong sends no event to correct itself. vividn: a rotation
	 * cured it, «so maybe just one size refresh is necessary after the page
	 * loads». Not while pinch-zoomed: there the visible part is a
	 * magnifier's view, not a smaller screen.
	 */
	function followViewport() {
		const viewport = window.visualViewport;
		if (!viewport) return;
		const root = document.documentElement.style;
		const measure = () => {
			if (viewport.scale > 1.01) {
				root.removeProperty('--viewport-height');
				root.removeProperty('--viewport-top');
				return;
			}
			root.setProperty('--viewport-height', `${viewport.height}px`);
			root.setProperty('--viewport-top', `${viewport.offsetTop}px`);
		};
		measure();
		const frame = requestAnimationFrame(measure);
		viewport.addEventListener('resize', measure);
		viewport.addEventListener('scroll', measure);
		addEventListener('orientationchange', measure);
		addEventListener('pageshow', measure);
		return () => {
			cancelAnimationFrame(frame);
			viewport.removeEventListener('resize', measure);
			viewport.removeEventListener('scroll', measure);
			removeEventListener('orientationchange', measure);
			removeEventListener('pageshow', measure);
		};
	}

	/**
	 * The service worker adds the isolation headers to everything it serves —
	 * but the first document of a visit was fetched before any worker
	 * controlled the page, so on a host that sends no headers of its own that
	 * document is not isolated. Reload once when the worker takes control,
	 * and only then; from the second load on, isolation is simply there.
	 *
	 * Guarded three ways, because a reload is the one thing this page must
	 * never do twice or at the wrong moment: a session flag so it happens at
	 * most once per tab; only until the visitor first types, taps or clicks,
	 * so nothing they did or were told is lost; and never when already
	 * isolated, which is every load on a deployment whose host sends the
	 * headers itself.
	 *
	 * The visitor and not the prompt (da-6ud02): the prompt is empty again
	 * the moment a command is submitted, and on a real network the worker
	 * takes control after a first command more often than not — so a reload
	 * guarded by the prompt took that command's ERROR with it, or the
	 * command itself while it ran. A tab they have touched waits for the
	 * next load, which is isolated from its first byte.
	 */
	function reloadOnceForIsolation() {
		if (crossOriginIsolated || !('serviceWorker' in navigator)) return;
		const KEY = 'datum-reloaded-for-isolation';
		let touched = false;
		for (const type of ['pointerdown', 'keydown', 'input'])
			addEventListener(type, () => (touched = true), { capture: true, once: true });
		const reload = () => {
			if (touched || sessionStorage.getItem(KEY) === '1') return;
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

<div class="shell" style="--shown: {$shown}px; --log-height: {$logHeight}px" data-glide-ends={$glideEnds}>
	<RootBar />
	{#if $damaged !== null}<Damaged db={$damaged} />{:else}<Waiting />{/if}

	<!-- The page. Moved by the bar and by nothing else: see the header. -->
	<div class="pages" use:page_scroller>
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
				{@render children?.()}
			{/if}
		</main>

		<BarLog />
		<PromptBar />
	</div>
	<PlaceModal />
</div>
<!-- Outside the shell, a size container, which would be its fixed boxes' frame. -->
<Tour />

<style>
	/* See the note above. `100%` is the layout viewport the shell is fixed
	   to; `--viewport-height` the visible part of it, which is never taller,
	   so a stale measurement can only be ignored, never hang the bar below
	   the screen. The insets keep a notch or a home indicator off the
	   prompt. A size container, so a box inside is a share of THIS box
	   rather than of `vh`.

	   THE BAR'S TWO HEIGHTS ARE DECLARED HERE, together and nowhere else
	   (da-ckw3): the bar is the sum of them, and the dashboard is what is
	   left of the page when that sum is taken off it, so the two numbers
	   have to be read in one place by both. vividn: «we may have to play
	   around with the heights some» — this is where. Folded, the bar is the
	   input row alone and the dashboard gains the output's height back;
	   that is the only thing that ever reflows the dashboard. */
	.shell {
		/* vividn, 2026-09-18: "increase fontsize of input bar slightly, and
		   decrease height of input bar so that there is not so much wasted
		   space with padding". The row was 3.6rem for symmetry with the
		   output section; it only ever needed to hold a 1.25rem line, and the
		   rest was padding. The OUTPUT keeps its 3.6rem — that number is the
		   four raw lines and the two-line document it has to hold
		   (BarResult.svelte), and it is not the row's to shrink. */
		--input-height: 2.9rem;
		/* ONE ENTRY TALL (da-7xp82). vividn, 2026-10-28: «it is too tall right
		   now, it should be wide enough for a single entry to fit nicely (+
		   maybe space for one line for extra data) / let's make the font size
		   a bit smaller in the output bar». So it is a strip entry's lines
		   (below) at the strip's own type, with the strip's
		   padding and the rule under it, and one line more wherever the
		   share of the page it was (da-mwp4m: 15cqh, 121px on a 390×844
		   phone) has room for it. A short phone (412×563, 15cqh 77px) gets
		   the entry alone: the line more would take the sliver of yesterday
		   from the day strip above (da-frewq). */
		--output-font: 0.8rem;
		--output-line: calc(var(--output-font) * 1.35);
		--output-pad: 0.25rem;
		/* Since da-hxjrk the entry is three lines — the command, the field and
		   state, the time — in two frames, each with its label standing on its
		   top border: the command's at 0.9em tall and the entry's at 0.84em,
		   the command's 0.15em under its last line (CliEntry.svelte,
		   Doc.svelte), and the frames' four borders. */
		--output-entry: calc(
			3 * var(--output-line) + 1.89 * var(--output-font) + 2 * var(--output-pad) + 4px + 1px
		);
		--output-height: clamp(var(--output-entry), 15cqh, calc(var(--output-entry) + var(--output-line)));
		/* The hairline that separates the bar from the dashboard is part of
		   the bar's box, so it belongs in the sum: without it the sections
		   add up to a pixel more than the bar they are in, and the output's
		   foot hangs one pixel below the screen. */
		--bar-rule: 1px;
		/* The bar is the input row, the rule, and HOWEVER MUCH of the output
		   section is open right now — `--shown`, which the drag moves and
		   which is 0 shut and `--output-height` at the notch (#lib/log.ts).
		   The dashboard is the page less the bar and less the log, so all
		   three follow one number and none of them can disagree. */
		--bar-height: calc(var(--input-height) + var(--shown, 0px) + var(--bar-rule));
		position: fixed;
		top: var(--viewport-top, 0px);
		left: 0;
		right: 0;
		display: flex;
		flex-direction: column;
		height: min(100%, var(--viewport-height, 100%));
		padding: env(safe-area-inset-top, 0px) env(safe-area-inset-right, 0px)
			env(safe-area-inset-bottom, 0px) env(safe-area-inset-left, 0px);
		container-type: size;
	}

	/* The page: the dashboard, the log, the bar. `overflow: hidden` and a
	   size container — hidden because the browser must never move this box
	   (a rubber-band, a flick, a focus), and a container because the three
	   heights inside are shares of it and `cqh` is the one unit that says
	   so without a percentage chain. The bar's own code moves it
	   (#lib/log.ts).

	   TWO SIZE CONTAINERS, NESTED, ON PURPOSE (a canary on PR #46 asked why).
	   `cqh` resolves against the NEAREST one, so `100cqh` in `main` and in
	   `.log` is this box — the page under the root tab bar — and never the
	   shell, which is that bar plus this box. That is exactly what the
	   arithmetic wants: the travel is the dashboard's height, and the
	   dashboard is the page less the bar. The shell keeps its own container
	   because it is what a box measured against the WHOLE window wants. */
	.pages {
		flex: 1;
		min-height: 0;
		display: flex;
		flex-direction: column;
		overflow: hidden;
		overscroll-behavior: none;
		container-type: size;
	}

	.notice {
		margin: 1rem 0.75rem;
		color: var(--quiet);
		font-style: italic;
	}

	.notice a {
		color: var(--accent);
	}

	/* The dashboard: the page less the bar. That number is also the whole of
	   the travel into the log, which is what puts the bar's top on the top
	   of the screen exactly when the scroll runs out (#lib/log.ts). */
	main {
		flex: none;
		height: calc(100cqh - var(--bar-height) - var(--log-height, 0px));
		/* A flex item will not shrink under its content unless told, and this
		   one has to reach ZERO: at the log's tallest the dashboard is gone
		   and the bar's top is the top of the page. Without this it stopped
		   24px short and the three boxes overflowed the one they are in. */
		min-height: 0;
		/* The frame fills this box exactly and scrolls its own panes. */
		overflow: auto;
		/* NO PADDING HERE. A box-sized element cannot be shorter than its own
		   padding, so 0.75rem all round was a 24px floor — and this box has to
		   reach ZERO when the log is at its tallest. The air moved inside, to
		   the frame and the notice, which collapse with it. */
	}
</style>
