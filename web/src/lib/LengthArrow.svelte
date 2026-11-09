<!--
	The arrow a LENGTH pill wears before its `25m` (da-g2ruk): the length
	runs BACK from its time, so it points left. vividn, 11-03: «let's also
	draw an arrow for the duration so <-12m», then «a bit more pronounced
	and vertically centered». Drawn, not the `←` glyph, which sits low in
	our fonts; the flex row it stands in centres it on the text. A length
	`+5m`, forward from its time, has it point right (da-vvo03).
-->
<script lang="ts">
	let { ahead = false }: { ahead?: boolean } = $props();
</script>

<svg class="length-arrow" class:ahead viewBox="0 0 14 10" aria-hidden="true" focusable="false"
	><path d="M13 5H2M6 1 2 5l4 4" /></svg
>

<style>
	.length-arrow {
		display: block;
		flex: none;
		width: 1.2em;
		height: 0.85em;
	}

	.length-arrow.ahead {
		transform: scaleX(-1);
	}

	.length-arrow path {
		fill: none;
		stroke: currentColor;
		stroke-width: 2.2;
		stroke-linecap: round;
		stroke-linejoin: round;
	}
</style>
