<!--
	The premade widget (da-36whg): fields ready to set up, by theme. vividn,
	2026-11-05: «let's move the examples out of the setup pane, and instead
	they should get their own widget/layout to the left of setup … They will
	be organized by theme rather than data kind», and «examples should be
	it's completely own layout. So a tab in between welcome and setup».

	Each is `EXAMPLES` (#lib/track.ts): its icon in its color, its name, its
	kind's glyph and what it shows. A tap opens the setup widget with its
	form filled (`openField`'s `example`), nothing written until it is saved
	there; weight's tap is one of its choices, lbs or kg, which name its key.
	One whose field exists already says «set up», and a tap opens setup on
	that field. Home & chores ends with the chores kit, for chores on a
	schedule: its button shows what the kit brings and installs it on a yes,
	as `kit install` at the prompt does (#lib/dbio.ts).
-->
<script lang="ts">
	import Pane from '#lib/Pane.svelte';
	import { act } from '#lib/act.ts';
	import { confirm } from '#lib/confirm.ts';
	import { openField } from '#lib/fieldWidget.ts';
	import { run } from '#lib/run.ts';
	import { chosen, THEMES, VERBS, type Example } from '#lib/track.ts';
	import { fields, press } from './engine.js';

	const taken = $derived(new Set($fields.map((f) => f.field)));
	const glyphOf = (e: Example) => VERBS.find((v) => v.verb === e.verb)?.glyph;

	/** Setup on `example`, filled with it (and `choice`'s keys), or on its field where it exists. */
	function open(example: Example, choice?: string): void {
		void (taken.has(example.field)
			? openField(example.field)
			: openField(example.field, example.verb, { example: chosen(example, choice) }));
	}

	/** The chores kit, installed once its dry run is seen and agreed to. */
	async function chores(): Promise<void> {
		const toml = (await import('../../../../kits/chores.toml?raw')).default;
		const argv = ['kit', 'install', '-'];
		const seen = await run([...argv, '--dry-run'], toml);
		if (seen.code !== 0) return void press([...argv, '--dry-run'], undefined, toml);
		const text = `${seen.stdout.trim()}\n\nA kit can add SQL views and commands to your database; install only kits you trust.`;
		if (await confirm(text, 'install')) await press([...argv, '--yes'], undefined, toml);
	}
</script>

<Pane title="premade">
	<div class="premade">
		<p class="what">Fields ready to set up. A tap opens setup with one filled in; nothing is saved until you save it there.</p>
		{#each THEMES as { theme, examples } (theme)}
			<section data-theme={theme}>
				<h3>{theme}</h3>
				<div class="examples">
					{#each examples as example (example.field)}
						{@const set = taken.has(example.field)}
						{@const choices = set ? [] : Object.keys(example.choices ?? {})}
						<div class="example" data-example={example.field} data-taken={set || undefined}>
							<button type="button" class="card" use:act={'open'} disabled={choices.length > 0} onclick={() => open(example)}>
								<span class="icon" style:color={example.color}>{example.glyph}</span>
								<span class="name">{example.field} <span class="kind">{glyphOf(example)}</span></span>
								<span class="what">{set ? 'set up' : example.shows}</span>
							</button>
							{#if choices.length}
								<div class="choices" role="group" aria-label="{example.field} in">
									{#each choices as choice (choice)}
										<button type="button" use:act={'open'} data-choice={choice} onclick={() => open(example, choice)}>{choice}</button>
									{/each}
								</div>
							{/if}
						</div>
					{/each}
				</div>
				{#if theme === 'home & chores'}
					<p class="kit">
						For chores on a schedule, each due again some days after it is done, the chores kit adds
						<code>newchore</code> and <code>chorelist</code>.
						<button type="button" class="install" use:act={'run'} onclick={chores}>Install the chores kit</button>
					</p>
				{/if}
			</section>
		{/each}
	</div>
</Pane>

<style>
	.premade {
		display: flex;
		flex-direction: column;
		gap: 0.75rem;
	}

	.what,
	.kit {
		margin: 0;
		color: var(--quiet);
	}

	h3 {
		margin: 0 0 0.35rem;
		font-size: 0.8rem;
		font-weight: normal;
		letter-spacing: 0.06em;
		text-transform: uppercase;
		color: var(--accent);
	}

	.examples {
		display: grid;
		grid-template-columns: repeat(auto-fill, minmax(12rem, 1fr));
		gap: 0.4rem;
	}

	.example {
		display: flex;
		background: var(--raised);
		border: 1px solid var(--rule);
		border-radius: 6px;
	}

	.example:hover,
	.example:focus-within {
		border-color: var(--accent);
	}

	/* The icon, then the name over what it shows. */
	.card {
		flex: 1;
		display: grid;
		grid-template-columns: 2rem 1fr;
		align-items: center;
		column-gap: 0.5rem;
		background: none;
		border: 0;
		padding: 0.35rem 0.5rem;
		color: var(--ink-bright);
		font: inherit;
		text-align: start;
		cursor: pointer;
	}

	.card:disabled {
		cursor: default;
	}

	.icon {
		grid-row: span 2;
		font-size: 1.5rem;
		text-align: center;
	}

	.kind {
		color: var(--muted);
	}

	.card .what {
		font-size: 0.8rem;
	}

	[data-taken] .card .what {
		color: var(--accent);
	}

	.choices {
		display: flex;
		flex-direction: column;
	}

	.choices button,
	.install {
		flex: 1;
		min-block-size: 28px;
		background: var(--raised);
		border: 0;
		border-inline-start: 1px solid var(--rule);
		padding-inline: 0.6rem;
		color: var(--ink);
		font: inherit;
		cursor: pointer;
	}

	.choices button + button {
		border-block-start: 1px solid var(--rule);
	}

	.install {
		display: block;
		margin-block-start: 0.35rem;
		border: 1px solid var(--rule);
		border-radius: 4px;
	}
</style>
