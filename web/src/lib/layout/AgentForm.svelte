<!--
	«Add agent» (da-bdyor): the form an agent's code is chosen in, and the code
	it makes, with what the code lets the agent do and until when (da-4cw3v).
	One component for the Sync widget and the welcome's «Integrate with AI»
	modal (da-f7orx), so the two never drift: `before` is the modal's chain,
	started where there is none, as a first share starts one.
-->
<script lang="ts">
	import { act } from '#lib/act.ts';
	import { agentArgv, type AgentAsk, type AgentCode } from '#lib/calls.ts';
	import { cliError } from '#lib/run.ts';
	import { syncOffer } from '#lib/sync.ts';
	import { syncAct } from '#lib/syncView.ts';

	let {
		db,
		locked,
		disabled = false,
		before,
		ondone,
		onclose
	}: {
		db: string;
		/** The chain has an admin: only then may a full agent write (ístina's F19). */
		locked: boolean;
		disabled?: boolean;
		/** Run before the code is made. */
		before?: () => Promise<void>;
		/** An attempt ended, made or failed. */
		ondone?: () => void;
		/** Cancel, or Done once the code is copied. */
		onclose?: () => void;
	} = $props();

	const RECENT: Record<string, string> = { '1h': 'hour', '1d': 'day', '7d': 'week' };

	const ask = $state<Required<AgentAsk>>({ level: 'full', write: false, window: '1d', expires: '30d', label: 'agent', fields: '' });
	/** The expiry typed when «after…» is chosen, as `--expires` reads it (da-4cw3v). */
	let own = $state('');
	let made = $state<{ label: string; code: string; until: string | null; can: string } | null>(null);
	let busy = $state(false);
	let failure = $state('');
	let copied = $state('');

	/** What the code lets the agent do: its role, and what it reads and writes. */
	function can(a: AgentCode, window: string): string {
		const reads = a.level === 'full' ? 'everything' : a.level === 'light' ? `the last ${RECENT[window] ?? window} and field setups` : 'nothing';
		const writes = a.role === 'reader' ? 'nothing' : a.level === 'write' ? (a.fields?.length ? `only ${a.fields.join(', ')}` : 'entries in any field') : 'entries';
		return `A ${a.role}: it reads ${reads} and writes ${writes}.`;
	}

	async function make(): Promise<void> {
		busy = true;
		failure = copied = '';
		try {
			await before?.();
			const expires = (ask.expires === 'custom' ? own.trim() : ask.expires) || undefined;
			const write = ask.write && (ask.level !== 'full' || locked);
			const r = await syncAct(db, agentArgv({ ...ask, write, expires, label: ask.label.trim() || undefined }));
			if (r.code !== 0) throw cliError(r.stderr);
			const a: AgentCode = JSON.parse(r.stdout.split('\n')[0]);
			const ticket = await syncOffer(db, { secret: a.secret, expires: a.expires ?? '', id: '', role: a.role, agent: a.agent });
			const until = a.expires ? new Date(a.expires).toLocaleDateString([], { dateStyle: 'medium' }) : null;
			made = { label: a.agent, code: `datum-agent:${ticket}`, until, can: can(a, ask.window) };
		} catch (e) {
			failure = (e as Error).message;
		} finally {
			busy = false;
			ondone?.();
		}
	}

	const copy = (what: string, text: string) =>
		(navigator.clipboard?.writeText(text) ?? Promise.reject()).then(
			() => (copied = `Copied the ${what}.`),
			() => (copied = `This browser did not let the page copy the ${what}.`)
		);
</script>

{#if made}
	<section class="card agentcode" data-db={db}>
		<p class="head">Code for {made.label}: the agent runs <code>datum sync restore '…'</code> with it.</p>
		<p class="head">
			<span class="can">{made.can}</span>
			{#if made.until}Anyone with it is this agent until {made.until}; Remove in Sync revokes it.{:else}Anyone with it is this agent until you Remove it in Sync.{/if}
		</p>
		<code class="code ticket">{made.code}</code>
		<div class="actions">
			<button type="button" class="command" use:act={'run'} onclick={() => made && copy('code', made.code)}>Copy code</button>
			<button type="button" class="command" use:act={'run'} onclick={() => made && copy('CLI command', `datum sync restore '${made.code}'`)}>Copy CLI command</button>
			<button type="button" class="command" use:act={'show'} onclick={() => onclose?.()}>Done</button>
		</div>
		{#if copied}<p class="quiet copied" role="status">{copied}</p>{/if}
	</section>
{:else}
	<form
		class="card agent"
		data-db={db}
		onsubmit={(e) => {
			e.preventDefault();
			void make();
		}}
	>
		<label class="name">Name <input type="text" bind:value={ask.label} spellcheck="false" autocomplete="off" /></label>
		<label class="name"
			>It gets <select bind:value={ask.level}>
				<option value="full">everything</option>
				<option value="light">recent data and field setups</option>
				<option value="write">nothing (it only writes)</option>
			</select></label
		>
		{#if ask.level === 'light'}
			<label class="name"
				>Recent: the last <select bind:value={ask.window}>
					{#each Object.entries(RECENT) as [value, word] (value)}<option {value}>{word}</option>{/each}
				</select></label
			>
		{:else if ask.level === 'write'}
			<label class="name">Only the fields <input type="text" bind:value={ask.fields} placeholder="coffee, mood (blank: any)" spellcheck="false" autocomplete="off" /></label>
		{/if}
		<!-- A full agent that writes is a member, which in a chain with no admin could lock it as its first admin (ístina's F19): the CLI refuses it there. -->
		{#if ask.level === 'full' && !locked}<p class="name">To write too, it needs an admin first (Add admin, in Sync).</p>
		{:else if ask.level !== 'write'}<label class="name"><input type="checkbox" bind:checked={ask.write} /> it writes too</label>{/if}
		<!-- A code is its device's secret, so it lapses unless asked not to (da-4cw3v); Remove revokes it sooner. -->
		<label class="name"
			>It expires <select bind:value={ask.expires}>
				<option value="30d">in 30 days</option>
				<option value="60d">in 60 days</option>
				<option value="90d">in 90 days</option>
				<option value="6mo">in 6 months</option>
				<option value="1y">in a year</option>
				<option value="never">never</option>
				<option value="custom">after…</option>
			</select></label
		>
		{#if ask.expires === 'custom'}<label class="name"
				>After <input type="text" bind:value={own} required pattern={'\\S+'} placeholder="45d, 3mo, 12h" spellcheck="false" autocomplete="off" /></label
			>{/if}
		<div class="actions">
			<button type="submit" class="command" use:act={'run'} disabled={busy || disabled}>Make code</button>
			<button type="button" class="command close" use:act={'show'} onclick={() => onclose?.()}>Cancel</button>
		</div>
		{#if busy}<p class="quiet waiting" role="status">Making the code…</p>{/if}
		{#if failure}<p class="failure" role="alert">{failure}</p>{/if}
	</form>
{/if}

<style>
	.card {
		margin: 0 0 0.5rem;
		padding: 0.5rem;
		border: 1px solid var(--rule);
	}
	.head {
		margin: 0;
		color: var(--ink-bright);
	}
	.code {
		display: block;
		margin-top: 0.3rem;
		user-select: all;
		font-size: 0.8em;
		overflow-wrap: anywhere;
	}
	.actions {
		display: flex;
		flex-wrap: wrap;
		align-items: center;
		gap: 0.4rem;
		margin-top: 0.5rem;
	}
	/* The Sync widget's own fields (`.join input` there), since the form opens inside it: never the browser's white. */
	input[type='text'],
	select {
		background: var(--page);
		border: 1px solid var(--rule);
		border-radius: 4px;
		padding: 0.2rem 0.4rem;
		color: var(--ink-bright);
		font: inherit;
	}
	.quiet {
		color: var(--muted);
	}
	.failure {
		color: var(--error);
	}
</style>
