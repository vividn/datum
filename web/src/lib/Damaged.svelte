<!--
	A database SQLite cannot read, and the way out (da-0v940).

	vividn, 2026-10-12: «I'm stuck in some sort of horrible seemingly
	unrecoverable state. I even tried deleting all the local, indexedDb,
	session storage etc, and nothing seems to work … This is also a really
	bad place to leave a user.» Every command failed with «database disk
	image is malformed», `db purge` too, since it is SQLite as well. So this
	says where the file is, which the error's `/data/…` never could, and
	offers the two ways out that need nothing else in the app to work: a copy
	of the raw bytes, and the file's removal ($lib/damaged.ts).
-->
<script lang="ts">
	import { act } from './act.js';
	import { saveFile } from './backup.js';
	import { announceChange } from './changes.js';
	import { confirm } from './confirm.js';
	import { copyOf, damaged, reset } from './damaged.js';
	import { OPFS_DIR } from './databases.js';

	export let db: string;

	let said = '';

	async function download() {
		said = '';
		try {
			saveFile(await copyOf(db), `${db}-damaged.tar`);
		} catch (e) {
			said = e instanceof Error ? e.message : String(e);
		}
	}

	async function resetDb() {
		said = '';
		const text = `This deletes ${db}.db and everything in it, and starts ${db} again, empty. Download a copy first if you may want it.`;
		if (!(await confirm(text, 'reset', db))) return;
		try {
			await reset(db);
		} catch (e) {
			said = e instanceof Error ? e.message : String(e);
			return;
		}
		damaged.set(null);
		announceChange([db]);
		location.reload();
	}
</script>

<section class="damaged" role="alert">
	<p>
		datum.day cannot read the database <b>{db}</b>: its file is damaged, so every command on it fails,
		<code>db purge</code> too.
	</p>
	<p>
		It is the file <code>{db}.db</code> in this site's private storage (OPFS), in the directory
		<code>{OPFS_DIR}</code>. Clearing cookies, local storage, session storage or IndexedDB does not touch
		it, and clearing site data in the browser may not either.
	</p>
	<div class="buttons">
		<button use:act={'run'} on:click={download}>Download a copy</button>
		<button use:act={'open'} on:click={resetDb}>Reset this database</button>
	</div>
	{#if said}<p class="said">{said}</p>{/if}
</section>

<style>
	.damaged {
		position: absolute;
		top: 2.6rem;
		left: 50%;
		transform: translateX(-50%);
		z-index: 30;
		width: min(36rem, calc(100% - 2rem));
		padding: 0.6rem 0.9rem;
		font-size: 0.85rem;
		color: var(--ink-bright);
		background: var(--ground);
		border: 1px solid var(--error);
		border-radius: 6px;
	}
	p {
		margin: 0 0 0.5rem;
	}
	.buttons {
		display: flex;
		flex-wrap: wrap;
		gap: 0.5rem;
	}
	button {
		font: inherit;
		color: var(--ink-bright);
		background: var(--tint);
		border: 1px solid var(--rule);
		border-radius: 4px;
		padding: 0.3rem 0.7rem;
		cursor: pointer;
	}
	button:global([data-act='run']) {
		border: 3px double var(--rule);
		padding: calc(0.3rem - 2px) calc(0.7rem - 2px);
	}
	.said {
		margin: 0.5rem 0 0;
		color: var(--error);
	}
</style>
