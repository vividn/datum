<!--
	`datum.day/act/<kind>/…` (da-uzl16): one of datum's actions
	(#lib/actions.ts), opened from outside the page, a link in a message. It
	never runs on its own: the page is `/` again and a dialog says what the
	link would do, which runs on a tap, as the sync login card asks before it
	signs a CLI in. A link to no action, an embed's, or a run the vocabulary
	refuses is a notice saying so, with a link home, and does nothing.
-->
<script lang="ts">
	import { onMount } from 'svelte';
	import { goto } from '$app/navigation';
	import { resolve } from '$app/paths';
	import { page } from '$app/state';
	import { withDb } from '#lib/database.ts';
	import { TAP, actionOf, perform, refusal, said, type Action } from '#lib/actions.ts';
	import { choose } from '#lib/confirm.ts';
	import { arriving } from '#lib/layout/shell.ts';

	const home = withDb(resolve('/'));

	let notice: string | null = $state(null);

	onMount(async () => {
		// The arrival is the way home alone: a run waits for it (`press`), so the question comes after.
		const going = enter();
		arriving(going);
		const action = await going;
		if (action === null) return;
		const verb = action.kind === 'run' ? 'run' : action.kind === 'tab' ? 'go' : 'open';
		if (await choose(`A link asks datum.day to ${said(action)}.`, [verb], 'cancel', TAP[action.kind])) await perform(action);
	});

	/** The action to ask about, the page home again; null where it is none or refused, said in the notice. */
	async function enter(): Promise<Exclude<Action, { kind: 'field' }> | null> {
		const action = actionOf(`/act/${page.params.path}${page.url.search}`);
		if (action === null || action.kind === 'field') return ((notice = 'this link names no action datum.day takes'), null);
		const refused = refusal(action);
		if (refused) return ((notice = `refused: ${refused}`), null);
		await goto(home, { replace: true });
		return action;
	}
</script>

{#if notice !== null}
	<p class="notice">{notice}. <a href={home}>home</a></p>
{/if}

<style>
	.notice {
		margin: 1rem 0;
		color: var(--quiet);
		font-style: italic;
	}

	.notice a {
		color: var(--accent);
	}
</style>
