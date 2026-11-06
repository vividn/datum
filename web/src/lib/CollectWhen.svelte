<!--
	Collect's time and duration section (da-us1n1), a column between the now
	strip and the rows, at rest and beside a field's view alike. vividn,
	2026-11-05: «two sections: (time) (-1m) (-5m) ---- (dur) (1m) (5m).
	clicking time or dur gives you more advanced options … these more
	advanced options appear by expanding out this section to reveal more
	options, you can drag it closed again», and «clicking -1m, -5m, 1m or 5m
	doesn't expand the section either, it changes the dur or time button to
	be the time or dur pill, and keeps summing up».

	Its time and length are the page's selection (#lib/cursor.ts), so `−1m`
	and `5m` move the very pills the nowview makes and draws, and a × on the
	bar goes through the cursor's reconciler. Each pill here says its sum
	(`−6m`, `6m`) and drags sideways to nudge it (da-axrdv), a minute for
	every few pixels and faster the further it goes; its × clears it.

	The advanced options are what the cursor alone cannot say quickly: a
	time typed as `-t` reads it (`9:15am`, `yesterday 10am`), resolved by the
	CLI's own parser with no database (`nowview --overlay --json`), `-y` a
	day back, `-d` a date at the same clock, and `-D` the whole day, a pill
	of its own beside the time, which names the day, taken by a command that
	lists `--full-day`; a length typed, read by the same CLI as `dur=` reads
	one (`--context`: `1.5h` is 90 minutes, `1h30` refused), and `-m`, a
	moment with no length, its own pill as the length's other face. A moment
	field open has no length to give, so the dur half is off beside it.

	At rest it is a clock alone, between the strip and the rows, and a tap on
	the clock opens it, the clock then standing between its halves (vividn,
	2026-11-06: «hide it by default. But a little clock button in the middle
	between the fields section and the now section, clicking it toggles
	whether this time panel is open»); whether it is open is the session's,
	as collect's zoom is. `time` and `dur` wear a pair of triangles, and drag
	sideways with no pill yet too: `time` from now, `dur` from no length
	(«add a little triangle arrow to the time and dur buttons so that you
	know you can drag them. Allow dragging them left and right, even when
	there is nothing yet selected»).
-->
<script module lang="ts">
	import { pills } from './bar.js';

	export const MOMENT_ID = 'moment';
	export const ALLDAY_ID = 'allday';

	/** `-m`, a moment with no length, on the bar or off it. */
	export function setMoment(on: boolean): void {
		pills.update((all) => [...all.filter((p) => p.id !== MOMENT_ID), ...(on ? [{ id: MOMENT_ID, kind: 'moment', text: '-m' }] : [])]);
	}

	/** `-D`, the whole day the time names, on the bar or off it. */
	function setAllday(on: boolean): void {
		pills.update((all) => [...all.filter((p) => p.id !== ALLDAY_ID), ...(on ? [{ id: ALLDAY_ID, kind: 'allday', text: '-D' }] : [])]);
	}
</script>

<script lang="ts">
	import { get } from 'svelte/store';
	import LengthArrow from './LengthArrow.svelte';
	import { act } from './act.js';
	import { CLICK_SLOP } from './inspect.js';
	import { run } from './run.js';
	import { lengthReadArgv, timeReadArgv, type Reads } from './calls.js';
	import { SPAN_ID, cursor, lengthArg, pillSections, pointAt, pointed, type Cursor } from './cursor.js';

	let {
		disabled = false,
		/** Which edge the strip stands at: the section opens away from it. */
		side = 'left',
		/** How far from that edge the section stands: the strip's width. */
		offset = 0,
		/** The section's own width, for the rows to stand past it. */
		width = $bindable(0),
		/** No length to give: a moment field is open, so the dur half is off (vividn, 11-03 16:07). */
		noLength = false,
		/** Open, or a clock alone (da-iunoo): the session's, as collect's zoom is. */
		shown = false,
		/** The clock tapped: whether it is open now, for the session to keep. */
		onshown
	}: { disabled?: boolean; side?: 'left' | 'right'; offset?: number; width?: number; noLength?: boolean; shown?: boolean; onshown?: (shown: boolean) => void } = $props();
	const lengthOff = $derived(disabled || noLength);

	/**
	 * Open: the setting, until the clock is tapped (a widget with no session to
	 * keep it, the tutorial's, keeps it here). A `$derived` assigned to holds
	 * the value assigned until `shown` changes (Svelte 5.25's overridable
	 * derived; this is 5.57).
	 */
	let expanded = $derived(shown);
	function toggleShown() {
		expanded = !expanded;
		if (!expanded) open = null;
		onshown?.(expanded);
	}

	const MINUTE = 60_000;
	/**
	 * How tall the section must be to stand as one column; shorter, its two
	 * halves stand side by side, each a column; and in a pane one row of
	 * buttons tall (the desktop's) it is a row, `time −1m −5m | dur 1m 5m`.
	 * Each is a little over the height its buttons take, spaced (da-iunoo:
	 * 247px and 104px measured), so neither scrolls.
	 */
	const COLUMN_PX = 256;
	const HALVES_PX = 112;
	let tall = $state(COLUMN_PX);

	/** Which part's advanced options are out, or none. */
	let open: 'time' | 'dur' | null = $state(null);

	const timePill = $derived($pills.find((p) => p.kind === 'time') ?? null);
	const allday = $derived($pills.some((p) => p.id === ALLDAY_ID));
	const moment = $derived($pills.some((p) => p.id === MOMENT_ID));
	const span = $derived($pills.some((p) => p.id === SPAN_ID) ? ($cursor?.span ?? null) : null);

	/** What the time pill says: the sum back from now, or the clock it names, with `-D` after. */
	const timeLabel = $derived.by(() => {
		const c = $pointed;
		const time = !timePill || !c ? null : c.rel === undefined ? pillSections(c.at).map((s) => s.text).join(' ') : Number.isInteger(c.rel) ? `−${lengthArg(c.rel)}` : timePill.text;
		return [time, allday ? '-D' : null].filter(Boolean).join(' ') || null;
	});
	const durLabel = $derived(moment ? '−m' : span ? lengthArg(span) : null);

	/** Where a selection is, without its length. */
	const where = (c: Cursor) => (c.rel !== undefined ? { rel: c.rel, from: c.from } : { at: c.at, step: 1, from: c.from });

	/** `−1m`, `−5m`: that much earlier, summing. */
	function earlier(by: number) {
		const c = get(cursor);
		if (c && c.rel === undefined) return pointAt({ at: new Date(+c.at - by * MINUTE), step: 1, span: c.span, from: c.from });
		pointAt({ rel: (c?.rel ?? 0) + by, span: c?.span, from: c?.from });
	}
	/** `1m`, `5m`: that much longer, summing, in place of `-m`. */
	function longer(by: number) {
		setMoment(false);
		const c = get(cursor);
		pointAt({ ...(c ? where(c) : { rel: 0 }), span: (c?.span ?? 0) + by });
	}
	/** The time pill's ×: the length, if any, ends at now again. */
	function clearTime() {
		setAllday(false);
		const c = get(cursor);
		pointAt(c?.span ? { rel: 0, span: c.span } : null);
	}
	/** The length pill's ×. */
	function clearDur() {
		setMoment(false);
		const c = get(cursor);
		if (c) pointAt(where(c));
	}

	/** Point at `at`, keeping the length. */
	const pointTo = (at: Date) => pointAt({ at, step: 1, span: get(cursor)?.span });
	/** The instant pointed at, or now. */
	const pointedAt = () => get(pointed)?.at ?? new Date();

	// ---- the advanced options ------------------------------------------

	let typedTime = $state('');
	let typedDur = $state('');
	let badTime = $state(false);
	let badDur = $state(false);

	/** A time typed: whatever `-t` reads, as the CLI reads it. */
	async function typeTime() {
		const said = typedTime.trim();
		if (!said) return;
		const got = await run(timeReadArgv(said));
		let at: number | null = null;
		try {
			at = got.code === 0 ? ((JSON.parse(got.stdout) as Reads['timeReadArgv']).at_ms ?? null) : null;
		} catch {
			// Not an answer: the box says so.
		}
		badTime = at === null;
		if (at !== null) (pointTo(new Date(at)), (typedTime = ''));
	}
	/** `-y`: a day back, at the same clock. */
	function dayBack() {
		const at = new Date(pointedAt());
		at.setDate(at.getDate() - 1);
		pointTo(at);
	}
	/** `-d`: that date, at the same clock. */
	function onDate(e: Event) {
		const [y, m, d] = (e.currentTarget as HTMLInputElement).value.split('-').map(Number);
		if (!y) return;
		const at = new Date(pointedAt());
		at.setFullYear(y, m - 1, d);
		pointTo(at);
	}
	/** A length typed: whatever a duration reads as, as the CLI reads it (the same parser as `dur=`). */
	async function typeDur() {
		const said = typedDur.trim();
		if (!said) return;
		const got = await run(lengthReadArgv(said));
		let minutes = 0;
		try {
			minutes = got.code === 0 ? Math.round((JSON.parse(got.stdout) as Reads['lengthReadArgv']).history_minutes ?? 0) : 0;
		} catch {
			// Not an answer: the box says so.
		}
		badDur = minutes <= 0;
		if (badDur) return;
		setMoment(false);
		const c = get(cursor);
		pointAt({ ...(c ? where(c) : { rel: 0 }), span: minutes });
		typedDur = '';
	}
	function toggleMoment() {
		if (!moment) clearDur();
		setMoment(!moment);
	}

	/** A tap on `time` or `dur`, or its pill: its options out, or in again. */
	const toggle = (part: 'time' | 'dur') => (open = open === part ? null : part);

	// ---- a drag closes the options --------------------------------------

	/** How far a drag toward the strip closes them. */
	const CLOSE_PX = 40;
	/**
	 * A press on the options, followed on the window until it lifts wherever
	 * it lifts: a drag that leaves them is still the drag, and a lift outside
	 * them still ends it, so no later move with no button down closes them.
	 */
	function closeDown(down: PointerEvent) {
		const move = (e: PointerEvent) => {
			if (e.pointerId !== down.pointerId) return;
			if ((down.clientX - e.clientX) * (side === 'right' ? -1 : 1) > CLOSE_PX) (open = null), end();
		};
		const end = () => (removeEventListener('pointermove', move), removeEventListener('pointerup', end), removeEventListener('pointercancel', end));
		addEventListener('pointermove', move);
		addEventListener('pointerup', end);
		addEventListener('pointercancel', end);
	}

	// ---- a pill DRAGGED (da-axrdv) --------------------------------------

	/** Px of drag for the first minute. */
	const JOG_PX = 6;
	/** How a click right after a drag is told from one: it is the drag's own. */
	const JOG_CLICK_MS = 300;
	let jog: { kind: 'time' | 'dur'; x: number; was: Cursor; moved: boolean } | null = null;
	let jogged = -Infinity;
	/**
	 * A press on `time` or `dur`, or its pill, followed on the window until it
	 * lifts, as the options' drag is: the word it began on is a pill after its
	 * first move, another element, so a capture on it ended with it and the
	 * lift went elsewhere, leaving a drag a later hover moved (canary on #1792).
	 */
	function jogDown(kind: 'time' | 'dur', down: PointerEvent) {
		if (kind === 'dur' ? lengthOff : disabled) return;
		// Nothing chosen yet: a time from now, a length from none.
		const was = get(pointed) ?? { at: new Date(), rel: 0 };
		jog = { kind, x: down.clientX, was, moved: false };
		const move = (e: PointerEvent) => e.pointerId === down.pointerId && jogMove(e);
		const end = (e: PointerEvent) => {
			if (e.pointerId !== down.pointerId) return;
			jogUp();
			removeEventListener('pointermove', move);
			removeEventListener('pointerup', end);
			removeEventListener('pointercancel', end);
		};
		addEventListener('pointermove', move);
		addEventListener('pointerup', end);
		addEventListener('pointercancel', end);
	}
	function jogMove(e: PointerEvent) {
		if (!jog) return;
		const dx = e.clientX - jog.x;
		if (!jog.moved && Math.abs(dx) < CLICK_SLOP) return;
		jog.moved = true;
		const by = Math.sign(dx) * Math.round((Math.abs(dx) / JOG_PX) ** 1.4);
		const c = jog.was;
		if (jog.kind === 'time') return pointAt({ at: new Date(+c.at + by * MINUTE), span: c.span, from: c.from });
		setMoment(false);
		pointAt({ ...where(c), span: Math.max(1, (c.span ?? 0) - by) });
	}
	function jogUp() {
		if (jog?.moved) jogged = performance.now();
		jog = null;
	}
	const tapped = (part: 'time' | 'dur') => performance.now() - jogged > JOG_CLICK_MS && toggle(part);

	/**
	 * Escape takes the options in, before anything under them: heard on the
	 * window's capture, ahead of every bubbling listener there, so a field's
	 * view (CollectWidget's Escape is Back) keeps the key for the next press.
	 */
	function onKey(e: KeyboardEvent) {
		if (e.key !== 'Escape' || e.defaultPrevented || !open) return;
		e.preventDefault();
		open = null;
	}
</script>

<svelte:window onkeydowncapture={onKey} />

<!-- Two triangles, apart: it drags sideways. -->
{#snippet drags()}<svg class="drags" data-drags viewBox="0 0 14 8" aria-hidden="true" focusable="false"><path d="M0 4 4 0.8v6.4zM14 4 10 0.8v6.4z" /></svg>{/snippet}

{#snippet clock()}
	<button class="clock" data-when-clock use:act={'show'} aria-expanded={expanded} aria-label={expanded ? 'hide the time and duration' : 'time and duration'} title={expanded ? 'hide the time and duration' : 'time and duration'} onclick={toggleShown}
		><svg viewBox="0 0 16 16" aria-hidden="true" focusable="false"><circle cx="8" cy="8" r="6.5" /><path d="M8 4.2V8l2.6 1.6" /></svg></button
	>
{/snippet}

{#snippet part(kind: 'time' | 'dur', label: string | null)}
	{#if label === null}
		<button
			class="word"
			use:act={'show'}
			class:out={open === kind}
			data-part={kind}
			data-pans="x"
			disabled={kind === 'dur' ? lengthOff : disabled}
			aria-expanded={open === kind}
			onpointerdown={(e) => jogDown(kind, e)}
			onclick={() => tapped(kind)}>{kind}{@render drags()}</button
		>
	{:else}
		<span class="pill" class:out={open === kind} data-pill={kind}>
			<button
				class="said"
				use:act={'show'}
				data-part={kind}
				data-pans="x"
				{disabled}
				aria-expanded={open === kind}
				onpointerdown={(e) => jogDown(kind, e)}
				onclick={() => tapped(kind)}
				>{#if kind === 'dur' && span}<LengthArrow />{/if}{label}{@render drags()}</button
			><button class="clear" {disabled} aria-label="clear {label}" onclick={kind === 'time' ? clearTime : clearDur}>×</button>
		</span>
	{/if}
{/snippet}

<div
	class="when"
	class:right={side === 'right'}
	class:closed={!expanded}
	class:halves={tall < COLUMN_PX && tall >= HALVES_PX}
	class:row={tall < HALVES_PX}
	bind:clientHeight={tall}
	style:left={side === 'left' ? `${offset}px` : null}
	style:right={side === 'right' ? `${offset}px` : null}
	bind:offsetWidth={width}
	data-when
>
	<div class="column">
		{#if expanded}
		<div class="half">
			{@render part('time', timeLabel)}
			<button class="step" use:act={'arm'} data-earlier="1" {disabled} title="a minute earlier" onclick={() => earlier(1)}>−1m</button>
			<button class="step" use:act={'arm'} data-earlier="5" {disabled} title="five minutes earlier" onclick={() => earlier(5)}>−5m</button>
		</div>
		{@render clock()}
		<div class="half">
			{@render part('dur', durLabel)}
			<button class="step" use:act={'arm'} data-longer="1" disabled={lengthOff} title="a minute longer" onclick={() => longer(1)}>1m</button>
			<button class="step" use:act={'arm'} data-longer="5" disabled={lengthOff} title="five minutes longer" onclick={() => longer(5)}>5m</button>
		</div>
		{:else}
			{@render clock()}
		{/if}
	</div>
	{#if expanded && open}
		<!-- svelte-ignore a11y_no_static_element_interactions -->
		<div class="more" data-when-more={open} onpointerdown={closeDown}>
			{#if open === 'time'}
				<form onsubmit={(e) => (e.preventDefault(), void typeTime())}>
					<label>-t <input data-type-time bind:value={typedTime} aria-invalid={badTime || undefined} oninput={() => (badTime = false)} placeholder="9:15am" {disabled} /></label>
				</form>
				<div class="pair">
					<button data-yesterday use:act={'arm'} {disabled} title="a day back, the same clock" onclick={dayBack}>-y</button>
					<button data-allday use:act={'arm'} class:on={allday} aria-pressed={allday} {disabled} title="the whole day, no clock" onclick={() => setAllday(!allday)}>-D</button>
				</div>
				<label>-d <input type="date" data-date {disabled} onchange={onDate} /></label>
			{:else}
				<form onsubmit={(e) => (e.preventDefault(), void typeDur())}>
					<label>dur <input data-type-dur bind:value={typedDur} aria-invalid={badDur || undefined} oninput={() => (badDur = false)} placeholder="1h30m" {disabled} /></label>
				</form>
				<div class="pair">
					<button data-moment use:act={'arm'} class:on={moment} aria-pressed={moment} {disabled} title="a moment, no length" onclick={toggleMoment}>-m</button>
				</div>
			{/if}
		</div>
	{/if}
</div>

<style>
	.when {
		position: absolute;
		top: 0;
		bottom: 0;
		z-index: 3;
		background: var(--ground);
		border-right: 1px solid var(--rule);
	}

	.when.right {
		border-right: none;
		border-left: 1px solid var(--rule);
	}

	/*
	 * Spaced so no two buttons meet (vividn, 11-06: «space out the buttons a
	 * little more in this time section. right now they overlap some»): a
	 * pill's × stands 0.7rem above its top right corner and 0.35rem past it,
	 * so a gap it can reach, above a pill or to its right, is wider than that.
	 */
	.column,
	.half {
		display: flex;
		flex-direction: column;
		align-items: center;
		justify-content: space-evenly;
		gap: 0.8rem;
	}

	.half {
		/* A pill's 3.6rem, and its × out of its corner. */
		width: 4.3rem;
		gap: 0.4rem;
	}

	.column {
		height: 100%;
		/* The × out of a pill's top corner, inside the column. */
		padding: 0.75rem 0.15rem 0.3rem;
		box-sizing: border-box;
		overflow: hidden auto;
		font-size: 0.75rem;
	}

	/* Closed, the clock alone, in the middle. */
	.when.closed .column {
		padding: 0 0.15rem;
		justify-content: center;
	}

	/* Too short for a column: the halves side by side, the clock between them; shorter still, a row. */
	:is(.halves, .row) .column {
		flex-direction: row;
		gap: 0.8rem;
		padding: 0.75rem 0.3rem 0.3rem;
	}

	.row .column {
		padding: 0 0.3rem;
	}

	.row .half {
		width: auto;
		flex-direction: row;
		gap: 0.8rem;
	}

	.clock {
		flex: none;
		display: grid;
		place-items: center;
		width: 24px;
		height: 24px;
		padding: 0;
		border: 1px solid var(--rule);
		border-radius: 999px;
		background: var(--ground);
		color: var(--muted);
	}

	.clock[aria-expanded='true'] {
		color: var(--ink-bright);
		border-color: var(--accent);
	}

	.clock svg {
		width: 14px;
		height: 14px;
		fill: none;
		stroke: currentColor;
		stroke-width: 1.5;
		stroke-linecap: round;
	}

	/* The two triangles: it drags sideways. */
	.drags {
		flex: none;
		width: 0.85em;
		height: 0.5em;
		margin-left: 0.25em;
		fill: currentColor;
		opacity: 0.7;
	}

	.row .pill {
		max-width: none;
	}

	/* A row is as narrow as its words: a phone's short pane has the rows to fit beside it. */
	.row :is(.step, .word) {
		min-width: 0;
		padding: 0.15rem 0.35rem;
	}

	.row .more {
		width: max-content;
		flex-direction: row;
		align-items: center;
	}

	button {
		font: inherit;
		cursor: pointer;
	}

	/* Every button a finger's 24px at least. */
	button {
		min-height: 24px;
	}

	.step,
	.word {
		border: 1px solid var(--rule);
		background: none;
		color: var(--ink);
		min-width: 2.6rem;
		padding: 0.12rem 0.3rem;
		border-radius: 999px;
	}

	.word {
		display: inline-flex;
		align-items: center;
		justify-content: center;
		font-style: italic;
		color: var(--muted);
		/* Held and dragged sideways, as its pill is. */
		touch-action: none;
	}

	.out {
		box-shadow: 0 0 0 2px var(--ink-bright);
	}

	/* The sum as a pill, its × in its corner. */
	.pill {
		position: relative;
		display: flex;
		/* Centered, with its × 0.35rem out of its right corner, inside the half. */
		max-width: calc(100% - 0.7rem);
		box-sizing: border-box;
		border: 1.5px solid var(--accent);
		border-radius: 0.6rem;
		font-family: ui-monospace, SFMono-Regular, Menlo, Consolas, monospace;
	}

	.pill button {
		display: inline-flex;
		align-items: center;
		justify-content: center;
		gap: 0.1em;
		background: none;
		border: none;
		color: var(--ink-bright);
		padding: 0.2rem 0.25rem;
	}

	.pill .said {
		flex: 1;
		min-width: 0;
		flex-wrap: wrap;
		text-align: center;
		/* Its own padding, and on the right what the × overlaps (24px less its 0.35rem out). */
		padding-right: calc(24px - 0.1rem);
		/* Held and dragged sideways. */
		touch-action: none;
	}

	.pill .clear {
		position: absolute;
		top: -0.7rem;
		right: -0.35rem;
		width: 24px;
		height: 24px;
		padding: 0;
		border-radius: 999px;
		background: var(--ground);
		border: 1px solid var(--accent);
		font-size: 0.7rem;
		line-height: 1;
	}

	/* The advanced options, out beside the column, over the rows. */
	.more {
		position: absolute;
		top: 0;
		bottom: 0;
		left: 100%;
		width: 11rem;
		display: flex;
		flex-direction: column;
		justify-content: center;
		gap: 0.4rem;
		padding: 0.4rem;
		box-sizing: border-box;
		overflow: hidden auto;
		background: var(--ground);
		border-right: 1px solid var(--rule);
		font-size: 0.8rem;
		/* A sideways drag closes it, so it is the page's to read. */
		touch-action: pan-y;
	}

	.right .more {
		left: auto;
		right: 100%;
		border-right: none;
		border-left: 1px solid var(--rule);
	}

	.more label {
		display: flex;
		align-items: center;
		gap: 0.3rem;
	}

	/* In the page's colors, as the rest of collect is (vividn, 11-06: «really jarringly off, lots of white»). */
	.more input,
	.more .pair button {
		background: var(--raised);
		color: var(--ink);
		border: 1px solid var(--rule);
		color-scheme: dark;
	}

	.more input {
		min-width: 0;
		flex: 1;
		font: inherit;
		border-radius: 0.3rem;
		padding: 0.15rem 0.3rem;
	}

	.more input::placeholder {
		color: var(--quiet);
	}

	.more input:focus {
		outline: 1px solid var(--accent);
		outline-offset: -1px;
	}

	.more input[aria-invalid] {
		outline: 1px solid var(--warn);
	}

	.more .pair {
		display: flex;
		gap: 0.4rem;
	}

	.more .pair button {
		border-radius: 999px;
		padding: 0.2rem 0.6rem;
	}

	.more .pair button.on {
		border-color: var(--accent);
		color: var(--ink-bright);
	}
</style>
