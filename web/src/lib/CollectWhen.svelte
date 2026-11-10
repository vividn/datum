<!--
	Collect's time and duration section (da-us1n1), open a column between the
	now strip and the rows, at rest and beside a field's view alike. vividn,
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
	every few pixels and faster the further it goes; `clear` in its options
	clears it, the pill wearing no × (da-c93wz, vividn, 2026-11-09: «remove
	the (x) button on the time and dur buttons, it is a bit ugly. Instead,
	make it easy to clear from within the expanded time and dur menus»).

	The advanced options are what the cursor alone cannot say quickly: a
	time typed as `-t` reads it (`9:15am`, `yesterday 10am`), resolved by the
	CLI's own parser with no database (`nowview --overlay --json`), `-y` a
	day back, `-d` a date at the same clock, and `-D` the whole day, a pill
	of its own beside the time, which names the day, taken by a command that
	lists `--full-day`; a length typed, read by the same CLI as `dur=` reads
	one (`--context`: `1.5h` is 90 minutes, `1h30` refused), and `-m`, a
	moment with no length, its own pill as the length's other face. A moment
	field open has no length to give, so the dur half is off beside it.

	At rest it is a clock alone, on the boundary between the strip and the
	rows (da-s2hht), and a tap on the clock opens it into its column, the
	clock then standing between its halves (vividn,
	2026-11-06: «hide it by default. But a little clock button in the middle
	between the fields section and the now section, clicking it toggles
	whether this time panel is open»); whether it is open is the session's,
	as collect's zoom is. `time` and `dur` wear a triangle outside each side
	(da-c93wz: «the triangle arrow symbols should appear outside the time and
	dur buttons»), and drag sideways with no pill yet too: `time` from now,
	`dur` from no length («add a little triangle arrow to the time and dur
	buttons so that you know you can drag them. Allow dragging them left and
	right, even when there is nothing yet selected»). `dur` dragged left is a
	length ending at the time, as `dur=5m`, and past none to the right one
	running forward from it, `+5m`, as `dur=+5m` (da-vvo03, vividn,
	2026-11-09: «then we can start allowing dragging dur to the right nicely»).

	A line runs on each side of it open, between it and the strip and between
	it and the rows, and closed one line between the strip and the rows, the
	clock on it (da-qjq60, vividn, 2026-11-07: «put another one separating the
	time section from the now section. When the time section is closed also
	put that line between the fields section and the now section, and put the
	clock button directly on that line»). It opens and closes over the rows'
	own glide, so its edge and the rows move together.
-->
<script module lang="ts">
	import { pills } from './bar.js';

	export const MOMENT_ID = 'moment';
	/**
	 * The rows' glide (CollectWidget's `.spot.shelf.glide`), which the section
	 * opens and closes over: one name, `--glide` in both components' CSS.
	 */
	export const GLIDE_MS = 300;
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
	import { prefersReducedMotion } from 'svelte/motion';
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
	/** Tapped once: from then it opens and closes over time, as it does not when it first draws. */
	let toggled = $state(false);
	function toggleShown() {
		toggled = true;
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

	/** Its two lines, one on each edge. */
	const LINES_PX = 2;
	/**
	 * The near line lies on the strip's own border, over its picture, which
	 * covers that border where it draws: one line, as wide as the far one.
	 */
	const ON_STRIP_PX = 1;
	/** The clock's size, as `.clock`'s. */
	const CLOCK_PX = 24;
	/** The column's width open, which the section's grows to. */
	let natural = $state(0);
	// The rows stand past where the section is going, gliding as it does; closed, past the clock's far half.
	$effect(() => {
		width = expanded ? natural + LINES_PX - ON_STRIP_PX : CLOCK_PX / 2;
	});
	/** The halves stay while the section closes over them. */
	const closing = (_: Element) => ({ duration: prefersReducedMotion.current ? 0 : GLIDE_MS });

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
	const ahead = $derived(!!span && !!$cursor?.ahead);
	const durLabel = $derived(moment ? '−m' : span ? `${ahead ? '+' : ''}${lengthArg(span)}` : null);

	/**
	 * What made a time once this section has moved it: a range keeps it, its
	 * spelling, and an instant is no longer the dayview's click, which alone
	 * the nowview centres on (da-nwf1h).
	 */
	const moved = (c: Cursor) => (c.span ? c.from : undefined);
	/** A selection's length, which way it runs included. */
	const lengthOf = (c: Cursor | null) => ({ span: c?.span, ahead: c?.ahead });
	/** Where a selection is, without its length. */
	const where = (c: Cursor) => (c.rel !== undefined ? { rel: c.rel, from: moved(c) } : { at: c.at, step: 1, from: moved(c) });

	/** `−1m`, `−5m`: that much earlier, summing. */
	function earlier(by: number) {
		const c = get(cursor);
		if (c && c.rel === undefined) return pointAt({ at: new Date(+c.at - by * MINUTE), step: 1, ...lengthOf(c), from: moved(c) });
		pointAt({ rel: (c?.rel ?? 0) + by, ...lengthOf(c), from: c?.from });
	}
	/** `1m`, `5m`: that much longer, summing, in place of `-m`. */
	function longer(by: number) {
		setMoment(false);
		const c = get(cursor);
		pointAt({ ...(c ? where(c) : { rel: 0 }), span: (c?.span ?? 0) + by, ahead: c?.ahead });
	}
	/** The time's `clear`: the length, if any, ends at now again. */
	function clearTime() {
		setAllday(false);
		const c = get(cursor);
		pointAt(c?.span ? { rel: 0, ...lengthOf(c) } : null);
	}
	/** The length's `clear`. */
	function clearDur() {
		setMoment(false);
		const c = get(cursor);
		if (c) pointAt(where(c));
	}

	/** Point at `at`, keeping the length. */
	const pointTo = (at: Date) => pointAt({ at, step: 1, ...lengthOf(get(cursor)) });
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
		// Typed, a length ends at the time, as an unsigned `dur=` does, whichever way a drag left it;
		// `+5m` runs forward from it, as `dur=+5m` does (da-klpoj). The reader drops the sign, so it is read here.
		pointAt({ ...(c ? where(c) : { rel: 0 }), span: minutes, ahead: said.startsWith('+') });
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
		if (jog.kind === 'time') return pointAt({ at: new Date(+c.at + by * MINUTE), ...lengthOf(c), from: moved(c) });
		setMoment(false);
		// One axis through none: left a length back from the time, right one forward from it.
		const axis = (c.ahead ? -1 : 1) * (c.span ?? 0) - by;
		pointAt({ ...where(c), span: Math.abs(axis), ahead: axis < 0 });
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

<!-- A triangle outside each side of the button, part of it to a press: it drags sideways. -->
{#snippet drags()}{#each ['left', 'right'] as at (at)}<svg class="drags {at}" data-drags={at} viewBox="0 0 4 8" aria-hidden="true" focusable="false"><path d={at === 'left' ? 'M0 4 4 0.8v6.4z' : 'M4 4 0 0.8v6.4z'} /></svg>{/each}{/snippet}

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
				>{#if kind === 'dur' && span}<LengthArrow {ahead} />{/if}{label}{@render drags()}</button
			></span
		>
	{/if}
{/snippet}

<div
	class="when"
	class:right={side === 'right'}
	class:closed={!expanded}
	class:halves={tall < COLUMN_PX && tall >= HALVES_PX}
	class:row={tall < HALVES_PX}
	bind:clientHeight={tall}
	style:left={side === 'left' ? `${offset - ON_STRIP_PX}px` : null}
	style:right={side === 'right' ? `${offset - ON_STRIP_PX}px` : null}
	style:width="{expanded ? natural + LINES_PX : LINES_PX}px"
	class:glides={toggled}
	style:--glide="{GLIDE_MS}ms"
	data-when
>
	{@render clock()}
	<!-- At its open width, centered and cut to the section's as it opens and closes; the clock stands in its gap. -->
	<div class="reveal">
		<div class="column" bind:offsetWidth={natural}>
			{#if expanded}
				<div class="half" out:closing>
					{@render part('time', timeLabel)}
					<button class="step" use:act={'arm'} data-earlier="1" {disabled} title="a minute earlier" onclick={() => earlier(1)}>−1m</button>
					<button class="step" use:act={'arm'} data-earlier="5" {disabled} title="five minutes earlier" onclick={() => earlier(5)}>−5m</button>
				</div>
				<i class="gap"></i>
				<div class="half" out:closing>
					{@render part('dur', durLabel)}
					<button class="step" use:act={'arm'} data-longer="1" disabled={lengthOff} title="a minute longer" onclick={() => longer(1)}>1m</button>
					<button class="step" use:act={'arm'} data-longer="5" disabled={lengthOff} title="five minutes longer" onclick={() => longer(5)}>5m</button>
				</div>
			{/if}
		</div>
	</div>
	{#if expanded && open}
		<!-- svelte-ignore a11y_no_static_element_interactions -->
		<div class="more" data-when-more={open} onpointerdown={closeDown}>
			{#if open === 'time'}
				<form onsubmit={(e) => (e.preventDefault(), void typeTime())}>
					<label>-t <input data-type-time bind:value={typedTime} aria-invalid={badTime || undefined} oninput={() => (badTime = false)} placeholder="9:15am" {disabled} /></label>
				</form>
				<!-- `clear` first in its row, where a phone's edge does not cut it. -->
				<div class="pair">
					<button data-clear use:act={'arm'} disabled={disabled || !timeLabel} title="back to now" onclick={() => (clearTime(), (open = null))}>clear</button>
					<button data-yesterday use:act={'arm'} {disabled} title="a day back, the same clock" onclick={dayBack}>-y</button>
					<button data-allday use:act={'arm'} class:on={allday} aria-pressed={allday} {disabled} title="the whole day, no clock" onclick={() => setAllday(!allday)}>-D</button>
				</div>
				<label>-d <input type="date" data-date {disabled} onchange={onDate} /></label>
			{:else}
				<form onsubmit={(e) => (e.preventDefault(), void typeDur())}>
					<label>dur <input data-type-dur bind:value={typedDur} aria-invalid={badDur || undefined} oninput={() => (badDur = false)} placeholder="1h30m" {disabled} /></label>
				</form>
				<div class="pair">
					<button data-clear use:act={'arm'} disabled={disabled || !durLabel} title="no length" onclick={() => (clearDur(), (open = null))}>clear</button>
					<button data-moment use:act={'arm'} class:on={moment} aria-pressed={moment} {disabled} title="a moment, no length" onclick={toggleMoment}>-m</button>
				</div>
			{/if}
		</div>
	{/if}
</div>

<style>
	/*
	 * A line on each edge (da-qjq60): open, between it and the strip and
	 * between it and the rows; closed, it is two lines' width on the strip's
	 * edge, the far line gone, and the clock stands on the one left.
	 */
	.when {
		position: absolute;
		top: 0;
		bottom: 0;
		z-index: 3;
		background: var(--ground);
		border: 1px solid var(--rule);
		border-block: none;
	}

	.when.closed {
		border-right-color: transparent;
	}

	.when.right.closed {
		border-right-color: var(--rule);
		border-left-color: transparent;
	}

	.when.glides {
		transition:
			width var(--glide) ease-out,
			border-color var(--glide) ease-out;
	}

	/* The open column, at its own width and cut to the section's. */
	.reveal {
		position: absolute;
		inset: 0;
		display: flex;
		justify-content: center;
		overflow: hidden;
	}

	/*
	 * Spaced so no two buttons meet (vividn, 11-06: «space out the buttons a
	 * little more in this time section. right now they overlap some»), a
	 * triangle out of each side of `time` and `dur` among them. The halves
	 * are as tall (as wide, side by side) as each other, so the clock's gap
	 * is in the middle.
	 */
	.column {
		flex: none;
		display: grid;
		grid-template-rows: 1fr auto 1fr;
		place-items: center;
		gap: 0.8rem;
		width: max-content;
		height: 100%;
		padding: 0.3rem 0.15rem;
		font-size: 0.75rem;
	}

	.half {
		display: flex;
		flex-direction: column;
		align-items: center;
		justify-content: space-evenly;
		/* A pill's 3.6rem, and a triangle out of each side. */
		width: 4.3rem;
		gap: 0.4rem;
	}

	.gap {
		width: 24px;
		height: 24px;
	}

	/* Too short for a column: the halves side by side, the clock between them; shorter still, a row. */
	:is(.halves, .row) .column {
		grid-template-rows: none;
		grid-template-columns: 1fr auto 1fr;
		grid-auto-flow: column;
		padding: 0.3rem;
	}

	/* Room for the first button's outer triangle. */
	.row .column {
		padding: 0 0.8rem;
	}

	.row .half {
		width: auto;
		flex-direction: row;
		gap: 0.8rem;
	}

	/*
	 * In the middle of the section, so closed it is ON the strip's edge
	 * (vividn, 11-06: «put it right on the boundary line between the now
	 * section and the fields section»), the rows standing clear of its far
	 * half, and open in the gap between the halves.
	 */
	.clock {
		position: absolute;
		z-index: 1;
		left: 50%;
		top: 50%;
		translate: -50% -50%;
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

	@media (prefers-reduced-motion: reduce) {
		.when.glides {
			transition: none;
		}
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

	/* A triangle out of each side: it drags sideways. */
	.drags {
		position: absolute;
		top: 50%;
		translate: 0 -50%;
		width: 0.3em;
		height: 0.5em;
		fill: var(--muted);
		opacity: 0.7;
	}

	/* Out of the button's border: its padding box, which they stand from, is inside it. */
	.drags.left {
		right: calc(100% + var(--drag-edge) + 0.2em);
	}

	.drags.right {
		left: calc(100% + var(--drag-edge) + 0.2em);
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
		position: relative;
		--drag-edge: 1px;
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

	/* The sum as a pill, its triangles out of its sides. */
	.pill {
		position: relative;
		display: flex;
		/* Centered, with a triangle out of each side, inside the half. */
		max-width: calc(100% - 1rem);
		box-sizing: border-box;
		--drag-edge: 1.5px;
		border: var(--drag-edge) solid var(--accent);
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
		/* Held and dragged sideways. */
		touch-action: none;
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

	/* Off, with nothing to clear or the widget disabled: the page's faintest ink. */
	.more .pair button:disabled {
		color: var(--quiet);
		cursor: default;
	}

	.more .pair button.on {
		border-color: var(--accent);
		color: var(--ink-bright);
	}
</style>
