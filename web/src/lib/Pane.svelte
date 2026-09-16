<!--
	One pane of the day frame: a titled box whose body scrolls on its own.

	The frame every region sits in (da-1x72) — dayview, nowview, tail and
	the REPL each get one. The pane owns the frame and the scroll; its
	default slot owns the content. Scrolling is per-pane and goes both
	ways: CLI tables are wide, and a pane must be able to scroll sideways
	without dragging the whole page with it.

	Inside the layout engine (da-jdop) the frame is drawn by
	`layout/PaneFrame.svelte` — the title bar, the tabs, the `…` menu — and
	this component is asked for its body only: it finds the
	frame's context and renders as `display: contents`, so the body scrolls
	inside the frame's content box exactly as it scrolled inside its own.
	The alarm goes up to the frame the same way. A widget that draws itself
	through `Pane` (the REPL, a CLI pane, the day strip) needs no change to
	live in a pane of the dashboard.
-->
<script lang="ts">
	import { getContext } from 'svelte';
	import type { Writable } from 'svelte/store';

	export let title: string;
	/**
	 * Draw this pane as unsafe to type into. Used when the database is in
	 * memory: the frame is the thing someone is looking at while they lose
	 * data, so it is the thing that has to say so.
	 */
	export let alarm = false;

	/**
	 * The scrolling body box, bindable.
	 *
	 * A pane whose content is drawn rather than written has to be told how
	 * many pixels it has (da-m0cj), and this element's content box is that
	 * number — the padded box the content actually lands in, which is not
	 * what the pane's own border box says.
	 */
	export let body: HTMLDivElement | undefined = undefined;

	/**
	 * Whether the title slot is the widget's OWN — the custom pane's ✎ and
	 * its editor — rather than merely the pane's name.
	 *
	 * Only a widget's own controls are moved into the frame (da-ndjo). A
	 * name the frame draws itself, moved in, costs more than it says: it
	 * overrides a renamed tab, silences the modified dot, and on a tabbed
	 * bar it is the same word twice beside the tab already carrying it.
	 * `CliPane` forwards whatever slot ITS caller gave it, so it passes
	 * this.
	 */
	export let ownTitle = true;

	/** Pin the view to the newest content. The REPL calls this after a run. */
	export function scrollToBottom(): void {
		if (body) body.scrollTop = body.scrollHeight;
	}

	/** The layout engine's frame around this pane, when there is one. */
	const frame = getContext<Frame | undefined>('layout-frame');
	$: frame?.alarm.set(alarm);

	/** What `layout/PaneFrame.svelte` puts in the context. */
	interface Frame {
		alarm: Writable<boolean>;
		/** The frame's h2, once it exists; the title slot is moved into it. */
		titleHost: Writable<HTMLElement | null>;
		/** Told when something has been moved in, so the frame hides its own. */
		hosted: Writable<boolean>;
	}

	function portal(node: HTMLElement, frame: Frame) {
		frame.hosted.set(true);
		// Hidden until there is a host: a frame draws the box for these
		// controls once it knows they exist (da-ndjo), and until then they
		// would show as a stray line at the top of the content.
		const unsubscribe = frame.titleHost.subscribe((host) => {
			node.style.display = host ? '' : 'none';
			if (host && node.parentElement !== host) host.appendChild(node);
		});
		return {
			destroy() {
				unsubscribe();
				node.remove();
				frame.hosted.set(false);
			}
		};
	}
</script>

<!--
	Inside a frame the title slot cannot be rendered here — the frame draws
	the bar — so it is rendered and then MOVED into the frame's h2 (the
	`portal` action), where the custom pane's ✎ and its editor land in the
	title bar of a dashboard pane exactly as they did in the grid's.
-->
<svelte:element
	this={frame ? 'div' : 'section'}
	class:pane={!frame}
	class:framed={!!frame}
	class:alarm={!frame && alarm}
>
	{#if !frame}
		<h2><slot name="title">{title}</slot></h2>
	{:else if $$slots.title && ownTitle}
		<span class="hosted-title" use:portal={frame}><slot name="title" /></span>
	{/if}
	<div class="body" bind:this={body}><slot /></div>
</svelte:element>

<style>
	.framed {
		display: contents;
	}

	.hosted-title {
		display: contents;
	}

	/* Orange rather than red: this is "your data is not safe yet", which is a
	   warning about the future, not a report of a failure that already
	   happened. It has to be visible at a glance from across a desk. */
	.pane.alarm {
		border-color: var(--alarm);
		box-shadow: 0 0 0 1px var(--alarm) inset;
	}
	.pane.alarm h2 {
		background: var(--alarm);
		/* Not a token: the almost-black the alarm's orange is legible on,
		   and nothing else in the app is painted on orange. */
		color: #1a1000;
	}

	.pane {
		display: flex;
		flex-direction: column;
		min-width: 0;
		min-height: 0;
		background: var(--ground);
		border: 1px solid var(--rule);
		border-radius: 6px;
	}

	h2 {
		margin: 0;
		padding: 0.35rem 0.6rem;
		border-bottom: 1px solid var(--tint);
		font-size: 0.75rem;
		font-weight: normal;
		letter-spacing: 0.06em;
		color: var(--accent);
	}

	.body {
		flex: 1;
		min-height: 0;
		overflow: auto;
		/* A third off 0.5rem 0.6rem (vividn: "reduce margins some", da-jgrn). */
		padding: 0.33rem 0.4rem;
		font-family: ui-monospace, SFMono-Regular, Menlo, Consolas, monospace;
		font-size: 0.85rem;
		line-height: 1.35;
		color: var(--ink);
	}
</style>
