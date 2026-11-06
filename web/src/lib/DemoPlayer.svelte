<!--
	A widget's demo, played (da-2fxiz, #lib/demo.ts): the kind's own component
	from `KINDS`, on its sample database held in memory, its script done to it
	step by step with each finger drawn, then, after a rest, the page loads
	again, a fresh database, and plays it again. Where `prefers-reduced-motion`
	asks, it is held still at its start, and ▶ plays it once (`once`).

	`data-state` says how far it is: opening, playing, played, still, or failed
	with why in `data-failed`; `demos.test.mjs` reads it.
-->
<script lang="ts">
	import { onMount } from 'svelte';
	import { initDb } from '#lib/db.ts';
	import { perform, sleep } from '#lib/demo.ts';
	import { boot, epoch, onPress, ran } from '#lib/layout/engine.ts';
	import { KINDS } from '#lib/layout/registry.ts';
	import { cliError, run, settled } from '#lib/run.ts';
	import { shellSplit } from '#lib/shellSplit.ts';

	let { kind, name, once = false }: { kind: string; name: string; once?: boolean } = $props();

	/** Between steps, the time to see what one did; after the last, the rest before it plays again. */
	const PAUSE_MS = 900;
	const REST_MS = 2500;

	const widget = $derived(KINDS[kind]);
	const demo = $derived(widget?.demos?.[name]);
	const settings = $derived({ ...widget?.meta.defaultSettings, ...demo?.settings });

	let stage: 'opening' | 'playing' | 'played' | 'still' | 'failed' = $state('opening');
	let failed = $state('');
	let drawn = $state(false);
	let fingers: { x: number; y: number }[] = $state([]);
	let host: HTMLElement | undefined = $state();

	async function must(line: string): Promise<void> {
		const result = await run(shellSplit(line));
		if (result.code !== 0) throw cliError(result.stderr, `${line} exited ${result.code}`);
	}

	async function play(): Promise<void> {
		stage = 'playing';
		let refused = '';
		const off = onPress((pressed) => void pressed.then((r) => (refused ||= r.code === 0 ? '' : r.stderr || `${r.argv.join(' ')} exited ${r.code}`)));
		try {
			for (const step of demo!.script) {
				await sleep(PAUSE_MS);
				await perform(host!, step, (at) => (fingers = at ?? []));
				await settled();
				if (refused) throw cliError(refused);
			}
		} finally {
			off();
		}
		stage = 'played';
		if (once) return;
		await sleep(REST_MS);
		location.reload();
	}

	onMount(() => {
		// In the colors of the page it is shown in, not the demo database's (da-y992): drawn as the app is there.
		try {
			for (const token of ['--hue', '--sat', '--shift']) {
				const value = parent === window ? '' : parent.document.documentElement.style.getPropertyValue(token);
				if (value) document.documentElement.style.setProperty(token, value);
			}
		} catch {
			// Framed by another origin: its own.
		}
		void (async () => {
			if (!demo) throw new Error(widget ? `${kind} has no demo ${name}` : `no widget kind ${kind}`);
			await initDb(true);
			await boot();
			for (const line of demo.seed) await must(line);
			await ran();
			drawn = true;
			await settled();
			if (!once && matchMedia('(prefers-reduced-motion: reduce)').matches) stage = 'still';
			else await play();
		})().catch((e) => {
			failed = e instanceof Error ? e.message : String(e);
			stage = 'failed';
		});
	});
</script>

<div class="demo" data-kind={kind} data-demo={name} data-state={stage} data-failed={failed || undefined}>
	<div class="host" data-widget={kind} bind:this={host}>
		{#if drawn && widget}
			<widget.component {...widget.props ? widget.props(settings) : { settings }} epoch={$epoch} key="demo" onchange={() => {}} />
		{/if}
	</div>
	{#each fingers as at, i (i)}
		<span class="finger" style:left="{at.x}px" style:top="{at.y}px"></span>
	{/each}
	{#if stage === 'still'}
		<a class="play" data-sveltekit-reload href="?{new URLSearchParams({ db: 'demo', kind, demo: name, play: '1' })}">▶ play</a>
	{:else if stage === 'failed'}
		<p class="failed">{failed}</p>
	{/if}
</div>

<style>
	.demo {
		position: fixed;
		inset: 0;
		display: flex;
		background: var(--ground);
	}
	/* The widget's box, as a pane's content box is: it sizes itself from it. */
	.host {
		display: flex;
		flex-direction: column;
		flex: 1;
		min-width: 0;
		min-height: 0;
		container-type: size;
	}
	.finger {
		position: fixed;
		width: 28px;
		height: 28px;
		margin: -14px 0 0 -14px;
		border: 2px solid var(--ink-bright);
		border-radius: 50%;
		background: color-mix(in srgb, var(--ink-bright) 25%, transparent);
		pointer-events: none;
		z-index: 10;
	}
	.play {
		position: absolute;
		right: 0.4rem;
		bottom: 0.4rem;
		padding: 0.2rem 0.5rem;
		border: 1px solid var(--rule);
		border-radius: 4px;
		background: var(--raised);
		color: var(--accent-bright);
		text-decoration: none;
		z-index: 10;
	}
	.failed {
		position: absolute;
		inset: auto 0 0;
		margin: 0;
		padding: 0.3rem;
		background: var(--raised);
		color: var(--ink-bright);
		font-size: 0.8rem;
		z-index: 10;
	}
</style>
