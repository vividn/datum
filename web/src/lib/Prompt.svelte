<!--
	THE command line: the caret, the box, the pills that ride after what is
	typed in it, and the Enter that runs it.

	It is drawn in two places and it is ONE line (da-3zon): the bottom bar
	(PromptBar.svelte) and every `input` widget on the page
	(layout/InputWidget.svelte). The box binds straight to `text` in
	#lib/bar.ts and Enter calls `submitLine` there, so nothing here is a
	copy of anything — type in either and both show it, Enter in either
	runs it, and both empty together. Two local strings synced by effects is
	how two boxes drift by a keystroke.

	The clock left in da-xfa4: it is BESIDE the logo now (da-ckw3), on this
	one row, almost touching it — vividn: "decrease margin between db/logo and
	clock, they are different colors so they can be almost touching".

	Enter empties the box the way a shell consumes a line. Up and down walk
	the history every prompt shares (#lib/cliStore.ts), shell-style: the
	draft you were typing is kept and handed back when you walk past the
	newest entry — kept per BOX, since it is what this reader was in the
	middle of typing here.

	THERE IS NO PLACEHOLDER while the database is open. vividn, 2026-09-14:
	"Let's get rid of the always present placeholder `occur coffee cups=2`,
	that doesn't add to the experience for new users and is annoying for
	non-new users." This supersedes da-e5u7, which chose that text after
	the one before it (`occur coffee 2 cups`) turned out not to parse: the
	worked examples the empty CLI widget offers are still there, run by
	`hints.test.mjs`, and they are where a first command comes from now.
	`opening database…` stays: that one says STATE, not example.

	`ghost` is a command the reader did not type, shown for about a second
	where the placeholder was and fading out of it (da-jprg). A span over
	the box rather than the placeholder itself because Firefox animates
	neither a transition nor a keyframe on `::placeholder` — measured
	2026-09-14 on chromium 1223, webkit 2336 and firefox 1538, where only
	the first two interpolate. It is never the input's value: the box is
	the reader's, and the moment they touch a key the ghost is gone.

	THE PILLS ride just after the typed text (da-xfa4). vividn: "move the -t
	pills to be just slightly to the right of the >, so that they are
	visible as you type text (they can move to the right as you type more
	text)". Following the text needs the text's width, and `field-sizing:
	content` is not everywhere it has to work — measured 2026-09-14:
	chromium 1223 has it, webkit 2336 and firefox 1538 do not — so the
	width comes from a hidden MIRROR span in the box's own font, which
	every engine can measure. The mirror carries the ghost's line while a
	ghost is up, so the pills are part of the ghost and go back leftward
	when it fades. Each instance measures its OWN text, so a widget's pills
	sit past what was typed in the widget.

	THE PILLS ARE ELEMENTS OF THE LINE (da-0eqgg). vividn, 2026-11-06: «you
	should be able to put text on any side of or in between the pills, like
	the pills are just elements in the text. So if I click after all the
	pills the cursor should go there», and «pressing backspace when the cursor
	is immediately after a pill should delete it». The line is `text`, then
	each argument pill and what is typed after it (`after`, #lib/bar.ts). ONE
	box still holds the caret, wherever it stands: it is the segment the caret
	is in, put among the pills by its flex `order`, and every other segment is
	drawn as text that a press moves the box into. One element, so every
	caller of the box — `focusLine`, a test's `.prompt-bar input` — keeps
	finding the one it found. A pill is one character to the caret: an arrow
	at a segment's edge crosses it, Backspace after it takes it, and its text
	joins the segment before. A selection stays within one segment.

	When text and pills outgrow the box the pills KEEP following: they go
	off the right edge, and the reader scrolls the line rightward to reach
	them, exactly as they would to read the end of what they typed. vividn,
	2026-09-18: "They should never go over the > at all. They should be to
	right, and just go off the right edge of the screen." Text and pills are
	ONE STRIP in one scrollport (`.field` and `.run` below), so that is one
	scroll and not two. (Two earlier rules stood here and he has ruled out
	both: parking the pills at the box's right edge, and letting them reach
	back over the `❯`.)

	THE KEY FRAMES (da-31d1d, #lib/keyframes.ts): on `occur|start|add
	<field>` or `switch <field> <state>` (never `end`, da-o4jaa) for a field
	whose spec declares keys, each key written on the line — a bare value,
	which the CLI binds in order (da-p3hxn), or a `key=value` — is outlined
	where it stands, drawn over the text by a copy of it in the box's font,
	and each key still to write waits in a frame after the text, before the
	pills — its name, and a default as ghost text. An assigned key is fixed
	and gets none. A required key's frame is solid in the accent, the rest
	dashed in the quiet line. A space in an empty frame skips it, but not
	inside quotes; Enter takes an empty `key=` off, and the CLI fills the
	defaults.

	A PASTE OF SEVERAL LINES runs them (da-jxfdw): an agent that cannot run
	commands writes its person a setup to paste here, and a one-line box
	would join it into one line that fails. Each line runs in order as if
	typed and entered, blank lines and `#` lines skipped; the first that
	fails stops it, and stays in the box with the lines after it held
	behind it (`+N`), so Enter, once it is fixed, goes on from there.

	THE `❯` IS A BUTTON in the bar (da-xw655): a circle around the chevron.
	vividn, 2026-10-27: «When it is clicked … it rotates 90 degress (with
	animation please), inverts colors, (so that the button is filled in and
	the chevron is empty), and the command widget slides up overtop the
	output section» (`commandsOpen`, PromptBar.svelte). An input widget's
	`❯` stays the mark it was: the widget is over nothing.
-->
<script lang="ts">
	import { onMount, tick, untrack } from 'svelte';
	import { act } from '#lib/act.ts';
	import {
		after,
		clearLine,
		commandsOpen,
		ghost,
		joinAt,
		lineOf,
		pills,
		registerLine,
		removePill,
		segments,
		spent,
		submitLine,
		text
	} from '#lib/bar.ts';
	import { isLength, roleOf, saysKey, valuedFlags } from '#lib/commands.ts';
	import LengthArrow from '#lib/LengthArrow.svelte';
	import Pill from '#lib/Pill.svelte';
	import { fields, grammar, ready } from '#lib/layout/engine.ts';
	import { advance, complete, framesOf, type Frame } from '#lib/keyframes.ts';
	import { cli } from '#lib/cliStore.ts';

	let {
		/**
		 * True in an input widget. It is where a focus that nobody clicked
		 * goes while one is on screen (`focusLine` in #lib/bar.ts).
		 */
		widget = false
	}: { widget?: boolean } = $props();

	/** The air between what is typed and the first pill. */
	const GAP = 10;
	/** Room for the caret past the last character, so it is never clipped. */
	const CARET = 3;

	/** Index into the history, or -1 while editing a fresh line. */
	let cursor = -1;
	let draft = '';
	/** A pasted block's lines after the one in the box, which Enter runs next. */
	let held: string[] = $state([]);

	let box: HTMLInputElement;
	/** The scrollport: `.field`, which the run is longer than. */
	let port: HTMLElement;
	let textWidth = $state(0);

	onMount(() => registerLine(box, widget));

	/**
	 * The keyboard back (da-k34p). A command button focuses the box with
	 * `inputmode="none"` (`focusLine`); a `none` left behind is a prompt
	 * nobody can type in on a phone. So it goes on pointerdown — before the
	 * browser handles the tap, so the keyboard opens on THAT tap — on blur,
	 * and whenever the line is empty again.
	 */
	const keyboard = () => box?.removeAttribute('inputmode');
	$effect.pre(() => {
		if (!$text) untrack(keyboard);
	});
	$effect.pre(() => {
		if (!$text) held = [];
	});

	/**
	 * The pills by ROLE (da-tu30m): context stands before the box like a
	 * breadcrumb — where the line is, not what is in it — and the
	 * arguments ride after the text, where they go into the argv.
	 */
	const context = $derived($pills.filter((p) => roleOf(p, $pills) === 'context'));
	const args = $derived($pills.filter((p) => !context.includes(p)));

	/**
	 * Where the box stands (da-0eqgg): after the pill with this id, or before
	 * every pill. An id, so a pill taken away elsewhere does not move the
	 * caret to another segment; one gone puts it back before them all.
	 */
	let seatId: string | null = $state(null);
	const seat = $derived(seatId === null ? 0 : args.findIndex((p) => p.id === seatId) + 1);
	const idOf = (i: number): string | null => (i ? args[i - 1].id : null);
	const segOf = (i: number): string => $segments[i] ?? '';
	function setSeg(i: number, value: string): void {
		if (!i) $text = value;
		else after.update((tails) => ({ ...tails, [args[i - 1].id]: value }));
	}

	/** Put the caret `offset` into segment `i`. Focused at once, so a phone's keyboard comes with the tap. */
	async function moveTo(i: number, offset: number): Promise<void> {
		seatId = idOf(i);
		box.focus();
		await tick();
		box.setSelectionRange(offset, offset);
	}

	/** A press on a segment drawn as text: the box moves there, the caret under the press. */
	function press(i: number, event: PointerEvent): void {
		event.preventDefault();
		keyboard();
		const typed = (event.currentTarget as HTMLElement).firstElementChild!.getBoundingClientRect();
		const length = segOf(i).length;
		// One face, so every character is one width.
		const at = length ? Math.round(((event.clientX - typed.left) / typed.width) * length) : 0;
		void moveTo(i, Math.max(0, Math.min(length, at)));
	}

	/** A pill is one character to the caret: the arrows cross it, Backspace after it takes it. */
	function keys(event: KeyboardEvent): void {
		recall(event);
		if (event.defaultPrevented || event.shiftKey || event.altKey || event.ctrlKey || event.metaKey) return;
		const at = box.selectionStart;
		if (at !== box.selectionEnd) return;
		const start = at === 0 && seat > 0;
		const end = at === box.value.length && seat < args.length;
		if (event.key === 'ArrowLeft' && start) void moveTo(seat - 1, segOf(seat - 1).length);
		else if (event.key === 'ArrowRight' && end) void moveTo(seat + 1, 0);
		else if (event.key === 'Backspace' && start) drop(seat - 1);
		else return;
		event.preventDefault();
	}

	/**
	 * Take the pill after segment `i`; what followed it joins `i` (`joinAt`),
	 * and the caret stays just before that text, where it stood after the pill.
	 */
	function drop(i: number): void {
		const tail = segOf(i + 1);
		const seam = joinAt(segOf(i), tail).length - tail.length;
		removePill(args[i].id);
		void moveTo(i, seam);
	}

	/** The flags whose next word is their value, which no key's frame takes. */
	const valued = $derived(valuedFlags($grammar?.get($text.trimStart().split(/\s/)[0])));
	const frames = $derived(framesOf($text, $fields, valued));
	/** The keys still to write, waiting in their frames after the text. */
	const waiting = $derived(frames?.filter((f) => f.from === undefined) ?? []);
	/** An optional key written with nothing in it yet, at the end: its default is ghosted after it. */
	const trailing = $derived(frames?.find((f) => f.value === '' && f.default !== undefined && f.to === $text.length));
	/** The line cut at the frames, for the copy that draws them. */
	const pieces = $derived(cut($text, frames ?? []));

	function cut(line: string, frames: Frame[]): { text: string; frame?: Frame }[] {
		const out: { text: string; frame?: Frame }[] = [];
		let at = 0;
		for (const frame of frames.filter((f) => f.from !== undefined).sort((a, b) => a.from! - b.from!)) {
			out.push({ text: line.slice(at, frame.from) }, { text: line.slice(frame.from, frame.to), frame });
			at = frame.to!;
		}
		return [...out, { text: line.slice(at) }];
	}

	/** Space advances to the next key's frame — at the end of the line, outside quotes. */
	async function space(event: InputEvent): Promise<void> {
		if (event.inputType !== 'insertText' || event.data !== ' ') return;
		if (seat || box.selectionStart !== $text.length || box.selectionEnd !== $text.length) return;
		const next = advance($text, $fields, valued);
		if (next === null) return;
		event.preventDefault();
		$text = next;
		await tick();
		box.setSelectionRange(next.length, next.length);
	}

	/** What the box is as wide as: what is typed in its segment, or the ghost while one is up. */
	const mirrored = $derived(
		seat ? segOf(seat) : $text ? $text + (trailing?.default ?? '') : $ghost ? $ghost.line : ''
	);
	/**
	 * THE LINE IS ONE SCROLLING RUN: the box, a gap, then the pills.
	 *
	 * vividn, 2026-09-18: "They should never go over the > at all. They should
	 * be to right, and just go off the right edge of the screen. The user can
	 * scroll to the right when input text gets long, so it should be
	 * equivalently present in this rightwards scroll."
	 *
	 * So the pills are not an overlay pinned over the box any more — they are
	 * the far end of the line itself. `.field` is the scrollport, `.run` is
	 * the line inside it, and the pills sit at the end of the run. What goes
	 * past the right edge is reached by scrolling right, which is the same
	 * gesture that reaches the end of a long command, and the two are the
	 * same scroll because they are the same strip.
	 *
	 * The BOX takes what is left of the row and never less than the text, so
	 * the caret is always inside it and the browser keeps the caret on screen
	 * by scrolling the run — never by scrolling the text out from under the
	 * pills. That was the fault in the first build of this rule: the box
	 * reserved the pills' width as its own right padding, and a border-box
	 * element cannot be narrower than its padding, so on a 390px phone two
	 * pills made the box 365px wide inside a 211px slot. Everything typed was
	 * drawn off the right of the screen. Measured, and in the PR.
	 *
	 * Two earlier rules vividn has ruled out: standing the pills at the box's
	 * RIGHT EDGE when they outgrow it, and (this bead's own, which vremya
	 * asked for and he overruled) letting them reach leftward over the `❯`.
	 */
	/**
	 * The box: as wide as what is in it, and the LAST segment takes what is
	 * left of the row (`.last`), so a press past every pill is a press in it.
	 * Never under the text, so the caret is always inside the box and the box
	 * itself never scrolls — the ONE scroll is the run's, which is what makes
	 * the pills part of the same gesture.
	 */
	const inputWidth = $derived(textWidth + CARET);
	$effect.pre(() => {
		void inputWidth;
		void seat;
		untrack(() => void follow());
	});

	/**
	 * Keep the caret on the screen after the box has been RESIZED.
	 *
	 * The browser does reveal a caret by scrolling the scrollport — but it
	 * does it on the keystroke, with the width the box had before it grew for
	 * the character just typed. And the box grows a beat later still: its
	 * width comes from the mirror, which is measured by a resize observer and
	 * not by the keystroke. So the browser's scroll is always a character or
	 * two behind, and on a long line the tail is drawn off the right edge.
	 *
	 * Hence: react to the WIDTH, not to the keystroke, and put a caret at the
	 * END of the line back at the edge — which is where a terminal keeps it.
	 * A caret in the middle is left to the browser, whose error there is one
	 * character and invisible. Nothing happens unless this box has the focus,
	 * so a pill appearing beside an idle prompt moves nothing.
	 */
	async function follow(): Promise<void> {
		await tick();
		if (!port || !box || document.activeElement !== box) return;
		if (box.selectionStart !== box.value.length) return;
		// An EMPTY box before the pills is at its start, whatever the run is
		// long: a ghost is what widens it there (the mirror carries the
		// ghost's line), and a ghost the reader did not type is no reason to
		// scroll their caret off the left edge (canary on PR #46).
		if (!box.value) return void (seat || (port.scrollLeft = 0));
		const caret = box.offsetLeft + parseFloat(getComputedStyle(box).paddingLeft) + inputWidth + CARET;
		port.scrollLeft = Math.max(0, caret - port.clientWidth);
	}

	function submit(): void {
		const { line, inline } = lineOf(complete($text, $fields, valued));
		if (!line.trim()) return;
		if (held.length) return void runLines([line, ...held], inline);
		$text = '';
		after.set({});
		seatId = null;
		draft = '';
		cursor = -1;
		void submitLine(line, inline);
	}

	/** A paste with a line break in it is a block of commands, put where the caret is. */
	function paste(event: ClipboardEvent): void {
		const pasted = event.clipboardData?.getData('text/plain') ?? '';
		if (!/[\r\n]/.test(pasted) || seat) return;
		event.preventDefault();
		const block = $text.slice(0, box.selectionStart ?? $text.length) + pasted + $text.slice(box.selectionEnd ?? $text.length);
		void runLines(block.split(/\r\n|\r|\n/));
	}

	/** Run each line in turn; the first that fails stays in the box, the rest held behind it. */
	async function runLines(lines: string[], inline: typeof args = []): Promise<void> {
		const todo = lines.filter((l) => l.trim() && !l.trimStart().startsWith('#'));
		$text = '';
		after.set({});
		seatId = null;
		draft = '';
		cursor = -1;
		for (const [i, line] of todo.entries()) {
			if ((await submitLine(line, i ? [] : inline)).code === 0) continue;
			$text = line;
			held = todo.slice(i + 1);
			return;
		}
	}

	function recall(event: KeyboardEvent): void {
		// The box is the reader's the moment they type in it.
		if ($ghost) ghost.set(null);
		const { history } = $cli;
		if (event.key === 'ArrowUp') {
			const next = cursor === -1 ? history.length - 1 : cursor - 1;
			if (next < 0) return;
			if (cursor === -1) draft = $text;
			event.preventDefault();
			seatId = null;
			cursor = next;
			$text = history[cursor];
		} else if (event.key === 'ArrowDown') {
			if (cursor === -1) return;
			event.preventDefault();
			cursor += 1;
			if (cursor >= history.length) {
				cursor = -1;
				$text = draft;
			} else {
				$text = history[cursor];
			}
		}
	}

	/**
	 * A spent pill fades on its ghost's own clock (da-d7ndh). The delay of
	 * `-age` says where the ghost was when the press landed, but each
	 * animation starts at the next frame it gets, and on a loaded CI runner
	 * that left the pill 53ms behind its ghost. Taking the ghost's start
	 * time leaves nothing to drift. A ghost not drawn yet has none to take,
	 * and then the pill, at no delay, starts on the frame the ghost does.
	 * With no ghost up, or no motion (no animations at all), the delay stands.
	 */
	function withGhost(node: HTMLElement): void {
		const ghost = node.closest('.run')?.querySelector('.ghost')?.getAnimations()[0];
		const fade = node.getAnimations()[0];
		if (!fade || !ghost) return;
		fade.effect?.updateTiming({ delay: 0 });
		if (ghost.startTime !== null) fade.startTime = ghost.startTime;
	}
</script>

<form class="line" style:--gap="{GAP}px" style:--caret="{CARET}px" onsubmit={(event) => (event.preventDefault(), submit())}>
	{#if widget}
		<span class="prompt">❯</span>
	{:else}
		<!-- Not the box's: a press here never takes the caret, so a phone's
		     keyboard stays down while the commands are up. -->
		<button
			type="button"
			class="prompt"
			class:open={$commandsOpen}
			aria-label="Commands"
			aria-expanded={$commandsOpen}
			aria-controls="bar-commands"
			onclick={() => commandsOpen.update((open) => !open)}><span>❯</span></button
		>
	{/if}
	<!-- After the `❯`, not before it: the `❯` stays where it always stood,
	     and the ACTION words that start under it (PromptBar.svelte) too. -->
	{#if context.length}
		<span class="context">
			{#each context as pill (pill.id)}
				<Pill {pill} context />
			{/each}
			<span class="crumb" aria-hidden="true">›</span>
		</span>
	{/if}
	<span class="field" bind:this={port}>
		<!-- A segment's `order` is twice its index and a pill's one more, so
		     the box, wherever it is in the markup, stands where its segment is. -->
		<span class="run">
		<input
			type="text"
			style="width: {inputWidth}px; order: {2 * seat}"
			class:after={seat > 0}
			class:last={seat === args.length}
			class:easing={!segOf(seat)}
			bind:this={box}
			bind:value={() => segOf(seat), (value) => setSeg(seat, value)}
			onkeydown={keys}
			onbeforeinput={space}
			onpaste={paste}
			onpointerdown={keyboard}
			onblur={keyboard}
			disabled={!$ready}
			placeholder={$ready ? '' : 'opening database…'}
			spellcheck="false"
			autocapitalize="none"
			autocomplete="off"
		/>
		<!-- The box's own font, out of the flow and out of the way: only its
		     width is wanted. -->
		<span class="mirror" aria-hidden="true" bind:clientWidth={textWidth}>{mirrored}</span>
		<!-- Keyed on the object, so a second press restarts the fade rather
		     than joining the first one's. Hidden while there is anything in
		     the box: the ghost has the placeholder's slot and nothing else. -->
		<!-- The key frames, drawn over the text by a copy of it: only the
		     outlines and a default's ghost are seen. One line, so no
		     whitespace of the template's lands in a `pre`; and in ONE inline
		     span, since a flex container drops the spaces between frames. -->
		{#if frames}
			<span class="frames" aria-hidden="true"><span>{#each pieces as piece}{#if piece.frame}<span class="frame" class:required={piece.frame.required} data-key={piece.frame.name}>{piece.text}{#if piece.frame === trailing}<span class="default">{trailing.default}</span>{/if}</span>{:else}{piece.text}{/if}{/each}</span></span>
		{/if}
		{#if $ghost && !$text}
			{#key $ghost}
				<span class="ghost" aria-hidden="true">{$ghost.line}</span>
			{/key}
		{/if}
		{#each $segments as segment, i}
			{#if i !== seat}
				<!-- The press is the box's to take: the caret goes where it lands. -->
				<span
					class="segment"
					class:after={i > 0}
					class:last={i === args.length}
					style:order={2 * i}
					aria-hidden="true"
					onpointerdown={(event) => press(i, event)}
					><span>{segment}</span>{#if !i && trailing}<span class="pad">{trailing.default}</span>{/if}</span
				>
			{/if}
			{#if !i}
				{#if held.length}
					<span class="held" title={held.join('\n')}>+{held.length}</span>
				{/if}
				{#each waiting as key (key.name)}
					<span class="key" class:required={key.required} data-key={key.name}
						>{key.name}{#if key.default !== undefined}<span class="default">{key.default}</span>{/if}</span
					>
				{/each}
			{/if}
			<!-- One pill is `Pill.svelte`, which the nowview's margin draws
			     too (da-rskp): one store, two places to see it, and the same
			     press in either. -->
			{#if args[i]}
				<span class="slot" style:order={2 * i + 1}><Pill pill={args[i]} /></span>
			{/if}
		{/each}
		<span class="slot" style:order={2 * args.length + 1}>
			<!-- Spent, not live: no × and no lock, out of the argv and out of
			     the time cursor, fading with the ghost of the command it went
			     into. A different class for a different thing, so a reader of
			     `.pill` — a test included — is never handed one. -->
			{#each $spent as pill (pill.id)}
				<span class="spent {pill.kind}" style:--pill={pill.color} style:animation-delay="-{pill.age}ms"
					use:withGhost
					>{#if !isLength(pill)}{pill.text}{:else if saysKey([...$pills, ...$spent], $fields)}dur={pill.text}{:else}<LengthArrow
						/>{pill.text}{/if}</span
				>
			{/each}
		</span>
		</span>
	</span>
	<!-- THE ×: the whole input in one tap — the text and every pill, locked
	     too (da-al892). vividn: «x shows when any text or pills are in the
	     input bar and clears it». Not staged the way Escape is: it is the
	     phone's reset, and a phone has no Escape. Outside the scrollport,
	     so it stays put however long the line runs, just before Send. The
	     mousedown's default is the focus moving, so preventing it leaves the
	     focus — and a phone's keyboard, open or shut — where it was. -->
	{#if $text || $pills.length}
		<button
			type="button"
			class="clear"
			use:act={'fill'}
			aria-label="clear the input"
			title="clear the input"
			onmousedown={(event) => event.preventDefault()}
			onclick={clearLine}>×</button
		>
	{/if}
	<!-- SEND (da-1pzwl): Enter without a keyboard. vividn: «Let's add a
	     Enter/Send button on the right side of the input box so that you
	     don't have to open the keyboard and hit enter». It calls Enter's own
	     `submit`. NOT a submit button: that turns every Enter into a
	     synthetic click on it, and with one here Enter after dragging the
	     log open ran nothing (barlog.test.mjs; a click after a drag is
	     what #lib/grip.ts swallows). Shown only when Enter would run
	     something — the verb is always typed, so pills alone are not a
	     line. It acts at once, so the DOUBLE outline; the mousedown
	     is prevented as the ×'s is, so the focus and the keyboard stay put.
	     After the ×, a gap between, so a thumb meant for it clears nothing.
	     And it holds the ×'s place: drawn, HIDDEN, while the × is up and
	     there is nothing to run, so the × never slides right into the spot
	     Send was just pressed in — a locked pill outlives the run, and a
	     second tap there would clear it (vremya on PR #213).
	     Like the ×, it is inside the bar's drag handle, so a tap within
	     300ms of letting go of a drag is taken as that drag's click and
	     does nothing (#lib/grip.ts). Every input widget draws one too: it
	     is the same line in a second place, so its Send runs it. -->
	{#if $text || $pills.length}
		<button
			type="button"
			class="send"
			use:act={'run'}
			class:idle={!$segments.some((segment) => segment.trim())}
			aria-label="run the line"
			title="run the line"
			onmousedown={(event) => event.preventDefault()}
			onclick={submit}>➤</button
		>
	{/if}
</form>

<style>
	/* NO RULE ABOVE IT. The line used to draw its own `border-top`, which
	   is a claim about what is ABOVE it — and the line does not know: in the
	   bar's input row nothing is (da-ckw3), and in an input widget the
	   receipt is. So the rule belongs to the surface, and the one that wants
	   it draws it (InputWidget.svelte).

	   It was not a free leftover in the bar. Svelte bumps a scoped
	   selector's specificity by repeating the class — `.line.svelte-x
	   .svelte-x.svelte-x` — so PromptBar's `.prompt :global(.line)` override
	   could not win, and a hairline the code said was gone was drawn from
	   the `❯` to the right edge (vremya on PR #46). An override that loses
	   is worth less than the declaration never being made. */
	.line {
		/* What is typed, and so what a pill beside it reads at — vividn asked
		   for the two to be one size (da-tnv3), so they are one number. A
		   step up on 2026-09-18: "increase fontsize of input bar slightly". */
		--input-font: 1.25rem;
		/* A context pill (da-tu30m): its size, and how much of a name it
		   shows before it is cut. The two numbers to adjust. */
		--context-font: 0.8rem;
		--context-max: 12ch;
		display: flex;
		align-items: center;
		gap: 0.5rem;
		padding: 0.45rem 0.6rem;
		font-family: ui-monospace, SFMono-Regular, Menlo, Consolas, monospace;
	}

	.prompt {
		color: var(--accent);
	}

	/* The circle, 24px at the least (conventions #12). Open, it turns a
	   quarter back, pointing up the way the commands slide (vividn,
	   2026-10-28), and inverts: the circle filled, the chevron the ground. */
	button.prompt {
		flex: none;
		display: grid;
		place-items: center;
		width: 1.75rem;
		height: 1.75rem;
		padding: 0;
		border: 1.5px solid var(--accent);
		border-radius: 50%;
		background: none;
		font: inherit;
		font-size: 1rem;
		line-height: 1;
		cursor: pointer;
		transition:
			transform 160ms ease-out,
			background-color 160ms ease-out,
			color 160ms ease-out;
	}

	button.prompt.open {
		transform: rotate(-90deg);
		background: var(--accent);
		color: var(--ground);
	}

	/* Its own size, out of the scrollport: it stays put while the line
	   scrolls under what is typed. */
	.context {
		flex: none;
		display: flex;
		align-items: center;
		gap: 0.25rem;
		font-size: var(--context-font);
	}

	.crumb {
		color: var(--quiet);
	}

	/* A phone: the context keeps its first letters, and the line its room. */
	@media (max-width: 480px) {
		.line {
			--context-max: 3ch;
		}
	}

	/* Grey, and quiet until it is wanted: it is a way out, not an action. */
	/* Over the foot of the closed output's tab (PromptBar.svelte). */
	.clear,
	.send {
		position: relative;
		z-index: 22;
	}
	.clear {
		flex: none;
		min-width: 24px;
		min-height: 24px;
		padding: 0;
		border: none;
		background: transparent;
		color: var(--quiet);
		font: inherit;
		font-size: var(--input-font);
		line-height: 1;
		cursor: pointer;
	}

	.clear:hover,
	.clear:focus-visible {
		color: var(--ink);
	}

	.send {
		flex: none;
		min-width: 28px;
		min-height: 28px;
		margin-left: 0.25rem;
		padding: 0 0.3rem;
		border-radius: 3px;
		background: transparent;
		color: var(--ink-bright);
		font: inherit;
		line-height: 1;
		cursor: pointer;
	}

	/* Runs at once, so the double outline (#lib/act.ts). */
	.send:global([data-act='run']) {
		border: 3px double var(--accent);
	}

	.send:hover,
	.send:focus-visible {
		border-color: var(--accent-bright);
	}

	/* Nothing to run: out of sight and out of reach, but still in its place. */
	.send.idle {
		visibility: hidden;
	}

	/* THE SCROLLPORT: what of the line is on the screen. What runs past its
	   right edge — the end of a long command, a pill, both — is reached by
	   scrolling it right, which is the one gesture vividn asked for. Its bar is
	   hidden: this is a 24px row, and a scrollbar drawn in it would be half
	   the row. */
	.field {
		position: relative;
		flex: 1;
		min-width: 0;
		overflow-x: auto;
		overflow-y: hidden;
		scrollbar-width: none;
	}

	.field::-webkit-scrollbar {
		display: none;
	}

	/* THE LINE ITSELF, as long as it is: its segments and its pills, and
	   never shorter than the row. The ghost and the mirror hang off it too,
	   so they scroll with the text they belong to rather than standing still
	   over it. */
	.run {
		position: relative;
		display: flex;
		align-items: center;
		min-width: 100%;
		width: max-content;
	}

	/* A segment, the box's or one drawn as text: half the air between text
	   and a pill on each side of it, so an empty one between two pills is
	   still somewhere to press. Only the last grows, to the end of the row:
	   a box that grew to the scrollport before its pills would put them past
	   the right edge. */
	input,
	.segment {
		flex: none;
		box-sizing: content-box;
		padding: 0 calc(var(--gap) / 2) 0 0;
		font: inherit;
		font-size: var(--input-font);
	}

	input.after,
	.segment.after {
		padding-left: calc(var(--gap) / 2);
	}

	input.last,
	.segment.last {
		flex: 1 0 auto;
	}

	input {
		min-width: 0;
		background: transparent;
		border: none;
		outline: none;
		color: var(--ink-bright);
	}

	/* Drawn as the box draws it, with room for the caret it would hold, and
	   the row's whole height: an empty one has no line box to be tall by. */
	.segment {
		align-self: stretch;
		display: flex;
		align-items: center;
		padding-right: calc(var(--gap) / 2 + var(--caret));
		color: var(--ink-bright);
		white-space: pre;
		cursor: text;
	}

	.segment .pad {
		visibility: hidden;
	}

	.slot {
		display: inline-flex;
		gap: 0.3rem;
	}

	/* An EMPTY box changes width only as a ghost comes or goes, and is SEEN
	   doing it, the pills after it going back leftward with it (vividn: "then
	   they can go to the left back towards the start of the input box"); a
	   box being typed in follows the text at once. */
	input.easing {
		transition: width 120ms ease-out;
	}

	/* Where the placeholder is, in the placeholder's font — the same
	   position, no second row, no layout shift — and out of the way of the
	   caret and of a click. It arrives near the brightness of a typed line
	   (vividn: "a slightly dimmed version") and fades to nothing: arriving
	   dim and fading dimmer is a change too small to read. */
	.ghost {
		position: absolute;
		inset: 0;
		display: flex;
		align-items: center;
		overflow: hidden;
		white-space: nowrap;
		pointer-events: none;
		color: var(--ink);
		font-size: var(--input-font);
		animation: ghost-fade 1s forwards;
	}

	/* The copy the frames are drawn on: the box's text, where the box has
	   it, and invisible — only the outlines and a default are seen. */
	.frames {
		position: absolute;
		inset: 0;
		display: flex;
		align-items: center;
		white-space: pre;
		pointer-events: none;
		color: transparent;
		font-size: var(--input-font);
	}

	/* A key: solid in the accent when the log cannot do without it, dashed
	   in the quiet line when it has a default or may be left out. An
	   outline, so a frame takes no room from the text it is drawn round,
	   and INSIDE the text's box: the scrollport clips what stands past it. */
	.frame {
		border-radius: 3px;
		outline: 1px dashed var(--quiet);
		outline-offset: -1px;
	}

	.frame.required {
		outline: 1px solid var(--accent);
	}

	/* A key still to write: its name, small, in its frame. */
	.key {
		padding: 0.1rem 0.35rem;
		border: 1px dashed var(--quiet);
		border-radius: 3px;
		color: var(--quiet);
		font-size: calc(var(--input-font) * 0.7);
		line-height: 1.2;
		white-space: nowrap;
	}

	.key.required {
		border: 1px solid var(--accent);
		color: var(--accent);
	}

	/* An optional key's default, as ghost text: what the line will say if it is left alone. */
	.default {
		color: var(--quiet);
	}

	.key .default {
		margin-left: 0.4em;
	}

	/* Measured, never seen: the same face, size and spacing as the box, so
	   its width is the text's width in the box. */
	.mirror {
		position: absolute;
		top: 0;
		left: 0;
		visibility: hidden;
		pointer-events: none;
		white-space: pre;
		font: inherit;
		font-size: var(--input-font);
	}

	/* SOLID, and the size of what is typed beside them. vividn, on the
	   deployed build: "We need to make the pills much more prominent, they
	   are almost invisible right now. Solid background color… increase
	   their font size to be the same as the input bar." So the kind's
	   colour — the colour the outline used to be — FILLS the pill and the
	   page's own ground is the ink on it (`--page`'s contract), which is
	   the one pairing the palette already names.

	   `--pill` is that colour, and the only thing a kind sets: the fill,
	   the ink and the locked ring are all derived from it here, so a new
	   kind stays one line. */
	.spent {
		--pill: var(--accent);
		display: inline-flex;
		align-items: center;
		border: none;
		border-radius: 999px;
		color: var(--page);
		background: var(--pill);
		font-family: ui-monospace, SFMono-Regular, Menlo, Consolas, monospace;
		font-size: var(--input-font);
		line-height: 1;
		white-space: nowrap;
	}

	.spent {
		gap: 0.3em;
		padding: 0.25rem 0.55rem;
		pointer-events: none;
		opacity: 0.6;
		animation: spent-fade 1s forwards;
	}

	/* HELD, then gone (da-jrozq). vividn: "stay fully opaque for 250-500 ms,
	   and then quickly fade rather than starting to fade immediately". So
	   400ms at full strength and an ease-in over the rest, which keeps it
	   near full and drops it at the end. The SHAPE only: the whole is still
	   GHOST_MS (#lib/bar.ts), which the timers taking it away keep to. */
	@keyframes ghost-fade {
		0%,
		40% {
			opacity: 1;
			animation-timing-function: ease-in;
		}
		100% {
			opacity: 0;
		}
	}

	/* From the spent pill's own opacity, not from 1: it is a receipt
	   fading, and it never brightens on its way out. The ghost's shape, so
	   the two go together — and a pill spent after the press joins the
	   ghost part-way through: on the ghost's own clock (`withGhost`),
	   or by a negative delay of its `age` when no ghost is up. */
	@keyframes spent-fade {
		0%,
		40% {
			opacity: 0.6;
			animation-timing-function: ease-in;
		}
		100% {
			opacity: 0;
		}
	}

	/* No fade for a reader who asked for no motion: it is shown, and the
	   store's own timer takes it away at the same moment (#lib/bar.ts). */
	@media (prefers-reduced-motion: reduce) {
		.ghost,
		.spent {
			animation: none;
		}
		input.easing,
		button.prompt {
			transition: none;
		}
	}

	.held,
	.key {
		margin-right: 0.3rem;
	}

	.held {
		color: var(--quiet);
		white-space: nowrap;
	}

	input::placeholder {
		color: var(--quiet);
	}

	input:disabled {
		opacity: 0.5;
	}
</style>
