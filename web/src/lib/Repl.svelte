<!--
	The REPL — the day frame's large pane.

	A prompt line at the bottom, scrollback above it. Enter shell-splits the
	typed line the same way the native REPL does, sends it through
	`run(argv)`, and appends the echoed command plus whatever stdout/stderr
	came back. A command that draws rather than writes is asked for at the
	width of the scrollback it lands in — see `fitArgv`. Up and down walk
	the history, shell-style: the draft you were typing is kept and handed
	back when you walk past the newest entry.

	`input` is bindable and `runArgv` is exported so the widget bar
	(da-nz29) lands in here rather than running silently: a press logs and
	its executed argv joins the scrollback and the recall history, so the
	next Up brings the command up ready to edit. The teach-loop is the
	whole point of the widgets sitting next to a command line.

	Every command carries the instant it was run (da-9ozz), stamped here at
	submit rather than carried back in `RunResult` — the seam is `run(argv)`
	and the CLI's result has no such field. The stamp is dim on purpose: a
	row's own `occur_time`, which the renderer already prints, is the fact
	that matters; when the command was typed is context. The prompt's own
	clock is the same label, live.

	And the scrollback survives a reload, out of `localStorage`, because a
	terminal you can't scroll back through yesterday is a terminal that
	forgets what you told it. Restoring is not re-running: what comes back
	is the text that was printed, never a second execution of the command
	that printed it.
-->
<script lang="ts">
	import { clockSecondsLabel, stampLabel } from '$lib/clock.js';
	import { isDegraded, storage } from '$lib/storageState.js';
	import { createEventDispatcher, onMount, tick } from 'svelte';
	import Pane from './Pane.svelte';
	import CliOutput from './CliOutput.svelte';
	import { fitArgv } from './panes.js';
	import { run, type RunResult } from './run.js';
	import { shellSplit } from './shellSplit.js';

	/** The text in the prompt. Bind it to type a command in from outside. */
	export let input = '';
	export let disabled = false;

	const dispatch = createEventDispatcher<{ ran: RunResult }>();

	type Command = { line: string; ranAt: number; result: RunResult };

	/**
	 * Where the scrollback is kept, and how many commands of it.
	 *
	 * The version is in the KEY, not in the value: a store written by an
	 * older shape is then not read at all, rather than read and picked
	 * apart. The cap is what stops a terminal left open for a month from
	 * growing without bound — oldest dropped, the way scrollback goes.
	 */
	const STORE_KEY = 'datum.repl.v1';
	const KEEP = 200;

	let scrollback: Command[] = [];
	let history: string[] = [];
	/** The live clock beside the prompt; also "what today is" for a stamp. */
	let now = new Date();
	/** Index into `history`, or -1 while editing a fresh line. */
	let cursor = -1;
	let draft = '';
	let pane: Pane;
	let body: HTMLDivElement | undefined;
	/** The scrollback's content width, in whole pixels, once measured. */
	let width: number | null = null;

	// The prompt's clock ticks per second, and only while someone could be
	// looking at it: a backgrounded tab that keeps a timer alive is a tab
	// that keeps a phone awake for a label nobody can read. Coming back
	// visible re-reads the time first, so the clock is never a second late
	// for the length of a tick.
	onMount(() => {
		let timer: ReturnType<typeof setInterval> | undefined;
		const readClock = () => (now = new Date());
		const start = () => {
			stop();
			timer = setInterval(readClock, 1000);
		};
		const stop = () => {
			if (timer !== undefined) clearInterval(timer);
			timer = undefined;
		};
		const visibility = () => {
			if (document.hidden) return stop();
			readClock();
			start();
		};
		start();
		document.addEventListener('visibilitychange', visibility);
		// A reload is the one moment a debounced write would lose: flush.
		window.addEventListener('pagehide', persist);
		return () => {
			stop();
			document.removeEventListener('visibilitychange', visibility);
			window.removeEventListener('pagehide', persist);
		};
	});

	// Restore before the first paint, so the scrollback is simply there
	// rather than appearing a frame later, and pin the view to its end the
	// way a run does.
	onMount(async () => {
		restore();
		await tick();
		pane.scrollToBottom();
	});

	/** Read back what a previous session left, or start empty. */
	function restore(): void {
		try {
			const raw = localStorage.getItem(STORE_KEY);
			if (!raw) return;
			const saved = JSON.parse(raw);
			if (!Array.isArray(saved?.scrollback) || !Array.isArray(saved?.history)) return;
			scrollback = saved.scrollback.filter(
				(entry: Command) =>
					typeof entry?.line === 'string' &&
					typeof entry?.ranAt === 'number' &&
					typeof entry?.result?.code === 'number'
			);
			history = saved.history.filter((line: string) => typeof line === 'string');
		} catch {
			// A private window, a store the browser blocks, a shape from a
			// build that is gone: none of them are worth an error in front of
			// someone trying to type a command. The session works, it just
			// starts empty.
		}
	}

	let pending: ReturnType<typeof setTimeout> | undefined;

	/** Queue a write; commands arrive in bursts and the store is not fast. */
	function save(): void {
		clearTimeout(pending);
		pending = setTimeout(persist, 100);
	}

	function persist(): void {
		clearTimeout(pending);
		let keep = scrollback.slice(-KEEP);
		for (;;) {
			try {
				localStorage.setItem(
					STORE_KEY,
					JSON.stringify({ scrollback: keep, history: history.slice(-KEEP) })
				);
				return;
			} catch {
				// Out of quota — a drawn view's stdout is a whole SVG — or a
				// store that refuses every write. Shed the oldest half and try
				// again; when there is nothing left to shed it was the store.
				if (keep.length === 0) return;
				keep = keep.slice(Math.ceil(keep.length / 2));
			}
		}
	}

	// What a drawn command has to be told (`fitArgv`): the content box, not
	// the element's own width — the body is padded, and an image a fraction
	// wider than the box it lands in is an image the browser scales back
	// down, which is the bug (da-wexl). Only new output is drawn to the new
	// size; scrollback keeps the size it was drawn at, the way a terminal's
	// does.
	onMount(() => {
		if (!body) return;
		const observer = new ResizeObserver(([entry]) => {
			width = Math.floor(entry.contentRect.width);
		});
		observer.observe(body);
		return () => observer.disconnect();
	});

	async function submit(): Promise<void> {
		const line = input;
		if (!line.trim()) return;
		const ranAt = Date.now();

		// Consume the line the way a shell does — it lives in the
		// scrollback and in the history now, and Up brings it back.
		input = '';
		draft = '';
		cursor = -1;
		remember(line);
		// Split here rather than through `runLine`, because the argv is what
		// gets fitted to the box. The echo above is the line as typed.
		await record(line, await run(fitArgv(shellSplit(line), width)), ranAt);
	}

	/**
	 * Run an argv from outside the prompt — a widget press (da-nz29).
	 *
	 * The scrollback echoes `result.argv`, the array the runner actually
	 * received, so a press teaches exactly the command it ran. A prettier
	 * string rebuilt from the press could drift from it; this cannot.
	 */
	export async function runArgv(argv: string[]): Promise<RunResult> {
		const ranAt = Date.now();
		const result = await run(argv);
		const line = result.argv.join(' ');
		remember(line);
		await record(line, result, ranAt);
		return result;
	}

	function remember(line: string): void {
		if (history[history.length - 1] !== line) history = [...history, line];
	}

	async function record(line: string, result: RunResult, ranAt: number): Promise<void> {
		scrollback = [...scrollback, { line, ranAt, result }];
		save();
		await tick();
		pane.scrollToBottom();
		dispatch('ran', result);
	}

	function recall(event: KeyboardEvent): void {
		if (event.key === 'ArrowUp') {
			const next = cursor === -1 ? history.length - 1 : cursor - 1;
			if (next < 0) return;
			if (cursor === -1) draft = input;
			event.preventDefault();
			cursor = next;
			input = history[cursor];
		} else if (event.key === 'ArrowDown') {
			if (cursor === -1) return;
			event.preventDefault();
			cursor += 1;
			if (cursor >= history.length) {
				cursor = -1;
				input = draft;
			} else {
				input = history[cursor];
			}
		}
	}
</script>

<Pane title="repl" alarm={isDegraded($storage)} bind:this={pane} bind:body>
	{#if scrollback.length === 0}
		<p class="quiet">type a command — <code>switch mood happy</code>, <code>tail</code></p>
	{/if}
	{#each scrollback as entry, i (i)}
		{#if i > 0}
			<hr class="rule" />
		{/if}
		<div class="head">
			<span class="stamp">{stampLabel(new Date(entry.ranAt), now)}</span>
			<!-- The stamp is a sibling of `.echo`, not a child of it: the
			     echo's text is the command and nothing else, which is what
			     every reader of it — a test, a copy-paste — already assumes. -->
			<div class="echo"><span class="prompt">❯</span> {entry.line}</div>
		</div>
		{#if entry.result.stdout}
			<CliOutput text={entry.result.stdout} />
		{/if}
		{#if entry.result.stderr}
			<CliOutput text={entry.result.stderr} error={entry.result.code !== 0} />
		{/if}
	{/each}

	<form slot="footer" class="line" on:submit|preventDefault={submit}>
		<!-- Live, not restored: the scrollback's stamps are history, this one
		     is the clock. On a narrow screen the seconds go and the minutes
		     stay — the input's width is not the clock's to take. -->
		<span class="stamp"
			>{clockSecondsLabel(now).slice(0, 5)}<span class="seconds"
				>{clockSecondsLabel(now).slice(5)}</span
			></span
		>
		<span class="prompt">❯</span>
		<input
			type="text"
			bind:value={input}
			on:keydown={recall}
			{disabled}
			placeholder={disabled ? 'opening database…' : 'occur coffee cups=2'}
			spellcheck="false"
			autocapitalize="none"
			autocomplete="off"
		/>
	</form>
</Pane>

<style>
	.quiet {
		margin: 0;
		color: #444466;
		font-style: italic;
	}

	.quiet code {
		font-style: normal;
		color: #7c7cff;
	}

	.head {
		display: flex;
		align-items: baseline;
		gap: 0.5rem;
	}

	.echo {
		flex: 1;
		min-width: 0;
		white-space: pre-wrap;
		color: #8888aa;
	}

	/* Dimmer than the echoed command and much dimmer than the data under
	   it: this is when you typed, and the entry's own occur_time — which
	   the renderer prints two lines down — is what actually matters. */
	.stamp {
		flex: none;
		color: #444466;
		font-variant-numeric: tabular-nums;
	}

	/* One dim hairline between commands, and the air either side of it. */
	.rule {
		margin: 0.5rem 0;
		border: none;
		border-top: 1px solid #1e1e38;
	}

	.prompt {
		color: #7c7cff;
	}

	.line {
		display: flex;
		align-items: center;
		gap: 0.5rem;
		border-top: 1px solid #1e1e38;
		padding: 0.45rem 0.6rem;
		font-family: ui-monospace, SFMono-Regular, Menlo, Consolas, monospace;
	}

	input {
		flex: 1;
		min-width: 0;
		background: transparent;
		border: none;
		outline: none;
		color: #e0e0ff;
		font: inherit;
		font-size: 0.95rem;
	}

	@media (max-width: 480px) {
		.line .stamp .seconds {
			display: none;
		}
	}

	input::placeholder {
		color: #444466;
	}

	input:disabled {
		opacity: 0.5;
	}
</style>
