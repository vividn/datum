<!--
	One pane of the shell's pane row: a titled, independently scrolling box.

	The shell primitive the CLI-render panes sit in (da-1x72) — dayview,
	nowview and tail each get one. The pane owns the frame and the scroll;
	its slot owns the rendered CLI output.
-->
<script lang="ts">
	export let title: string;
</script>

<section class="pane">
	<h2>{title}</h2>
	<div class="body"><slot /></div>
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
		font-size: 0.85rem;
		color: #c8c8e8;
	}
</style>
