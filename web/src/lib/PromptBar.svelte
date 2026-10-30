<!--
	THE SEAM (da-ckw3, reshaping da-xfa4's bar).

	The bar is the seam between the dashboard and the history, and it reads
	as a terminal does — the history above, the newest answer under it, the
	line being typed at the very bottom:

	    dashboard (the layout)
	    ────────────────────────────────────────────────────  ^
	    older entry …                     ← the log, opened by pulling up
	    older entry
	    OUTPUT   the newest result  ← this IS the log's last entry
	    INPUT    datum  16:09:21  ❯ occur coffee▏ [pills]

	TWO SECTIONS, OUTPUT OVER INPUT. vividn, 2026-10-27: «keep the input bar
	on the bottom, and put the output/return bar above it again … Mimicking
	an actual cli with the input bar on the bottom». (It was input over
	output from 2026-09-17, da-ckw3, until then.) The input is 2.9rem; the
	output took the collect tab's command pane's room (da-mwp4m,
	defaults.ts), and is one strip entry tall (da-7xp82, routes/+layout.svelte).

	- the INPUT section is ONE ROW — the mark (Brand.svelte: the database's
	  name, its colour, its menu), the clock WITH SECONDS, the `❯` and the
	  box with its pills (Prompt.svelte), and nothing else. The clock does
	  not hide its seconds on a narrow screen: vividn, "On mobile too please
	  show seconds on the clock". The mark ellipsizes rather than taking the
	  box's room.
	- the OUTPUT section is THE HISTORY'S LAST ROWS, side by side in one
	  strip the full width of the bar that scrolls sideways (da-qua5g): the
	  newest at the right end, a command a group, drawn with the same
	  `CliEntry.svelte` the log above draws its rows with, so the newest
	  command looks the same whether the log is open or shut (vividn,
	  2026-09-21).

	ON A PAGE THAT IS THE CLI WIDGET ALONE (`/cli`, `/repl`) there is no
	output section: vividn, 2026-10-27, «the output bar is then completely
	gone on this full screen widget, since it basically becomes the output
	section» (`outputless`, #lib/log.ts).

	Both are `--input-height` / `--output-height`, declared together on the
	shell (routes/+layout.svelte) because the dashboard's own height is the
	other side of that sum. vividn: "we may have to play around with the
	heights some" — they are two numbers in one place.

	THE OUTPUT FOLDS AWAY, and the GESTURE is what folds it. Every screen
	starts open, and after that it is whatever this device last chose
	(#lib/log.ts). Each state's height is fixed, so the
	dashboard reflows when a PERSON moves the bar and never because a command
	answered something long.

	THREE STATES (da-0b2vy): CLOSED, the SINGLE ROW, and FULLSCREEN, where
	the history fills the page one command a line and the strip gives way to
	it (#lib/log.ts). A drag anywhere on the bar goes a state the way it went
	(#lib/detent.ts). There is no handle (da-pnxug), vividn, 2026-10-28: «it
	actually makes it difficult to grab the nowview handle … the swiping up
	and down on the output section/input section is sufficient here».
	Except CLOSED, where there is no output to swipe (da-d4tqt), vividn,
	2026-10-29: «when the output section is completely hidden, show a little
	^------^ handle in middle above the input bar that reopens it». Then
	(da-k5kwu), 2026-10-30: «let's put it on the far right, and have it
	protruding into the swipe space. make it wide and tall enough for the
	ACTION verb to appear in there».

	FOLDED, A COMMAND STILL ANSWERS, in the tab: six characters (#lib/tabWord.ts),
	each action in the color a document draws it in, fading over `ACTION_MS`
	(#lib/bar.ts). A failed command says a red `ERROR!`, for `ERROR_MS`, and a
	tap on it unfolds the output onto the error text. Nothing unfolds by itself.

	THE LOG IS REACHED THROUGH THE BAR. How far open the bar is, is ONE
	number of pixels (#lib/log.ts has the arithmetic and the notch); the
	page itself never scrolls. The two gestures that move that number — a
	drag and a wheel — are `use:grip` (#lib/grip.ts), on this bar; the
	log's rows scroll as a terminal's do.

	The keyboard has the same three states (da-pnxug): on the box, PageUp
	goes closed → row → full and PageDown back, and Escape comes back too.

	THE COMMANDS OVER THE OUTPUT (da-xw655): the `❯` is a button
	(Prompt.svelte), and pressed it slides the command widget (CommandPane)
	up over the output section, at the output's own height, over whatever
	the bar shows there — the row, nothing, or the foot of the full log. It
	slides away on the `❯` again, on Escape, on a run, which the output then
	shows (`pressWithPills`), and on a choice that fills the line.

	Everything the bar already did it still does: the pills, the ghost and
	`spent` are the PROMPT's (Prompt.svelte, one buffer shared with every
	input widget); `prefill` is taken here because the bar is the one view
	always on screen, and `focusLine` decides which box the caret lands in;
	while storage is degraded the border is the panes' alarm orange and the
	title is the storage sentence.
-->
<script lang="ts">
	import { tick, untrack } from 'svelte';
	import { fly } from 'svelte/transition';
	import Brand from '#lib/Brand.svelte';
	import CliEntry from '#lib/CliEntry.svelte';
	import { clearScrollback, cli, stepped, type Command } from '#lib/cliStore.ts';
	import { parseEcho } from '#lib/echo.ts';
	import { ACTION_MS, ERROR_MS, commandsOpen, focusLine, marks, peel, pills, prefill, text } from '#lib/bar.ts';
	import CommandPane from '#lib/layout/CommandPane.svelte';
	import { epoch } from '#lib/layout/engine.ts';
	import { clockSecondsLabel, now } from '#lib/clock.ts';
	import { closeLog, inLog, openOutput, output, outputless, raise, shown } from '#lib/log.ts';
	import { layout } from '#lib/layout/current.ts';
	import { isWidget } from '#lib/layout/types.ts';
	import { grip } from '#lib/grip.ts';
	import { tabWord } from '#lib/tabWord.ts';
	import Prompt from '#lib/Prompt.svelte';
	import { isDegraded, storage, storageSentence } from '#lib/storageState.ts';

	let field: HTMLElement;

	$effect.pre(() => {
		const asked = $prefill;
		if (asked !== null) untrack(() => void take(asked));
	});

	/**
	 * The one-shot `prefill` signal is taken HERE because the bar is the one
	 * view always on screen — but where the caret then goes is `focusLine`'s
	 * choice, which is an input widget when the reader has put one on the
	 * page (da-3zon).
	 */
	async function take({ line, keyboard }: { line: string; keyboard: boolean }): Promise<void> {
		prefill.set(null);
		text.set(line);
		await tick();
		focusLine(keyboard);
	}

	/**
	 * A root tab that is the CLI widget ALONE draws no output section: the
	 * widget is the output there (vividn's answer, 2026-10-27). A CLI beside
	 * other panes is not the whole page, and the bar keeps its output.
	 */
	$effect.pre(() => {
		const tab = $layout.root.tabs[$layout.root.active];
		outputless.set(!!tab && isWidget(tab.content) && tab.content.kind === 'cli');
	});

	/**
	 * THE STRIP (da-qua5g): the history's last `STRIP` commands, oldest at the
	 * left and the newest at the right end, in one row that scrolls sideways.
	 * vividn, 2026-10-27: «when it's just the single row, new commands can
	 * come in and push previous entries to the left forming a little
	 * scrollback». Little: the whole history is the log's.
	 *
	 * MOUNTED NEAR THE VIEW, not all thirty: the bar is on every page, and
	 * thirty groups mounted at every load cost a phone at 4× about 230 ms
	 * of first paint and a long task or two on every command landing
	 * (vremya on #1269). So it mounts the newest `MOUNT`, and `GROW` more
	 * whenever the reader comes within a row's width of the oldest mounted.
	 */
	const STRIP = 30;
	const MOUNT = 4;
	const GROW = 6;
	/** How long an arrival's slide plays, in ms. */
	const SLIDE_MS = 260;
	/**
	 * The oldest command mounted, or null for the newest `MOUNT`. An entry
	 * and not a count, so a command landing while the reader is scrolled
	 * back mounts one more rather than unmounting the one being read.
	 * `raw`: a `$state` proxies the object it is given, and the proxy is
	 * found in no list of the entries themselves.
	 */
	let from: Command | null = $state.raw(null);
	/** What the strip can show: a button's undo or redo is a mark on the command it stepped (da-25q3k). */
	const commands = $derived($cli.scrollback.filter((entry) => !stepped(entry)));
	const strip = $derived.by(() => {
		const all = commands;
		const at = from ? all.indexOf(from) : -1;
		return all.slice(at < 0 ? -MOUNT : Math.max(at, all.length - STRIP));
	});
	/** The newest command the strip has drawn, to tell an arrival from a growth. Plain, as `pinned` is. */
	let newest: Command | undefined;

	let roll: HTMLDivElement | undefined = $state();
	let track: HTMLDivElement | undefined = $state();
	/**
	 * Whether the reader is at the newest end — or has never scrolled away
	 * from it. Plain, not `$state`, on purpose: it is read only inside the
	 * effect below and written by the scroll handler, and a write must not
	 * run that effect again.
	 */
	let pinned = true;

	const atEnd = (box: HTMLElement): boolean => box.scrollLeft + box.clientWidth >= box.scrollWidth - 2;

	/**
	 * Where the reader is, read on every scroll — except during a slide:
	 * the track's transform widens what the row can scroll over, so the
	 * jump to the end reads as short of it. Read again as the slide ends.
	 */
	function reread(): void {
		if (roll && !track?.getAnimations().length) pinned = atEnd(roll);
	}

	/**
	 * Mount `GROW` more when less than a row's width of groups is left before
	 * the view. Never during a slide: what it mounts runs the effect again,
	 * and that would take the slide off.
	 */
	function fill(): void {
		if (!roll || track?.getAnimations().length || roll.scrollLeft >= roll.clientWidth) return;
		const all = commands;
		const at = all.indexOf(strip[0]);
		if (at > 0 && all.length - at < STRIP) from = all[Math.max(0, at - GROW, all.length - STRIP)];
	}

	/**
	 * At the end, the strip settles: and once more than three rows of groups
	 * stand before the view, the oldest goes, one at a time, off screen. The
	 * gap between this and `fill` keeps the two from undoing each other.
	 */
	function rest(): void {
		reread();
		if (roll && pinned && from && strip.length > MOUNT && roll.scrollLeft > 3 * roll.clientWidth) from = strip[1];
		fill();
	}

	/**
	 * A command lands: the row moves to it, and the rest SLIDE LEFT to make
	 * room, so it is plain what moved. Not by a smooth scroll — its own
	 * scroll events would read as the reader scrolling away mid-glide — but
	 * by jumping to the end and playing the jump back as a slide of the
	 * whole track (FLIP), measured on the group that was newest before.
	 * Scrolled back, the reader stays where they were, even as the oldest
	 * group leaves the front of the strip.
	 */
	$effect.pre(() => {
		const list = strip;
		const arrived = list.at(-1) !== newest;
		newest = list.at(-1);
		const box = roll;
		const anchor = track?.lastElementChild;
		if (!box || !anchor) return;
		// A slide still playing is taken off when the next command lands
		// (canceled, so it re-reads nothing): the place measured is where the
		// row rests, not where an earlier arrival's transform has it.
		if (arrived) for (const slide of track?.getAnimations() ?? []) slide.cancel();
		const before = anchor.getBoundingClientRect().left;
		untrack(() =>
			tick().then(() => {
				if (!anchor.isConnected) return;
				if (!pinned) {
					box.scrollLeft += anchor.getBoundingClientRect().left - before;
					return fill();
				}
				box.scrollLeft = box.scrollWidth;
				const moved = before - anchor.getBoundingClientRect().left;
				if (track && arrived && moved > 0 && !matchMedia('(prefers-reduced-motion: reduce)').matches)
					track
						.animate([{ transform: `translateX(${moved}px)` }, { transform: 'none' }], {
							duration: SLIDE_MS,
							easing: 'ease-out'
						})
						.finished.then(rest, () => {});
				else rest();
			})
		);
	});

	/** Opened, the row starts at its newest end. */
	$effect(() => {
		if (roll) roll.scrollLeft = roll.scrollWidth;
	});

	const degraded = $derived(isDegraded($storage));

	/**
	 * WHO GETS AN ESCAPE, first to last (da-al892). One key, one meaning:
	 * the first of these that has something to close takes it, and marks
	 * it taken with `preventDefault`.
	 *   1. Help, armed — `help.ts` stops the key in the capture phase, so
	 *      nothing below ever hears it;
	 *   2. an open `<dialog>` (Confirm, Ask, Chooser, a
	 *      Sheet) — the browser's own cancel, which marks nothing, so it is
	 *      asked for by looking;
	 *   3. an open menu (PaneMenu), a custom pane's
	 *      editor, a field's state list or its `+` (FieldsWidget), a
	 *      command's subcommands (CommandsWidget);
	 *   4. the log, here;
	 *   5. layout mode asks how to leave it (RootBar.svelte, da-vvcxc);
	 *   6. LAST, the input peels one layer (`peel` in #lib/bar.ts).
	 * The window's listeners run in mount order, not in this one, so the
	 * last two wait for the dispatch to finish and read what it left —
	 * and the peel takes only the line the key was pressed on, never one
	 * written since.
	 */
	/** No slide for a reader who asked for no motion. */
	const still = () => matchMedia('(prefers-reduced-motion: reduce)').matches;

	function onKey(event: KeyboardEvent): void {
		// PageUp and PageDown on the box move the bar a state, as a swipe does.
		if ((event.key === 'PageUp' || event.key === 'PageDown') && !$outputless && field.contains(event.target as Node)) {
			event.preventDefault();
			if (event.key === 'PageUp') raise();
			else closeLog();
			return;
		}
		if (event.key !== 'Escape' || event.defaultPrevented) return;
		// The commands are on top, so they go first — after the dispatch,
		// since their own subcommands and flags (CommandsWidget) take the
		// key before them and mounted after this listener.
		if ($commandsOpen) {
			if (!document.querySelector('dialog[open]'))
				setTimeout(() => event.defaultPrevented || commandsOpen.set(false));
			return;
		}
		if ($inLog) {
			closeLog();
			event.preventDefault();
			return;
		}
		if (document.querySelector('dialog[open]')) return;
		const was = [$pills, $text];
		setTimeout(() => {
			if (!event.defaultPrevented && was[0] === $pills && was[1] === $text) peel();
		});
	}

	/**
	 * THE × PAST THE NEWEST (da-3eczg). vividn, 2026-10-28: «dragging the
	 * collapsed view further to the left should eventually show an x icon
	 * with clear written under it which clears the scrollback». A finger
	 * going on left once the strip stands at its newest end pulls the strip
	 * off the × behind it, ever less as it goes (the nowview's stop at now,
	 * #1288), and short of the threshold, let go, it springs back and
	 * nothing happens. No dialog: the gesture is the act. Focused, the × is
	 * shown, so a keyboard reaches it.
	 *
	 * A PULL TO REFRESH (da-uqx7n). vividn, 2026-10-29: «it is much too
	 * easy to clear currently. It should be similar to the browser pull down
	 * to refresh action, where an X in a circle slowly comes out until a
	 * certain threshold after which the circle becomes bright and then it
	 * clears on let go». So only a touch that STARTS with the strip at rest
	 * at its newest end arms; a swipe that only arrives there stretches it
	 * and arms nothing. Armed, the circle grows and fades in with the
	 * finger's travel, turns bright at `PULL_PX`, dims again back under it,
	 * and a lift clears only while it is bright.
	 *
	 * Read from TOUCH events: the strip scrolls natively, and the browser
	 * cancels the pointer it pans with but goes on telling the page where
	 * the finger is.
	 *
	 * SIDEWAYS ONLY (da-6lqa6): a touch whose first `SLOP` px go more up or
	 * down than across is the bar's swipe, and pulls nothing for the rest of
	 * it. It read every touch, so a swipe up drifting left dragged the strip
	 * along and grew the ×.
	 */
	const CLEAR_PX = 56;
	/** The pull's limit: the strip comes off the × ever less, toward this. */
	const STRETCH_PX = 2 * CLEAR_PX;
	/** How far an armed finger travels past the end before a lift clears: more than one ordinary swipe. */
	const PULL_PX = 200;
	/** How far a finger goes before its axis is known: the bar's own `DRAG_START` (#lib/grip.ts). */
	const SLOP = 8;
	/** Where the touch began, and whether it went up or down first: then it is no pull. */
	let began: { x: number; y: number } | null = null;
	let upright = false;
	/** Where the finger was as the strip stood at its end, or null; and how far left of it it is. */
	let endAt: number | null = null;
	let over = $state(0);
	/** The touch began with one finger and the strip at rest at its end, and has not left it. */
	let armed = $state(false);
	let focused = $state(false);
	const bright = $derived(armed && over >= PULL_PX);
	/** How far the circle has come out, 0 to 1. */
	const grown = $derived(focused ? 1 : still() ? +bright : armed ? Math.min(over / PULL_PX, 1) : 0);
	const pull = $derived(
		focused ? CLEAR_PX : still() ? (bright ? CLEAR_PX : 0) : over / (1 + over / STRETCH_PX)
	);

	function onTouchStart(event: TouchEvent): void {
		armed = event.touches.length === 1 && !!roll && atEnd(roll);
		began = { x: event.touches[0].clientX, y: event.touches[0].clientY };
		upright = false;
		onTouchMove(event);
	}

	/** A second finger calls the pull off: what it pulled goes back, and a lift clears nothing. */
	function onTouchMove(event: TouchEvent): void {
		if (!roll) return;
		const touch = event.touches[0];
		if (began && touch && Math.hypot(touch.clientX - began.x, touch.clientY - began.y) >= SLOP) {
			upright = Math.abs(touch.clientY - began.y) > Math.abs(touch.clientX - began.x);
			began = null;
		}
		const x = event.touches.length === 1 && !upright ? touch.clientX : null;
		if (x === null || endAt === null || x > endAt) endAt = x !== null && atEnd(roll) ? x : null;
		if (endAt === null) armed = false;
		over = x === null || endAt === null ? 0 : endAt - x;
	}

	function onTouchEnd(event: TouchEvent): void {
		if (event.type === 'touchend' && !event.touches.length && bright) clearScrollback();
		armed = false;
		began = null;
		endAt = null;
		over = 0;
	}

	/** What the tab says. */
	const said = $derived($marks && tabWord($marks.words, $marks.failed));
</script>

<svelte:window onkeydown={onKey} />

<!-- `grip` is the DRAG, and a drag has no keyboard equivalent to offer:
     everything it reaches — the box, the mark, `close history`, PageUp,
     PageDown, Escape — is a control or a key of its own. The same action is on the log
     (#lib/BarLog.svelte): one handle, one gesture, one number. -->
<footer
	class="prompt-bar"
	class:alarm={degraded}
		title={degraded ? $storageSentence : undefined}
	use:grip
>
	{#if $shown > 0}
		<!-- It OPENS: its height is the first slice of how far the bar is
		     pulled (#lib/log.ts), so a drag grows it smoothly and the notch
		     is where it has finished. Clipped while it is part-way, from the
		     top: what shows first is the foot of the row, next to the input. -->
		<div class="section output">
			<!-- The × and the strip, with ONE right edge: the section's padding
			     is outside it, so nothing of the × shows past the strip at rest. -->
			<div class="pull">
				<!-- Behind the strip's right end, until a pull past the newest or a
				     focus slides the strip off it (da-3eczg). The commands on
				     screen go, never an entry. -->
				{#if $cli.scrollback.length}
					<button
						type="button"
						class="clear-strip"
						aria-label="clear the scrollback"
						class:bright
						style:width="{CLEAR_PX}px"
						style:right="{Math.min(0, pull - CLEAR_PX)}px"
						style:opacity={grown}
						style:--grown={grown}
						title="clear the scrollback: the commands on screen, not your data"
						onfocus={() => (focused = true)}
						onblur={() => (focused = false)}
						onclick={() => ((focused = false), clearScrollback(), focusLine(true))}
						><span aria-hidden="true">×</span>clear</button
					>
				{/if}
				<!-- The touch handlers only watch a scroll for its overscroll; the ×
				     itself is the keyboard's way. -->
				<!-- svelte-ignore a11y_no_static_element_interactions -->
				<div
					class="roll"
					class:pulling={over > 0}
					style:transform={pull ? `translateX(${-pull}px)` : undefined}
					bind:this={roll}
					onscroll={() => (reread(), fill())}
					ontouchstart={onTouchStart}
					ontouchmove={onTouchMove}
					ontouchend={onTouchEnd}
					ontouchcancel={onTouchEnd}
				>
					<div class="track" bind:this={track}>
						<!-- ROWS OF THE HISTORY, not a second rendering of them.
						     vividn, 2026-09-21: "the output section … should actually
						     *be* the first line of the repl. … It should also have the
						     same format as the rest of the repl." So each group mounts
						     the same `CliEntry` the log does — stamp, echo line and
						     all — and the log ends at the row before the newest
						     (`skip={1}` in BarLog.svelte).

						     ONE GROUP A COMMAND, its documents side by side inside it
						     and a rule between it and the next, under the command's
						     own head: vividn, «make sure that it is clear where the
						     boundaries between different commands are». A group is as
						     wide as the documents it echoed (da-7xp82), never wider
						     than the row, where `+N` holds the rest; a
						     command whose answer is printed text — a table, a
						     picture, an error — takes the row's whole width, as it
						     takes a whole row in the history (scrollback.ts). `cap`
						     is more lines than the section holds: the raw output
						     stands on the group's foot and the lines that do not fit
						     are cut from the top, so a `tail` shows its newest rows. -->
						{#each strip as entry (entry)}
							{@const docs = parseEcho(entry.result.stdout)?.length ?? 0}
							<div class="group" class:wide={!docs} data-ran={entry.ranAt}>
								<CliEntry {entry} cap={12} />
							</div>
						{/each}
					</div>
				</div>
			</div>
		</div>
	{/if}

	<div class="section input">
		<!-- The mark and the clock, as ONE item of the row. On a wide screen it
		     is not a box at all (`display: contents`) and they stand side by
		     side; on a phone it is a column, the mark over the clock, so the
		     line has their shared width back (da-ovdtt). -->
		<div class="head">
			<!-- A size container: the database's name, whatever its length, does
			     not widen this block and so never takes room from the box. -->
			<div class="brand-block">
				<Brand />
			</div>
			<!-- Live, not restored: a command's stamp is history, this one is the
			     clock. Seconds at every width (vividn, on the phone). The whole
			     label is ONE flex item: a `:SS` that were an item of its own
			     would be a block box beside a block box, and `innerText` — and a
			     reader copying the clock — would find a line break in the middle
			     of a time. -->
			<span class="stamp"
				><span class="clock"
					>{clockSecondsLabel($now).slice(0, 5)}<span class="seconds"
						>{clockSecondsLabel($now).slice(5)}</span
					></span
				></span
			>
		</div>
		<!-- The ACTION strip lives INSIDE this block, so it begins exactly at
		     the `❯` — vividn, 2026-09-18: "put them still in bottom part like
		     this, but over by the >. Over on the right side they are hidden."
		     Absolute positioning cannot find the `❯` from the section, because
		     where the row's items fall depends on the mark's and the clock's
		     widths; inside the block that starts there, `left: 0` IS there. -->
		<div class="prompt" bind:this={field}>
			<Prompt />
		</div>
	</div>

	{#if $commandsOpen}
		<!-- Clipped at the input's top, so it slides out from behind it. Its
		     buttons stand in columns reached up and down, and a swipe or a
		     wheel on them is theirs: the bar's grip leaves it alone (da-m9zo7). -->
		<div class="commands-over" data-own-gestures>
			<div
				id="bar-commands"
				class="commands"
				transition:fly={{ y: '100%', opacity: 1, duration: still() ? 0 : 160 }}
			>
				<CommandPane epoch={$epoch} key="bar:commands" axis="y" />
			</div>
		</div>
	{/if}
	<!-- THE TAB, CLOSED ONLY (da-d4tqt, da-k5kwu): a press opens the output,
	     a swipe on it is the bar's own (`use:grip`), and what the last command
	     said stands in it. -->
	{#if !$outputless && $shown === 0 && !$inLog}
		<button
			type="button"
			class="reopen"
			class:failed={$marks?.failed}
			aria-label="show the output"
			title="show the output"
			onclick={$marks?.failed ? openOutput : raise}
		>
			<!-- Keyed on the object, so a second command restarts the fade. -->
			{#key $marks}
				{#if !said}
					<svg viewBox="0 0 40 6" aria-hidden="true"><path d="M1 1 4 5h32l3-4" /></svg>
				{:else}
					<span class="said" style="--fade: {$marks?.failed ? ERROR_MS : ACTION_MS}ms">
						{#each said.parts as part, i (i)}{#if i},{/if}<span
								class={part.text === 'ERROR!' ? 'chip' : 'action'}
								style:color={part.color}>{part.text}</span
							>{/each}{#if said.more}+{/if}
					</span>
				{/if}
			{/key}
		</button>
	{/if}
</footer>

<style>
	.prompt-bar {
		position: relative;
		display: flex;
		flex-direction: column;
		flex: none;
		/* The two sections, summed on the shell — which is also where the
		   dashboard's own height is taken from that sum
		   (routes/+layout.svelte). */
		height: var(--bar-height);
		min-width: 0;
		background: var(--ground);
		border-top: var(--bar-rule) solid var(--rule);
		/* The bar is a vertical handle: the browser may pan it sideways (a
		   long line in the box) but never up and down, which is ours. */
		touch-action: pan-x;
	}

	/* AND AGAIN ON THE BOX, because the box is a scroll container now.
	   `touch-action` is resolved from the touch's target up to the NEAREST
	   SCROLL CONTAINER and no further — so once `.field` became one (the
	   pills ride its run, Prompt.svelte), the bar's own `pan-x` stopped
	   being consulted for a touch that lands on the box, `.field`'s `auto`
	   let the browser claim a vertical pan, and the drag died on the
	   `pointercancel` that followed the first move. vremya measured it on
	   PR #46 with a real finger: seven drags from the box, `pointermove 1,
	   pointercancel 1`, the bar back at 0 every time, while the same drag
	   from the clock or the mark opened the log.

	   `pan-x` is what this box wants anyway — sideways is exactly the scroll
	   the pills are reached by, and up and down is the bar's. */
	.section.input :global(.field) {
		touch-action: pan-x;
	}

	/* Orange rather than red: "your data is not safe yet" is a warning
	   about the future, not a report of a failure. Visible from across a
	   desk (PaneFrame.svelte's alarm). */
	.prompt-bar.alarm {
		border-top-color: var(--alarm);
		box-shadow: 0 -1px 0 var(--alarm) inset;
	}

	/* vividn, 2026-09-18: "decrease margin between db/logo and clock, they are
	   different colors so they can be almost touching". So the row's gap is
	   the SMALL one and the things that do want air ask for it themselves
	   (the box, below) — rather than one generous gap paid for four times. */
	.section {
		display: flex;
		align-items: center;
		/* Between the row's items. It went to almost nothing — vividn, 2026-09-18:
		   "they can be almost touching" — and came back on 2026-09-21, when
		   almost-touching turned out to read as one string on a phone and to
		   put the mark's hover arrow over the clock. */
		gap: 0.5rem;
		min-width: 0;
		padding: 0 0.9rem 0 0.5rem;
	}

	/* The one seam that still wants air: what is typed, away from the clock. */
	.section.input .prompt {
		margin-left: 0.35rem;
	}

	/* The row's items are CENTRED IN THE WHOLE SECTION. vividn, 2026-09-18:
	   "Right now in the input section, all the text is at the very top of the
	   bar and there is a huge margin on the bottom. Center the text vertically
	   in the input section." They used to centre in what was left above a
	   `padding-bottom` the width of the ACTION strip, which put them a strip's
	   half-height too high and left the strip's whole height empty when no
	   command had answered.

	   So there is no padding, and the strip OVERLAYS the bottom of the row
	   instead — it is 0.7rem type on the last few px of a 2.9rem row, under
	   the line's own descender space and beside nothing, because the words
	   begin at the `❯` and the box's text begins after it. */
	.section.input {
		position: relative;
		flex: none;
		height: var(--input-height);
	}

	/* Output only, and the full width of the bar. */
	/* As tall as the bar is open, not a fixed height: the first
	   `--output-height` px of the pull ARE this section growing (`--shown`,
	   set on the shell). Clipped, and anchored at the BOTTOM, so a half-open
	   one shows the foot of what it says, next to the input, rather than
	   squashing it. The rule under it is the seam to the input row. */
	.section.output {
		flex: none;
		flex-direction: column;
		align-items: stretch;
		justify-content: flex-end;
		height: var(--shown, 0px);
		overflow: hidden;
		border-bottom: 1px solid var(--tint);
	}

	.pull {
		position: relative;
		flex: none;
	}

	/* THE × PAST THE NEWEST (da-3eczg): at the strip's right edge, under
	   it, and the strip is opaque, so it covers the × until it slides off. */
	.clear-strip {
		position: absolute;
		right: 0;
		bottom: 0;
		display: flex;
		flex-direction: column;
		align-items: center;
		justify-content: center;
		height: calc(var(--output-height) - 1px);
		padding: 0;
		background: none;
		border: none;
		color: var(--quiet);
		font-family: ui-monospace, SFMono-Regular, Menlo, Consolas, monospace;
		font-size: 0.7rem;
		cursor: pointer;
	}

	/* The × in its circle, which comes out with the pull (da-uqx7n). */
	.clear-strip span {
		display: grid;
		place-items: center;
		box-sizing: border-box;
		width: 1.6rem;
		height: 1.6rem;
		border: 1.5px solid currentColor;
		border-radius: 50%;
		font-size: 1.2rem;
		line-height: 1;
		scale: calc(0.4 + 0.6 * var(--grown));
	}

	.clear-strip:hover,
	.clear-strip:focus-visible {
		color: var(--ink-bright);
	}

	/* Past the threshold: let go now clears. */
	.clear-strip.bright {
		color: var(--accent);
	}

	.clear-strip.bright span {
		background: var(--accent);
		color: var(--ground);
	}

	/* THE STRIP (da-qua5g): one row of commands that scrolls sideways, the
	   row's whole height — the section's, less the rule under it. A size
	   container, so a group is sized in shares of the row (`cqw`) and
	   never by what it holds. `pan-x` again because this is a scroll
	   container, and `touch-action` stops at the nearest one (see `.field`
	   above): sideways is the strip's, up and down the bar's. */
	.roll {
		flex: none;
		box-sizing: border-box;
		height: calc(var(--output-height) - 1px);
		padding-block: var(--output-pad);
		/* The strip's type, a step down from the input's (da-7xp82), and the
		   line the section's height is counted in (routes/+layout.svelte). */
		font-size: var(--output-font);
		line-height: 1.35;
		overflow-x: auto;
		overflow-y: hidden;
		/* Past the end, the pull is the page's (da-3eczg), not the browser's stretch. */
		overscroll-behavior-x: none;
		scrollbar-width: thin;
		container-type: inline-size;
		touch-action: pan-x;
		position: relative;
		z-index: 1;
		background: var(--ground);
	}

	/* Let go short of the ×, the strip springs back over it; under a finger, it follows. */
	.roll:not(.pulling) {
		transition: transform 0.2s ease-out;
	}

	/* At least the row's width, and the groups at its RIGHT end while they
	   do not fill it: the newest is always the last thing before the edge.
	   An auto margin and not `justify-content: flex-end`, which would push
	   an overflow off the left where no scroll reaches it. */
	.track {
		display: flex;
		width: max-content;
		min-width: 100%;
		height: 100%;
	}

	.group:first-child {
		margin-left: auto;
	}

	/* A COLUMN, because a group is a row of the history (CliEntry.svelte):
	   its head — the time and the command — sits ABOVE what the command
	   answered, exactly as it does in the log, and the entry stands on its
	   foot, as a terminal's last answer stands on the prompt. */
	/* AS WIDE AS WHAT IT SAYS (da-7xp82): vividn, «so that it is not so wide.
	   That way we can fit multiple entries in view more easily on mobile».
	   A group was 21rem a document whatever it held; it is now its
	   documents' own width, never past the row's, and never less than
	   22ch, which leaves its head the command's first word or two after
	   the time and the `❯` (`15:00:00 ❯ occur wa…`) and keeps two whole on
	   a 390px phone. A longer command does not widen it: it is cut. */
	.group {
		display: flex;
		flex-direction: column;
		justify-content: flex-end;
		flex: none;
		width: max-content;
		min-width: min(22ch, 100cqw);
		max-width: 100cqw;
		height: 100%;
		/* Sideways only: the undo's hit box reaches up out of the group to the
		   roll's edge (CliEntry.svelte), and what overflows the foot is cut by
		   what holds it. */
		overflow-x: clip;
	}

	.group.wide {
		width: 100cqw;
	}

	.group:not(.wide) :global(.head) {
		width: 0;
		min-width: 100%;
	}

	/* The command's frame is the group's whole height (CliEntry.svelte,
	   da-hxjrk): the head at its top, what the command answered under it,
	   and what does not fit cut at its foot by what holds it (`.docs`, and
	   `.raw`, which keeps its last lines): not by the frame, whose time and
	   step stand half outside it. */
	.group :global(.frame) {
		display: flex;
		flex-direction: column;
		flex: 1;
		min-height: 0;
	}

	/* What Docs.svelte measures for its `+N` and its brace is the room the
	   row may give, the row's width, not the documents' own. */
	.group :global(.docs .room) {
		width: 100cqw;
	}

	/* Each document its own width, not an equal share of the group's. */
	.group :global(.docs > .doc) {
		flex: 0 1 auto;
	}

	/* The strip's type, not the 1rem the history reads documents at; the
	   brace's rows keep their own size (Doc.svelte). */
	.group :global(.doc) {
		font-size: inherit;
		line-height: inherit;
	}

	/* The documents take what the head leaves of the frame. */
	.group :global(.frame > .docs) {
		flex: 1;
		min-height: 0;
		height: auto;
	}

	/* A long value is cut, rather than widening its command's group. */
	.group :global(.doc .rows) {
		max-width: 16rem;
	}

	/* THE BOUNDARY between two commands is their frames' (CliEntry.svelte),
	   with a gap between them. */
	.group + .group {
		margin-left: 0.5rem;
	}

	/* The raw lines take what the head leaves and keep the LAST of them:
	   `cap` asks for more than fit, and a column standing on its foot cuts
	   the overflow from the top. */
	/* `.raw.raw`: RawLines' own `.raw` centres its lines at the same
	   specificity, and which of two equal rules wins is the bundler's order. */
	.group :global(.raw.raw) {
		flex: 0 1 auto;
		min-height: 0;
		justify-content: flex-end;
		overflow: hidden;
	}

	/* Each line its own height: a line clips its own overflow sideways, and
	   a flex item that clips may shrink to nothing, which squashed every line
	   into the box instead of cutting the oldest. */
	.group :global(.raw p) {
		flex: none;
	}

	.section.output :global(.result) {
		flex: 1;
		height: 100%;
	}

	/* A FIXED width, and the same width whatever the name is: a database's
	   name never moves the box beside it, and never takes a pixel from it.
	   The number is the width the mark is fitted INTO — the block is a size
	   container and the name's type is a share of it (Brand.svelte) — and
	   it is smaller than the column the mark used to have, which is the
	   space vividn asked the mark to give back now that it stands beside the
	   clock rather than over it. */
	.brand-block {
		display: flex;
		align-items: center;
		/* The name sits at the block's LEFT, near the screen's edge, and the
		   block's leftover falls between it and the clock.
		   
		   This is the third setting of one number and the reason is worth
		   keeping. The block is a FIXED width so a long database name can
		   never take a pixel from the box; a short name does not fill it, so
		   the leftover has to go somewhere. Left-aligned it goes into the gap
		   (where it started); right-aligned it goes to the outside, which is
		   what vividn asked for on 2026-09-18 — "you can sacrifice the margin in
		   between it and the clock more" — and what made the mark 16px in from
		   the edge with 1.6px before the clock. Seeing that on a phone he
		   amended it, 2026-09-21: "reduce left margin of db/logo, and put a
		   little more space between it and the clock. Right now the clock and
		   db select arrow overlap." So it is left-aligned again, and the gap
		   is the leftover plus `.section`'s own.
		   
		   1.6px was not only an overlap. The arrow is invisible until hover
		   and a phone has no pointer, so what a reader saw at rest was
		   `datum09:35:58` reading as one string. */
		justify-content: flex-start;
		flex: 0 0 4.6rem;
		min-width: 0;
		line-height: 1.1;
		container-type: inline-size;
	}

	.head {
		display: contents;
	}

	.stamp {
		display: flex;
		align-items: center;
		flex: none;
		color: var(--ink-bright);
		font-family: ui-monospace, SFMono-Regular, Menlo, Consolas, monospace;
		font-size: 1.05rem;
		font-variant-numeric: tabular-nums;
	}

	/* The rest of the row, and never less than this: a command line you
	   cannot read what you typed in is not a command line (the bead's trap,
	   and fit.test.mjs's claim at rest). 9rem is about thirteen characters
	   at the prompt's own size. */
	/* Stretched and RELATIVE, because the ACTION strip hangs off its bottom
	   edge and the `❯` is its left edge. The line inside still centres. */
	.prompt {
		position: relative;
		display: flex;
		align-items: center;
		align-self: stretch;
		flex: 1;
		min-width: 9rem;
	}

	/* The row's own padding. There is no `border-top: none` here any more:
	   the line no longer draws a rule for this row to take away
	   (Prompt.svelte). An override was the wrong shape — Svelte bumps a
	   scoped selector's specificity, so it lost, and a hairline the code
	   said was gone was drawn from the `❯` to the right edge for as long as
	   something else happened to cover it (vremya on PR #46). It is
	   asserted computed in bar.test.mjs. */
	.prompt :global(.line) {
		flex: 1;
		min-width: 0;
		padding: 0 0.2rem;
	}

	/* The tab: it comes out of the input's top border at the far right, its
	   own ground covering the rule under it, and fits its six characters.
	   vividn, 2026-10-30: «It should just come right out of the border of the
	   input widget. The text on it will always have exactly 6 characters so it
	   can be made to fit the text exactly and needs very small margins». No
	   higher than the nowview's handles leave (reachable.test). A finger's
	   24px goes on down into the row unseen, under the box's own controls
	   (Prompt.svelte). */
	.reopen {
		position: absolute;
		right: 0;
		bottom: var(--input-height);
		z-index: 21;
		display: flex;
		align-items: center;
		justify-content: center;
		width: calc(6ch + 0.4rem + 1px);
		height: 14px;
		padding: 0;
		background: var(--ground);
		border: 1px solid var(--rule);
		border-width: 1px 0 0 1px;
		border-radius: 4px 0 0 0;
		color: var(--quiet);
		font-family: ui-monospace, SFMono-Regular, Menlo, Consolas, monospace;
		font-size: 0.7rem;
		line-height: 1;
		cursor: grab;
	}
	.reopen::after {
		content: '';
		position: absolute;
		inset: 0 0 -10px;
	}
	.reopen svg {
		width: 1.5rem;
		height: 4px;
		transform: rotate(180deg);
		fill: none;
		stroke: currentColor;
		stroke-width: 1.5;
		stroke-linecap: round;
		stroke-linejoin: round;
	}
	/* Held bright for most of its life and then gone: a word that fades
	   evenly from the first second reads as unimportant for fourteen of
	   them. */
	.said {
		white-space: nowrap;
		animation: mark-fade var(--fade) ease-out forwards;
	}
	@keyframes mark-fade {
		0%,
		80% {
			opacity: 1;
		}
		100% {
			opacity: 0;
		}
	}
	/* datum's own color for the tag, inline from `action_color` (da-ckw3);
	   this is the fallback for a module too old to publish one. */
	.action {
		color: var(--accent-bright);
		white-space: nowrap;
	}
	.chip {
		background: var(--error);
		border-radius: 2px;
		color: var(--page);
		font-weight: 700;
	}
	.reopen:hover,
	.reopen:focus-visible {
		color: var(--accent-bright);
	}

	.commands-over {
		position: absolute;
		left: 0;
		right: 0;
		bottom: var(--input-height);
		z-index: 22;
		height: var(--output-height);
		overflow: hidden;
	}

	.commands {
		display: flex;
		height: 100%;
		background: var(--ground);
		border-top: 1px solid var(--accent);
		border-bottom: 1px solid var(--tint);
	}

	/* A scroll that reaches the end of the buttons stops there, and never
	   goes on to the page. */
	.commands :global(.bar) {
		overscroll-behavior: contain;
	}

	/* Shown, then gone at the same moment, for a reader who asked for no
	   animation — the store's timer is what actually takes it away. */
	@media (prefers-reduced-motion: reduce) {
		.said {
			animation: none;
		}
		.roll:not(.pulling) {
			transition: none;
		}
	}

	/* A phone: the row keeps its four things and gives the box the rest. */
	@media (max-width: 480px) {
		.section {
			gap: 0.4rem;
			/* "reduce left margin of db/logo" (vividn, 2026-09-21): the bar's own
			   left padding is that margin, and the mark now sits against it. */
			padding-inline: 0.25rem 0.4rem;
		}
		/* vividn, 2026-09-28: side by side, the mark and the clock took about
		   40% of a phone's bar, and `occur tea amount=3 cups=1` showed only
		   its pills. So they share one column, the mark over the clock, and
		   the column is as wide as the clock — a name never widens it. */
		.head {
			display: flex;
			flex-direction: column;
			align-items: flex-start;
			flex: none;
		}
		.brand-block {
			flex: none;
			width: 4rem;
		}
		.stamp {
			font-size: 0.9rem;
			line-height: 1.1;
		}
		.prompt :global(.line) {
			gap: 0.3rem;
			padding-inline: 0;
		}
	}
</style>
