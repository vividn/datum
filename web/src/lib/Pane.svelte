<!--
	One pane of the day frame: a titled box whose body scrolls on its own.

	The frame every region sits in (da-1x72) — dayview, nowview, tail and
	the REPL each get one. The pane owns the frame and the scroll; its
	default slot owns the content. Scrolling is per-pane and goes both
	ways: CLI tables are wide, and a pane must be able to scroll sideways
	without dragging the whole page with it.

	The `footer` slot stays put below the scroll — that is where the REPL
	hangs its prompt line so output can pile up above it.
-->
<script lang="ts">
	export let title: string;

	let body: HTMLDivElement;

	/** Pin the view to the newest content. The REPL calls this after a run. */
	export function scrollToBottom(): void {
		if (body) body.scrollTop = body.scrollHeight;
	}
</script>

<section class="pane">
	<h2>{title}</h2>
	<div class="body" bind:this={body}><slot /></div>
	<slot name="footer" />
</section>

<style>
	.pane {
		display: flex;
		flex-direction: column;
		min-width: 0;
		min-height: 0;
		background: #0d0d1a;
		border: 1px solid #2a2a4a;
		border-radius: 6px;
	}

	h2 {
		margin: 0;
		padding: 0.35rem 0.6rem;
		border-bottom: 1px solid #1e1e38;
		font-size: 0.75rem;
		font-weight: normal;
		letter-spacing: 0.06em;
		color: #7c7cff;
	}

	.body {
		flex: 1;
		min-height: 0;
		overflow: auto;
		padding: 0.5rem 0.6rem;
		font-family: ui-monospace, SFMono-Regular, Menlo, Consolas, monospace;
		font-size: 0.85rem;
		line-height: 1.35;
		color: #c8c8e8;
	}
</style>
