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

	interface Props {
		title: string;
		/**
		 * Draw this pane as unsafe to type into. Used when the database is in
		 * memory: the frame is the thing someone is looking at while they lose
		 * data, so it is the thing that has to say so.
		 */
		alarm?: boolean;

		/**
		 * The scrolling body box, bindable.
		 *
		 * A pane whose content is drawn rather than written has to be told how
		 * many pixels it has (da-m0cj), and this element's content box is that
		 * number — the padded box the content actually lands in, which is not
		 * what the pane's own border box says.
		 */
		body?: HTMLDivElement;

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
		ownTitle?: boolean;

		/**
		 * Draw the BODY alone: no title strip, no outline.
		 *
		 * For a surface that is not a pane of the dashboard and must not read as
		 * one. The bar's log is the case (da-ckw3): it shows the REPL under the
		 * command line, and vividn, 2026-09-21: "Remove the 'repl' header line."
		 * A titled box under a reader's own prompt says "this is a widget"; what
		 * it has to say is "this is the rest of what you just saw".
		 *
		 * Separate from `frame`, which also draws no title here — that one hands
		 * the title UP to a frame that draws it. This one has no title at all.
		 */
		bare?: boolean;

		/**
		 * The body's type, as a scale on the pane's own 0.85rem (da-hkjp).
		 *
		 * A pane's type size is not its widget's to write down in CSS: every
		 * body shares this one rule, so a widget that wants smaller type has to
		 * ask for it from OUT here. The tail asks (`type` in registry.ts, a
		 * declared option of the kind, so `da-vhwq` can reach it by name); one
		 * is every other pane, unchanged.
		 */
		scale?: number;
	}

	let { title, alarm = false, body = $bindable(), ownTitle = true, bare = false, scale = 1 }: Props = $props();

	/** Pin the view to the newest content. The REPL calls this after a run. */
	export function scrollToBottom(): void {
		if (body) body.scrollTop = body.scrollHeight;
	}

	/** The layout engine's frame around this pane, when there is one. */
	const frame = getContext<Frame | undefined>('layout-frame');
	$effect.pre(() => {
		frame?.alarm.set(alarm);
	});

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
	The slots stay `<slot>`s until the callers pass snippets (the sweep,
	da-ul23i): a legacy caller's named slot never reaches a `{@render}`, and
	the `title` slot shares its name with the `title` prop, so its snippet
	needs a new name, which its callers (CliPane, CustomPane) must use.

	Inside a frame the title slot cannot be rendered here — the frame draws
	the bar — so it is rendered and then MOVED into the frame's h2 (the
	`portal` action), where the custom pane's ✎ and its editor land in the
	title bar of a dashboard pane exactly as they did in the grid's.
-->
<svelte:element
	this={frame ? 'div' : 'section'}
	class:pane={!frame}
	data-title={frame ? undefined : title}
	class:bare={!frame && bare}
	class:framed={!!frame}
	class:alarm={!frame && !bare && alarm}
>
	{#if !frame && !bare}
		<!-- svelte-ignore slot_element_deprecated -->
		<h2><slot name="title">{title}</slot></h2>
	{:else if frame && $$slots.title && ownTitle}
		<!-- svelte-ignore slot_element_deprecated -->
		<span class="hosted-title" use:portal={frame}><slot name="title" /></span>
	{/if}
	<!-- svelte-ignore slot_element_deprecated -->
	<div class="body" bind:this={body} style:--type-scale={scale === 1 ? null : scale}><slot /></div>
</svelte:element>

<style>
	.framed {
		display: contents;
	}

	/* No outline and no strip: the body is the whole of it. */
	.pane.bare {
		border: none;
		border-radius: 0;
		background: none;
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
		font-size: calc(0.85rem * var(--type-scale, 1));
		line-height: 1.35;
		color: var(--ink);
	}
</style>
