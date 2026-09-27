<!--
	THE command line: the caret, the box, the pills that ride after what is
	typed in it, and the Enter that runs it.

	It is drawn in two places and it is ONE line (da-3zon): the bottom bar
	(PromptBar.svelte) and every `input` widget on the page
	(layout/InputWidget.svelte). The box binds straight to `text` in
	$lib/bar.ts and Enter calls `submitLine` there, so nothing here is a
	copy of anything — type in either and both show it, Enter in either
	runs it, and both empty together. Two local strings synced by effects is
	how two boxes drift by a keystroke.

	The clock left in da-xfa4: it is BESIDE the logo now (da-ckw3), on this
	one row, almost touching it — vividn: "decrease margin between db/logo and
	clock, they are different colors so they can be almost touching".

	Enter empties the box the way a shell consumes a line. Up and down walk
	the history every prompt shares ($lib/replStore.ts), shell-style: the
	draft you were typing is kept and handed back when you walk past the
	newest entry — kept per BOX, since it is what this reader was in the
	middle of typing here.

	THERE IS NO PLACEHOLDER while the database is open. vividn, 2026-09-14:
	"Let's get rid of the always present placeholder `occur coffee cups=2`,
	that doesn't add to the experience for new users and is annoying for
	non-new users." This supersedes da-e5u7, which chose that text after
	the one before it (`occur coffee 2 cups`) turned out not to parse: the
	worked examples the empty REPL offers are still there, run by
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

	When text and pills outgrow the box the pills KEEP following: they go
	off the right edge, and the reader scrolls the line rightward to reach
	them, exactly as they would to read the end of what they typed. vividn,
	2026-09-18: "They should never go over the > at all. They should be to
	right, and just go off the right edge of the screen." Text and pills are
	ONE STRIP in one scrollport (`.field` and `.run` below), so that is one
	scroll and not two. (Two earlier rules stood here and he has ruled out
	both: parking the pills at the box's right edge, and letting them reach
	back over the `❯`.)

	THE KEY FRAMES (da-31d1d, $lib/keyframes.ts): on `occur|start <field>`
	(never `end`, da-o4jaa) for a field whose spec declares keys, each key
	written on the line is outlined where it stands, drawn over the text by
	a copy of it in the box's font, and each key still to write waits in a
	frame after the text, before the pills — its name, and an optional key's
	default as ghost text. A required key's frame is solid in the accent, an optional
	one's dashed in the quiet line. A space at the end writes the next
	key's `key=`, but not inside quotes; Enter writes each unwritten
	optional key's default into the line and runs what the line then says.
-->
<script lang="ts">
	import { onMount, tick } from 'svelte';
	import { act } from '$lib/act.js';
	import { clearLine, ghost, pills, registerLine, spent, submitLine, text } from '$lib/bar.js';
	import { roleOf } from '$lib/commands.js';
	import Pill from '$lib/Pill.svelte';
	import { fields, ready } from '$lib/layout/engine.js';
	import { advance, complete, framesOf, type Frame } from '$lib/keyframes.js';
	import { repl } from '$lib/replStore.js';

	/**
	 * True in an input widget. It is where a focus that nobody clicked
	 * goes while one is on screen (`focusLine` in $lib/bar.ts).
	 */
	export let widget = false;

	/** The air between what is typed and the first pill. */
	const GAP = 10;
	/** Room for the caret past the last character, so it is never clipped. */
	const CARET = 3;

	/** Index into the history, or -1 while editing a fresh line. */
	let cursor = -1;
	let draft = '';

	let box: HTMLInputElement;
	/** The scrollport: `.field`, which the run is longer than. */
	let port: HTMLElement;
	/** The scrollport's width: what the run has before it goes off the edge. */
	let boxWidth = 0;
	let textWidth = 0;
	let pillsWidth = 0;

	onMount(() => registerLine(box, widget));

	/**
	 * The keyboard back (da-k34p). A command button focuses the box with
	 * `inputmode="none"` (`focusLine`); a `none` left behind is a prompt
	 * nobody can type in on a phone. So it goes on pointerdown — before the
	 * browser handles the tap, so the keyboard opens on THAT tap — on blur,
	 * and whenever the line is empty again.
	 */
	const keyboard = () => box?.removeAttribute('inputmode');
	$: if (!$text) keyboard();

	/**
	 * The pills by ROLE (da-tu30m): context stands before the box like a
	 * breadcrumb — where the line is, not what is in it — and the
	 * arguments ride after the text, where they go into the argv.
	 */
	$: context = $pills.filter((p) => roleOf(p, $pills) === 'context');
	$: args = $pills.filter((p) => !context.includes(p));

	$: frames = framesOf($text, $fields);
	/** The keys still to write, waiting in their frames after the text. */
	$: waiting = frames?.filter((f) => f.from === undefined) ?? [];
	/** An optional key written with nothing in it yet, at the end: its default is ghosted after it. */
	$: trailing = frames?.find((f) => f.value === '' && f.default !== undefined && f.to === $text.length);
	/** The line cut at the frames, for the copy that draws them. */
	$: pieces = cut($text, frames ?? []);

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
		if (box.selectionStart !== $text.length || box.selectionEnd !== $text.length) return;
		const next = advance($text, $fields);
		if (next === null) return;
		event.preventDefault();
		$text = next;
		await tick();
		box.setSelectionRange(next.length, next.length);
	}

	/** What the pills follow: what is typed, or the ghost while one is up. */
	$: mirrored = $text ? $text + (trailing?.default ?? '') : $ghost ? $ghost.line : '';
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
	 * The box: the whole visible row, and never narrower than what is in it.
	 *
	 * The row, so a press anywhere in it puts the caret there. Never under
	 * the text, so the caret is always inside the box and the box itself
	 * never scrolls — the ONE scroll is the run's, which is what makes the
	 * pills part of the same gesture.
	 */
	$: inputWidth = Math.max(textWidth + CARET, boxWidth);
	/** The pills, straight after the typed text, and the run they end. */
	$: pillsLeft = textWidth + GAP;
	$: runWidth = pillsWidth ? Math.max(inputWidth, pillsLeft + pillsWidth) : inputWidth;
	$: void follow(inputWidth);

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
	async function follow(width: number): Promise<void> {
		await tick();
		if (!port || !box || document.activeElement !== box) return;
		if (box.selectionStart !== box.value.length) return;
		// An EMPTY box is at its start, whatever the run is long: a ghost is
		// what widens it there (the mirror carries the ghost's line), and a
		// ghost the reader did not type is no reason to scroll their caret
		// off the left edge (canary on PR #46).
		port.scrollLeft = box.value ? Math.max(0, width + CARET - port.clientWidth) : 0;
	}

	function submit(): void {
		const line = complete($text, $fields);
		if (!line.trim()) return;
		$text = '';
		draft = '';
		cursor = -1;
		void submitLine(line);
	}

	function recall(event: KeyboardEvent): void {
		// The box is the reader's the moment they type in it.
		if ($ghost) ghost.set(null);
		const { history } = $repl;
		if (event.key === 'ArrowUp') {
			const next = cursor === -1 ? history.length - 1 : cursor - 1;
			if (next < 0) return;
			if (cursor === -1) draft = $text;
			event.preventDefault();
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
</script>

<form class="line" on:submit|preventDefault={submit}>
	<span class="prompt">❯</span>
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
	<span class="field" bind:this={port} bind:clientWidth={boxWidth}>
		<span class="run" style="width: {runWidth}px">
		<input
			type="text"
			style="width: {inputWidth}px"
			bind:this={box}
			bind:value={$text}
			on:keydown={recall}
			on:beforeinput={space}
			on:pointerdown={keyboard}
			on:blur={keyboard}
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
		<span class="pills" style="left: {pillsLeft}px" bind:clientWidth={pillsWidth}>
			<!-- One pill is `Pill.svelte`, which the nowview's margin draws
			     too (da-rskp): one store, two places to see it, and the same
			     press in either. -->
			{#each waiting as key (key.name)}
				<span class="key" class:required={key.required} data-key={key.name}
					>{key.name}{#if key.default !== undefined}<span class="default">{key.default}</span>{/if}</span
				>
			{/each}
			{#each args as pill (pill.id)}
				<Pill {pill} />
			{/each}
			<!-- Spent, not live: no × and no lock, out of the argv and out of
			     the time cursor, fading with the ghost of the command it went
			     into. A different class for a different thing, so a reader of
			     `.pill` — a test included — is never handed one. -->
			{#each $spent as pill (pill.id)}
				<span class="spent {pill.kind}" style:--pill={pill.color} style:animation-delay="-{pill.age}ms"
					>{pill.text}</span
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
			on:mousedown|preventDefault
			on:click={clearLine}>×</button
		>
	{/if}
	<!-- SEND (da-1pzwl): Enter without a keyboard. vividn: «Let's add a
	     Enter/Send button on the right side of the input box so that you
	     don't have to open the keyboard and hit enter». It calls Enter's own
	     `submit`. NOT a submit button: that turns every Enter into a
	     synthetic click on it, and with one here Enter after dragging the
	     log open ran nothing (barlog.test.mjs; a click after a drag is
	     what $lib/grip.ts swallows). Shown only when Enter would run
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
	     does nothing ($lib/grip.ts). Every input widget draws one too: it
	     is the same line in a second place, so its Send runs it. -->
	{#if $text || $pills.length}
		<button
			type="button"
			class="send"
			use:act={'run'}
			class:idle={!$text.trim()}
			aria-label="run the line"
			title="run the line"
			on:mousedown|preventDefault
			on:click={submit}>➤</button
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

	/* Runs at once, so the double outline ($lib/act.ts). */
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

	/* THE LINE ITSELF, as long as it is: the box, a gap, the pills. The
	   ghost and the mirror hang off it too, so they scroll with the text
	   they belong to rather than standing still over it. */
	.run {
		position: relative;
		display: flex;
		align-items: center;
	}

	/* Its width is the run's to give (`inputWidth`), so `flex: none`: a box
	   that grew to the scrollport would put the caret past the right edge on
	   every long line, which is the fault this shape replaced. */
	input {
		flex: none;
		min-width: 0;
		background: transparent;
		border: none;
		outline: none;
		padding: 0;
		color: var(--ink-bright);
		font: inherit;
		font-size: var(--input-font);
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

	/* On top of the box, following the text. `left` is animated so a pill
	   left behind by a ghost is SEEN going back to its waiting place rather
	   than teleporting (vividn: "then they can go to the left back towards
	   the start of the input box"); at 120ms it still reads as following
	   while someone types. */
	.pills {
		position: absolute;
		top: 50%;
		transform: translateY(-50%);
		display: flex;
		align-items: center;
		gap: 0.3rem;
		transition: left 120ms ease-out;
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
		padding: 0.25rem 0.55rem;
		pointer-events: none;
		opacity: 0.6;
		animation: spent-fade 1s forwards;
	}

	/* HELD, then gone (da-jrozq). vividn: "stay fully opaque for 250-500 ms,
	   and then quickly fade rather than starting to fade immediately". So
	   400ms at full strength and an ease-in over the rest, which keeps it
	   near full and drops it at the end. The SHAPE only: the whole is still
	   GHOST_MS ($lib/bar.ts), which the timers taking it away keep to. */
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
	   ghost part-way through, by a negative delay of its `age`. */
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
	   store's own timer takes it away at the same moment ($lib/bar.ts). */
	@media (prefers-reduced-motion: reduce) {
		.ghost,
		.spent {
			animation: none;
		}
		.pills {
			transition: none;
		}
	}

	input::placeholder {
		color: var(--quiet);
	}

	input:disabled {
		opacity: 0.5;
	}
</style>
