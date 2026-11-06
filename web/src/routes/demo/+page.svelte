<!--
	A widget's demo, alone on a page (da-2fxiz, #lib/demo.ts): what a guide's
	frame shows. `?kind=<kind>&demo=<name>`, on `?db=demo`, held in memory; the
	root layout draws no shell around it. `play` plays it once, still or not.
-->
<script lang="ts">
	import { page } from '$app/state';
	import DemoPlayer from '#lib/DemoPlayer.svelte';

	const query = page.url.searchParams;
</script>

<DemoPlayer kind={query.get('kind') ?? ''} name={query.get('demo') ?? ''} once={query.has('play')} />
