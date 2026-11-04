<!--
	«Settings» (da-99g5q), a place: the gear's «Settings» opens it
	(`openPlace`), a modal on a desktop and a tab on a phone, and like any
	widget it can be added from the +. vividn, 2026-11-04: «Let's make a
	settings widget. Move the settings out of gear menu. Instead put a
	settings option in there that opens the settings tab or modal». The gear
	keeps its doors and its facts; a setting added later goes here, a line
	saying what it does under it.
-->
<script lang="ts">
	import { untrack } from 'svelte';
	import { browser } from '$app/env';
	import Pane from '#lib/Pane.svelte';
	import { act } from '#lib/act.ts';
	import { fieldArgv } from '#lib/calls.ts';
	import { run } from '#lib/run.ts';
	import { press } from './engine.js';
	import { wrapTabs } from './shell.js';

	let { epoch = 0 }: { epoch?: number } = $props();

	/**
	 * «Track timezone» (da-yw6o2). vividn, 2026-10-30: «you click on track
	 * timezone and it switches timezone to be the current timezone of the
	 * device. clicking it again IGNOREs the timezone». The zone the field
	 * holds is the CLI's answer, read again on every epoch.
	 */
	let zone: string | null = $state(null);
	const deviceZone = Intl.DateTimeFormat().resolvedOptions().timeZone;
	/** Reads started and the newest one answered: last started wins (see AliasesWidget.svelte). */
	let started = 0;
	let shown = -1;
	$effect.pre(() => {
		void epoch;
		if (browser) untrack(() => void readZone());
	});
	async function readZone() {
		const at = ++started;
		const { code, stdout } = await run(fieldArgv('timezone'));
		if (at < shown || code !== 0) return;
		shown = at;
		try {
			zone = JSON.parse(stdout)[0]?.zone || null;
		} catch {
			// Not the JSON asked for (a process killed mid-write): the button stays as it was.
		}
	}
</script>

<Pane title="settings">
	<section class="setting">
		{#if zone}
			<button type="button" class="command" data-setting="timezone" use:act={'run'} onclick={() => void press(['timezone', 'IGNORED'])}
				>Stop tracking timezone ({zone})</button
			>
		{:else}
			<button type="button" class="command" data-setting="timezone" use:act={'run'} onclick={() => void press(['timezone', deviceZone])}
				>Track timezone ({deviceZone})</button
			>
		{/if}
		<p>Logs this device's zone to the timezone field, so each entry is read in the zone you were in; pressed again, it logs IGNORED and stops.</p>
	</section>
	<section class="setting">
		<label><input type="checkbox" data-setting="wrap" bind:checked={$wrapTabs} /> Wrap tabs when swiping</label>
		<p>On this device, a swipe past the last tab goes to the first, and past the first to the last.</p>
	</section>
</Pane>

<style>
	.setting {
		margin: 0 0 1rem;
	}
	.setting p {
		margin: 0.3rem 0 0;
		color: var(--quiet);
		font-size: 0.85rem;
	}
</style>
