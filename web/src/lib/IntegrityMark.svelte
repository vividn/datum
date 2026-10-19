<!--
	The warning datum puts by a day's date label when `check integrity`
	finds something in it (da-0lful), for the strip's own date stamps: the
	CLI's icon, drawn the same, naming the fields it flags.
-->
<script lang="ts">
	export let fields: string[];
	$: label = `integrity: ${fields.join(', ')}`;
</script>

<svg class="integrity-mark" viewBox="0 0 24 24" role="img" aria-label={label}>
	<title>{label}</title>
	<path
		d="M10.25,4.19,2.63,18a2,2,0,0,0,1.75,3H19.62a2,2,0,0,0,1.75-3L13.75,4.19A2,2,0,0,0,10.25,4.19Z"
		fill="#ffff00"
		stroke="#ff0000"
		stroke-width="1.5"
	/>
	<line x1="12" y1="9" x2="12" y2="13" stroke="#ff0000" stroke-width="2" stroke-linecap="round" />
	<circle cx="12" cy="17" r="1" fill="#ff0000" />
</svg>

<style>
	/* In the stamp's corner, clear of its right-aligned lines: the
	   weekday on top is the shortest of them. */
	.integrity-mark {
		position: absolute;
		top: 2px;
		left: 2px;
		width: 1em;
		height: 1em;
	}
</style>
