<!--
	The conflicts widget (da-6cutb), a place: every entry `datum conflicts
	--json` lists — the engine's `conflicts`, read on every beat, which is
	what the commands widget's red button counts — and, per entry, the one
	`datum conflicts resolve` line that settles it.

	vividn, 2026-10-08: «yes there should be a conflicts widget that get opened
	by the conflicts command».

	A key's sides stand side by side, lettered as the CLI letters them
	(`letterOf`: the column's order, every copy's), each with its device and
	time. A key's PICK is the CLI's own `KEY=`: a tap on a side writes its
	letter into the key's box, and anything else typed there is a value — so
	the box says what the line will. The keys not picked follow the strategy
	chosen, and «keep as shown» — no flag, the default — leaves them as the
	entry shows them: a delete stays deleted, an unset key unset.

	«Resolve» runs the line through `press`, so it is echoed and answered in
	the bar as any command is; the write's beat then takes the entry off the
	list and a count off the button. A refusal is the CLI's own words under
	the entry it refused (`--merge` on a delete): nothing here decides ahead
	of the CLI what it will take.
-->
<script lang="ts">
	import Pane from '$lib/Pane.svelte';
	import { act } from '$lib/act.js';
	import { stripAnsi } from '$lib/ansi.js';
	import { letterOf, resolveArgv, said, STRATEGIES, when, type Strategy } from '$lib/conflicts.js';
	import { shellJoin } from '$lib/shellSplit.js';
	import { conflicts, press, ready } from './engine.js';

	interface Choice {
		/** What the keys not picked follow; none keeps what the entry shows. */
		how: Strategy | '';
		/** Each key's pick, as the CLI takes it: a side's letter, or a value. */
		picks: Record<string, string>;
	}

	const UNCHOSEN: Choice = { how: '', picks: {} };
	/** «keep as shown» first: it is the default. */
	const HOWS: Choice['how'][] = ['', ...STRATEGIES];

	/** What is chosen for each entry, and what the CLI said to its last resolve, by hid. */
	let chosen: Record<string, Choice> = $state({});
	let refused: Record<string, string> = $state({});
	let busy = $state(false);

	const choose = (hid: string, change: Partial<Choice>) =>
		(chosen = { ...chosen, [hid]: { ...(chosen[hid] ?? UNCHOSEN), ...change } });

	const pick = (hid: string, key: string, to: string) =>
		choose(hid, { picks: { ...(chosen[hid] ?? UNCHOSEN).picks, [key]: to } });

	async function resolve(hid: string, argv: string[]): Promise<void> {
		busy = true;
		const result = await press(argv);
		busy = false;
		refused = { ...refused, [hid]: result.code === 0 ? '' : stripAnsi(result.stderr).trim() };
	}
</script>

<Pane title="conflicts">
	{#if $conflicts.length === 0}
		<p class="quiet">No sync conflicts.</p>
	{/if}
	{#each $conflicts as entry (entry.hid)}
		{@const choice = chosen[entry.hid] ?? UNCHOSEN}
		{@const argv = resolveArgv(entry.hid, choice.how, choice.picks)}
		<section data-hid={entry.hid}>
			<h3>{entry.slug}{entry.deleted ? ' (deleted)' : ''}</h3>
			{#each Object.entries(entry.conflicts) as [key, sides] (key)}
				{@const picked = (choice.picks[key] ?? '').toUpperCase()}
				<div class="key" data-key={key}>
					<span class="name">{key}</span>
					{#each sides as side, i}
						{@const letter = letterOf(i)}
						<button
							type="button"
							class="side"
							class:on={picked === letter}
							aria-pressed={picked === letter}
							data-side={letter}
							onclick={() => pick(entry.hid, key, picked === letter ? '' : letter.toLowerCase())}
						>
							<b>{letter}</b>
							<span class="value">{said(key, side.value)}</span>
							<small>{side.device} · {when(side.hlc)}</small>
						</button>
					{/each}
					<input
						aria-label={`${key}: a side's letter, or a value`}
						placeholder="a, b or a value"
						value={choice.picks[key] ?? ''}
						oninput={(e) => pick(entry.hid, key, e.currentTarget.value)}
					/>
				</div>
			{/each}
			<div class="how" role="radiogroup" aria-label="the keys not picked">
				{#each HOWS as how (how)}
					<button
						type="button"
						role="radio"
						class:on={choice.how === how}
						aria-checked={choice.how === how}
						data-how={how}
						onclick={() => choose(entry.hid, { how })}>{how ? `--${how}` : 'keep as shown'}</button
					>
				{/each}
			</div>
			<div class="run">
				<button type="button" class="command" use:act={'run'} disabled={busy || !$ready} onclick={() => resolve(entry.hid, argv)}>Resolve</button>
				<code>{shellJoin(argv)}</code>
			</div>
			{#if refused[entry.hid]}<p class="refused">{refused[entry.hid]}</p>{/if}
		</section>
	{/each}
</Pane>

<style>
	section {
		display: flex;
		flex-direction: column;
		gap: 0.4rem;
		padding: 0.5rem 0;
	}
	section + section {
		border-top: 1px solid var(--rule);
	}
	h3 {
		margin: 0;
		font-size: 0.85rem;
		font-weight: normal;
		color: var(--ink-bright);
		overflow-wrap: anywhere;
	}
	.key,
	.how,
	.run {
		display: flex;
		flex-wrap: wrap;
		align-items: center;
		gap: 0.3rem;
	}
	.name {
		color: var(--muted);
	}
	/* The chosen one solid and bold, the others a tint (web-conventions §29). */
	.side,
	.how button {
		min-block-size: 24px;
		background: var(--tint);
		border: 1px solid var(--rule);
		border-radius: 4px;
		padding: 0.2rem 0.5rem;
		color: var(--ink);
		font: inherit;
		font-size: 0.85rem;
		text-align: start;
		cursor: pointer;
	}
	.side.on,
	.how button.on {
		background: var(--accent);
		border-color: var(--accent);
		color: var(--page);
		font-weight: bold;
	}
	.value {
		overflow-wrap: anywhere;
	}
	small {
		display: block;
		opacity: 0.75;
		font-weight: normal;
	}
	input {
		box-sizing: border-box;
		min-block-size: 24px;
		inline-size: 9rem;
		background: var(--page);
		border: 1px solid var(--rule);
		border-radius: 4px;
		padding: 0.2rem 0.4rem;
		color: var(--ink-bright);
		font: inherit;
		font-size: 0.85rem;
	}
	code {
		color: var(--muted);
		overflow-wrap: anywhere;
	}
	.quiet {
		margin: 0;
		color: var(--quiet);
		font-style: italic;
	}
	.refused {
		margin: 0;
		color: var(--error);
	}
</style>
