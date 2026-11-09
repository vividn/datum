<!--
	The welcome widget (da-u7vz8): what a first visit opens on. vividn,
	2026-10-08: «a welcome widget as first tab that explains what datum is,
	some basic ideas to be aware of, and a basic user flow».

	Since da-uzyih.19 it is short (WelcomeText.svelte): the name, its
	tagline, the tutorial, the line for an AI agent, the docs and the
	source, on a phone's screen without scrolling. What it taught (fields,
	shapes, the input bar, tabs and widgets) is the tutorial's first tabs.

	It closes as any tab does: its «Close welcome» button went (da-6tk9t,
	vividn, 2026-11-09: «let's remove the "close welcome" button on the
	welcome tab»).
-->
<script lang="ts">
	import Pane from '#lib/Pane.svelte';
	import WelcomeText from './WelcomeText.svelte';
</script>

<Pane title="welcome">
	<WelcomeText />
</Pane>
