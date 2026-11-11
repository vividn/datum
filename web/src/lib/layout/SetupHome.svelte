<!--
	The setup widget with no field chosen (da-asgtm): under «manual setup»,
	the premade examples, «What do you want to track?», by theme. vividn,
	2026-11-09: «combine the premade and setup tabs back into one tab, which
	makes it easy for new people to get going, but also experienced users to
	add new fields, edit existing fields». It was the premade widget's
	catalog (da-36whg), whose examples are premade.toml's, compiled into the
	CLI and read once a session with `setup --premade --json` (da-rky8h).

	A row is an example's icon and name; a click opens its preview, the right
	half on a wide pane and a sheet on a narrow one: its kind and what it
	shows, its options in one row (another kind, weight's lbs or kg, and «show
	label» where it names a key to print by its dots), and «A day of it», its
	example day drawn by the CLI (`dayview F --sample`, da-8l6ol), or for a
	record the keys a row of it holds. Configure opens the setup form filled
	with the example as the options give it, nothing written until it is saved
	there; Start tracking sets it up as shown, its place included, and goes to
	the collection, the tour following a person's first field. A field that
	exists says so, and Configure opens it.

	Home & chores ends with the chores kit, for chores on a schedule: its
	button shows what the kit brings and installs it on a yes, as `kit
	install` at the prompt does, then opens the kit's chores tab, which the
	button opens from then on.
-->
<script module lang="ts">
	import { premadeArgv } from '#lib/calls.ts';
	import { run } from '#lib/run.ts';
	import type { Premade } from '#lib/track.ts';

	/** premade.toml, read once a session; a refusal is read again by the next mount. */
	let catalog: Promise<Premade | null> | undefined;
	function read(): Promise<Premade | null> {
		catalog ??= run(premadeArgv())
			.then((r) => (r.code === 0 ? (JSON.parse(r.stdout) as Premade) : null))
			.catch(() => null);
		return catalog.then((premade) => {
			if (!premade) catalog = undefined;
			return premade;
		});
	}
	/** The example the preview was on, for the way back from Configure: the session's, whatever the database, as the catalog is. */
	let kept: string | null = null;
</script>

<script lang="ts">
	import Sheet from '#lib/Sheet.svelte';
	import { act } from '#lib/act.ts';
	import { confirm } from '#lib/confirm.ts';
	import { openField } from '#lib/fieldWidget.ts';
	import { inlineSvg } from '#lib/svgSafe.ts';
	import { begin } from '#lib/tour.ts';
	import { bandArgv } from '#lib/calls.ts';
	import { chosen, specArgv, VERBS, type Example, type Verb } from '#lib/track.ts';
	import { fields, press } from './engine.js';
	import { open, taken as named } from './shell.js';
	import { refreshKits } from './store.js';

	let premade = $state<Premade | null>(null);
	let failed = $state(false);
	read().then((p) => (p ? (premade = p) : (failed = true)));
	const themes = $derived((premade?.themes ?? []).map((theme) => ({ theme, examples: premade!.examples.filter((e) => e.theme === theme) })));
	const taken = $derived(new Set($fields.map((f) => f.field)));

	/** Wide, the preview is the right half; narrow, a sheet. */
	let width = $state(0);
	const wide = $derived(width >= 720);
	let picked = $state(kept);
	/** The options: a choice (lbs or kg), the kind (0 the example's own, i its `as[i - 1]`), its labels shown. */
	let choice: string | undefined = $state();
	let variant = $state(0);
	let labelled = $state(true);
	const shown = $derived(premade?.examples.find((e) => e.field === picked) ?? (wide ? premade?.examples[0] : undefined));

	function pick(e: Example): void {
		[picked, kept] = [e.field, e.field];
		[choice, variant, labelled] = [undefined, 0, true];
	}
	/** The choice made, or the file's first, its default. */
	const choiceOf = (e: Example) => choice ?? Object.keys(e.choices ?? {})[0];

	const kind = (verb: Verb) => VERBS.find((v) => v.verb === verb)!;

	/** `e` as the options give it: its choice's keys, the kind chosen, the label keys where shown. */
	function shaped(e: Example): Example {
		const c = chosen(e, choiceOf(e));
		const v = variant ? e.as?.[variant - 1] : undefined;
		const as = v ? { ...c, kind: v.kind, shows: v.shows, day: v.day, spec: v.spec } : c;
		return labelled && c.labels ? { ...as, spec: { ...as.spec, labels: c.labels } } : as;
	}
	const now = $derived(shown && shaped(shown));

	/** The band's lane, in fractions of the day: `y` its top edge with a height, a dot's centre without. */
	const lane = $derived.by(() => {
		const { y, height } = (now?.spec ?? {}) as { y?: number; height?: number };
		if (y === undefined) return null;
		return height === undefined ? { top: y - 0.06, h: 0.12 } : { top: y, h: height };
	});

	/** The example day, drawn by the CLI at the band's width; only the latest asked is kept. */
	let band: HTMLElement | undefined = $state();
	let bandWidth = $state(0);
	let asked = 0;
	$effect(() => {
		const [e, w, at] = [now, Math.floor(bandWidth), ++asked];
		if (!e || e.kind === 'add' || !band || w <= 0) return;
		const argv = [...bandArgv(w, wide ? 140 : 110), e.field, '--sample', JSON.stringify({ ...e.spec, day: e.day })];
		void run(argv).then((r) => {
			if (at !== asked || !band) return;
			// A refused draw empties the band rather than leave another example's day under this one's name.
			const svg = r.code === 0 ? inlineSvg(r.stdout) : null;
			if (svg) band.replaceChildren(svg);
			else band.replaceChildren();
		});
	});

	/** The full setup form, filled as shown; a field that exists, its own. */
	function configure(e: Example): void {
		void (taken.has(e.field) ? openField(e.field) : openField(e.field, e.kind, { example: shaped(e) }));
	}

	/** Set up as shown and tracked, then the collection; the tour after a person's first field. */
	async function start(e: Example): Promise<void> {
		const first = $fields.every((f) => f.ignored);
		const result = await press([...specArgv(shaped(e).spec), '--track']);
		if (result.code !== 0) return;
		// The sheet goes with it: setup stays mounted behind collect (da-ebz6k).
		if (!wide) picked = kept = null;
		await open('collect');
		if (first) void begin(e.field);
	}

	/** The chores kit, installed once its dry run is seen and agreed to, then its tab; installed, its tab. */
	let chored = $state(false);
	void refreshKits()
		.then(() => named('chores'))
		.then((t) => (chored = t));
	async function chores(): Promise<void> {
		if (chored) return void open('chores');
		const toml = (await import('../../../../kits/chores.toml?raw')).default;
		const argv = ['kit', 'install', '-'];
		const seen = await run([...argv, '--dry-run'], toml);
		if (seen.code !== 0) return void press([...argv, '--dry-run'], undefined, toml);
		const text = `${seen.stdout.trim()}\n\nA kit can add SQL views and commands to your database; install only kits you trust.`;
		if (!(await confirm(text, 'install'))) return;
		if ((await press([...argv, '--yes'], undefined, toml)).code !== 0) return;
		chored = true;
		// The kit's layouts are cached: read again, so its chores tab is one to open.
		await refreshKits();
		await open('chores');
	}
</script>

<div class="home" class:wide bind:clientWidth={width}>
	<h3 class="ask">What do you want to track?</h3>
	{#if failed}
		<p class="quiet">The examples could not be read.</p>
	{/if}
	<div class="split">
		<div class="catalog">
			{#each themes as { theme, examples } (theme)}
				<section data-theme={theme}>
					<h4>{theme}</h4>
					<div class="rows">
						{#each examples as e (e.field)}
							<button
								type="button"
								class="row"
								use:act={wide ? 'show' : 'open'}
								data-example={e.field}
								data-taken={taken.has(e.field) || undefined}
								aria-pressed={shown?.field === e.field}
								onclick={() => pick(e)}><span class="icon" style:color={e.color}>{e.symbol}</span> {e.field}</button
							>
						{/each}
					</div>
					{#if theme === 'home & chores'}
						<p class="kit">
							Chores on a schedule, each due again some days after it is done: the chores kit adds
							<code>newchore</code> and <code>chorelist</code>.
							<button type="button" class="install" use:act={'open'} onclick={chores}
								>{chored ? 'Open the chores tab' : 'Install the chores kit'}</button
							>
						</p>
					{/if}
				</section>
			{/each}
		</div>
		{#if wide && now}
			<aside class="preview" aria-label="preview">{@render preview(now)}</aside>
		{/if}
	</div>
</div>

<!-- Measured first: a sheet opened before the width is known would close, and forget the pick, as it is. -->
{#if width && !wide}
	<Sheet title={now?.field ?? ''} open={!!now} onclose={() => (picked = kept = null)}>
		{#snippet head()}
			<button type="button" class="back" onclick={() => (picked = kept = null)}>‹ back</button>
		{/snippet}
		{#if now}<div class="preview">{@render preview(now)}</div>{/if}
	</Sheet>
{/if}

{#snippet preview(e: Example)}
	{@const base = shown!}
	{@const k = kind(e.kind)}
	{@const c = chosen(base, choiceOf(base))}
	{@const choices = Object.keys(base.choices ?? {})}
	{@const set = taken.has(e.field)}
	<div class="title" data-preview={e.field}>
		<span class="big" style:color={e.color}>{e.symbol}</span>
		<span>
			<span class="name">{e.field}</span>
			<span class="kind">{k.glyph} {k.what}: {k.line}</span>
		</span>
	</div>
	<p>{e.shows}</p>
	{#if set}
		<p class="taken">You track this already: Configure opens yours.</p>
	{:else if base.as?.length || choices.length || c.labels?.length}
		<div class="options">
			{#if base.as?.length}
				{#each [{ kind: base.kind, label: base.label ?? base.kind }, ...base.as] as v, i (i)}
					<button type="button" class="chip" aria-pressed={variant === i} data-variant={v.kind} onclick={() => (variant = i)}
						>{kind(v.kind).glyph} {v.label}</button
					>
				{/each}
			{/if}
			{#if choices.length}
				<span class="quiet">in</span>
				{#each choices as name (name)}
					<button type="button" class="chip" aria-pressed={choiceOf(base) === name} data-choice={name} onclick={() => (choice = name)}>{name}</button>
				{/each}
			{/if}
			{#if c.labels?.length}
				<button type="button" class="chip" role="switch" aria-checked={labelled} data-label onclick={() => (labelled = !labelled)}
					>{labelled ? '☑' : '☐'} show label</button
				>
			{/if}
		</div>
	{/if}
	<h4>A day of it</h4>
	{#if e.kind === 'add'}
		<p class="quiet">A record has no time, so it isn't on the day. One looks like:</p>
		<code class="record">{((e.spec.keys ?? []) as { name: string }[]).map((key) => `${key.name}=…`).join('  ')}</code>
	{:else}
		<div class="band" style:--h="{wide ? 140 : 110}px" bind:clientWidth={bandWidth}>
			<div class="svg" bind:this={band}></div>
			{#if lane}<span class="lane" style:top="{lane.top * 100}%" style:height="{lane.h * 100}%"></span>{/if}
		</div>
	{/if}
	<div class="acts">
		<button type="button" class="configure" use:act={'show'} onclick={() => configure(base)}>Configure</button>
		{#if !set}
			<button type="button" class="start" use:act={'run'} onclick={() => start(base)}>Start tracking</button>
		{/if}
	</div>
{/snippet}

<style>
	.home {
		display: flex;
		flex-direction: column;
		gap: 0.5rem;
		color: var(--ink);
		font-size: 0.85rem;
	}

	.ask {
		margin: 0.5rem 0 0;
		font-size: 1rem;
		font-weight: normal;
		color: var(--ink-bright);
	}

	.quiet,
	.kit,
	.kind {
		margin: 0;
		color: var(--quiet);
	}

	.split {
		display: grid;
		gap: 1rem;
		align-items: start;
	}

	.wide .split {
		grid-template-columns: minmax(0, 1fr) minmax(0, 1fr);
	}

	.catalog {
		display: flex;
		flex-direction: column;
		gap: 0.75rem;
	}

	h4 {
		margin: 0 0 0.2rem;
		font-size: 0.75rem;
		font-weight: normal;
		letter-spacing: 0.06em;
		text-transform: uppercase;
		color: var(--muted);
	}

	/* As many columns as fit: two on a phone. */
	.rows {
		display: grid;
		grid-template-columns: repeat(auto-fill, minmax(9.5rem, 1fr));
		gap: 0 0.5rem;
	}

	.row {
		display: flex;
		align-items: center;
		gap: 0.5rem;
		min-block-size: 40px;
		background: none;
		border: 1px solid transparent;
		border-radius: 4px;
		padding: 0 0.4rem;
		color: var(--ink-bright);
		font: inherit;
		text-align: start;
		cursor: pointer;
	}

	.row:hover {
		background: var(--tint);
	}

	.row[aria-pressed='true'] {
		border-color: var(--accent);
	}

	.icon {
		inline-size: 1.4rem;
		text-align: center;
	}

	.install,
	.chip,
	.configure,
	.back {
		box-sizing: border-box;
		min-block-size: 32px;
		background: var(--raised);
		border: 1px solid var(--rule);
		border-radius: 4px;
		padding: 0.2rem 0.8rem;
		color: var(--ink);
		font: inherit;
		cursor: pointer;
	}

	.install {
		display: block;
		margin-block-start: 0.35rem;
	}

	/* Sticky beside the catalog, so it stays in view while the list scrolls. */
	.wide .preview {
		position: sticky;
		top: 0;
		padding: 1rem;
		background: var(--raised);
		border: 1px solid var(--rule);
		border-radius: 6px;
	}

	.preview {
		display: flex;
		flex-direction: column;
		gap: 0.6rem;
	}

	.preview p {
		margin: 0;
	}

	.title {
		display: flex;
		align-items: center;
		gap: 0.75rem;
	}

	.title > span:last-child {
		display: flex;
		flex-direction: column;
	}

	.big {
		font-size: 2rem;
	}

	.name {
		font-size: 1.15rem;
		color: var(--ink-bright);
	}

	.taken {
		color: var(--accent);
	}

	.options {
		display: flex;
		flex-wrap: wrap;
		align-items: center;
		gap: 0.4rem;
	}

	.chip {
		border-radius: 999px;
		border-color: var(--accent);
		background: none;
		color: var(--accent);
	}

	.chip[aria-pressed='true'],
	.chip[aria-checked='true'] {
		background: var(--accent);
		color: var(--page);
	}

	/* The CLI's band, the lane faintly shaded over it; no ruler, and no now. */
	.band {
		position: relative;
		block-size: var(--h);
		border: 1px solid var(--rule);
		border-radius: 4px;
		overflow: hidden;
	}

	.svg :global(svg) {
		display: block;
	}

	.svg :global(.current-time-line) {
		display: none;
	}

	.lane {
		position: absolute;
		inset-inline: 0;
		background: color-mix(in srgb, var(--accent) 10%, transparent);
		pointer-events: none;
	}

	.record {
		overflow-wrap: anywhere;
	}

	.acts {
		display: flex;
		gap: 0.6rem;
	}

	.acts button {
		flex: 1;
		min-block-size: 40px;
	}

	.start {
		background: var(--track);
		border-radius: 4px;
		color: var(--page);
		font: inherit;
		cursor: pointer;
	}

	/* It writes at once, so the double outline (#lib/act.ts). */
	.start:global([data-act='run']) {
		border: 3px double var(--page);
	}
</style>
