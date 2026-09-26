<!--
	`seed` — a sample week (da-6ecm5), written into the database this page
	is on, which DebugPane.svelte has already checked is `DEBUG`. The
	commands are $lib/sample.ts's, and so is the sample KIT (da-1mkej) —
	`kits/sample.toml`, specs and aliases for the same fields, which
	installing and uninstalling change and nothing else.

	vividn, on #118: "adding sample data shouldn't delete existing data or
	vice versa." So seeding only adds — a second seed is a second sample,
	and the report says so — and clearing takes only what seeding wrote.

	Every command is shown with what it said back, because when the sample
	looks wrong the log is how anyone finds out why. The count is read back
	from the database after the last one, never counted from the list: the
	two differ exactly when something went wrong.
-->
<script lang="ts">
	import { act } from './act.js';
	import { ran } from './layout/engine.js';
	import { clearSample, installKit, seedSample, uninstallKit, type Step } from './sample.js';
	import { stripAnsi } from './ansi.js';

	let steps: Step[] = [];
	let busy = false;
	let report = '';
	let failed = false;

	const entries = (n: number) => `${n} sample ${n === 1 ? 'entry' : 'entries'} in this database`;

	/** Seed: what the database holds now, whether an earlier sample is among it, and any shortfall. */
	async function seed(onStep: (s: Step) => void): Promise<string> {
		const { before, after, sent } = await seedSample(onStep);
		failed = after !== before + sent;
		return (
			entries(after) +
			(before ? ` — ${before} from an earlier seed, which seeding does not remove` : '') +
			(failed ? ` — expected ${before + sent}` : '')
		);
	}

	async function clear(onStep: (s: Step) => void): Promise<string> {
		const left = await clearSample(onStep);
		failed = left !== 0;
		return entries(left);
	}

	/** The kit's report only says it ran; what the CLI said is in the log below it. */
	const kit = (how: typeof installKit, done: string) => async (onStep: (s: Step) => void) => {
		await how(onStep);
		return done;
	};

	async function perform(what: typeof seed): Promise<void> {
		busy = true;
		failed = false;
		report = '';
		steps = [];
		try {
			report = await what((s) => (steps = [...steps, s]));
		} catch (e) {
			failed = true;
			report = `stopped: ${(e as Error).message}`;
		} finally {
			busy = false;
			await ran();
		}
	}
</script>

<p class="acts">
	<button type="button" class="tool" use:act={'run'} disabled={busy} on:click={() => perform(seed)}>seed the last 7 days</button>
	<button type="button" class="tool" use:act={'run'} disabled={busy} on:click={() => perform(clear)}>clear the sample</button>
	<button type="button" class="tool" use:act={'run'} disabled={busy} on:click={() => perform(kit(installKit, 'the sample kit is installed'))}
		>install the sample kit</button
	>
	<button type="button" class="tool" use:act={'run'} disabled={busy} on:click={() => perform(kit(uninstallKit, 'the sample kit is uninstalled'))}
		>uninstall the sample kit</button
	>
</p>
{#if report}<p class="report" class:failed>{report}</p>{/if}
<ol class="log">
	{#each steps as s}
		<li class:failed={s.code !== 0}>
			<code>❯ datum {s.argv.join(' ')}</code>
			{#if (s.stdout + s.stderr).trim()}<pre>{stripAnsi(s.stdout + s.stderr).trim()}</pre>{/if}
		</li>
	{/each}
</ol>

<style>
	.acts {
		display: flex;
		flex-wrap: wrap;
		gap: 0.4rem;
		margin: 0;
	}
	/* 24px tall, the house rule for a control a finger hits (web-conventions #12). */
	.tool {
		min-height: 24px;
		padding: 0.1rem 0.5rem;
		font: inherit;
		color: var(--accent);
		background: none;
		border: 1px solid var(--rule);
		border-radius: 4px;
		cursor: pointer;
	}
	/* Each writes the database at once, so the double outline ($lib/act.ts). */
	.tool:global([data-act='run']) {
		border: 3px double var(--accent);
		padding: calc(0.1rem - 2px) calc(0.5rem - 2px);
	}
	.tool[disabled] {
		color: var(--quiet);
		cursor: default;
	}
	.report {
		margin: 0.4rem 0 0;
	}
	.failed {
		color: var(--error);
	}
	.log {
		margin: 0.4rem 0 0;
		padding: 0;
		list-style: none;
	}
	.log li {
		margin-bottom: 0.3rem;
	}
	.log pre {
		margin: 0 0 0 1rem;
		white-space: pre-wrap;
		color: var(--quiet);
	}
</style>
