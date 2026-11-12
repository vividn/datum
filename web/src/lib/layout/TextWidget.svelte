<!--
	The text widget (da-12308). vividn, 2026-11-11: «Let's make a "text"
	widget, it's normally static text, but during modify mode, you can enter
	in markdown text that gets formatted when not in modify mode».

	Out of modify mode its `text`, drawn by #lib/markdown.ts as the docs are,
	read-only; in it a textarea of the source, each keystroke an `onchange`
	that `modifyWidget` (mode.ts) writes into the layout, so ✓ keeps it, ↺
	takes it back and the tab's Save saves and syncs it.

	Its images load from datum.day's own origin alone, the bucket the site and
	its pictures are published in (deploy.yml); any other is a link to it, so a
	layout synced to someone else carries no image from a host that learns of
	each view (da-ipk3v).

	Any other image is «image from example.com, tap to keep» (da-wgf4a): a tap
	fetches it once, keeps it in `_internal`, beside the layouts, by its
	SHA-256 (`db attachment`), and writes its target `attachment:<sha256>`, so
	it is drawn from the database, which syncs, and no viewer asks the host
	again. The new text is this widget's own setting moved, held for the
	session until «Save options with layout» saves it, as any widget's is.

	A link to one of datum's actions (#lib/actions.ts, da-uzl16) is a button
	that does it, the reader's tap its yes, and `act:field/<name>` that
	field's collect button itself (#lib/wireActions.ts).
-->
<script lang="ts">
	import Pane from '#lib/Pane.svelte';
	import { markdown, OUTWARD, type Targets } from '#lib/markdown.ts';
	import { wireActions } from '#lib/wireActions.ts';
	import { run } from '#lib/run.ts';
	import { attachmentArgv, keepAttachmentArgv } from '#lib/calls.ts';

	interface Props {
		text?: string;
		modifying?: boolean;
		epoch?: number;
		onchange?: (moved: { text: string }) => void;
	}

	let { text = '', modifying = false, epoch = 0, onchange }: Props = $props();

	/** Each attachment read, its `data:` URL; one not here yet is asked again at the next epoch, as sync may bring it. */
	let held = $state<Record<string, string>>({});
	/** Why the last tap kept nothing. */
	let refused = $state('');

	const ATTACHMENT = /!\[[^\]]*\]\(attachment:([0-9a-f]{64})\)/g;
	$effect(() => {
		void epoch;
		for (const [, hash] of text.matchAll(ATTACHMENT))
			if (!(hash in held))
				void run(['--db', '_internal', ...attachmentArgv(hash)]).then((r) => r.code === 0 && (held[hash] = r.stdout.trim()));
	});

	const image = (src: string) =>
		src.startsWith('attachment:') ? (held[src.slice('attachment:'.length)] ?? null) : new URL(src, location.href).origin === location.origin ? src : null;
	const TARGETS: Targets = { ...OUTWARD, image, act: () => true, keep: true };

	const html = $derived(markdown(text, TARGETS));

	/** Fetch the image at `src` once, keep it, and write its every image's target as the kept copy. */
	async function keep(src: string) {
		refused = '';
		let url: string;
		const host = new URL(src).host;
		try {
			const got = await fetch(src, { credentials: 'omit', referrerPolicy: 'no-referrer' });
			if (!got.ok) {
				refused = `${host} answered ${got.status} ${got.statusText}`.trim();
				return;
			}
			const blob = await got.blob();
			url = await new Promise<string>((done, fail) => {
				const reader = new FileReader();
				reader.onload = () => done(reader.result as string);
				reader.onerror = () => fail(reader.error);
				reader.readAsDataURL(blob);
			});
		} catch {
			// A network error and a host that shares no images with other pages (CORS) are one error to a page.
			refused = `${host} could not be reached, or does not share its images with other pages`;
			return;
		}
		const kept = await run(['--db', '_internal', ...keepAttachmentArgv()], url);
		if (kept.code !== 0) {
			refused = kept.stderr.replace(/^Error:\s*/, '').trim();
			return;
		}
		const target = kept.stdout.trim();
		onchange?.({ text: text.replace(/(!\[[^\]]*\]\()([^)\s]+)\)/g, (whole, head: string, at: string) => (at === src ? `${head}${target})` : whole)) });
	}

	function tapped(event: MouseEvent) {
		const button = (event.target as Element).closest<HTMLButtonElement>('button[data-keep]');
		if (button) void keep(button.dataset.keep!);
	}
</script>

<Pane title="text">
	{#if modifying}
		<textarea
			aria-label="the text, in markdown"
			placeholder={'# A heading\nSome *markdown*: lists, **bold**, [links](https://datum.day)'}
			value={text}
			oninput={(e) => onchange?.({ text: e.currentTarget.value })}
		></textarea>
	{:else if text.trim()}
		<!-- The click is a «tap to keep» button's, which a key presses as any button's. -->
		<!-- svelte-ignore a11y_click_events_have_key_events, a11y_no_static_element_interactions -->
		<div class="text" use:wireActions={html} onclick={tapped}>{@html html}</div>
		{#if refused}<p class="refused" role="alert">Not kept: {refused}</p>{/if}
	{:else}
		<p class="quiet">A text widget: Modify, in its menu, to write its markdown.</p>
	{/if}
</Pane>

<style>
	textarea {
		box-sizing: border-box;
		inline-size: 100%;
		block-size: 100%;
		min-block-size: 6rem;
		resize: none;
		padding: 0.4rem;
		border: 1px solid var(--rule);
		border-radius: 4px;
		background: var(--ground);
		color: var(--ink);
		font-family: ui-monospace, SFMono-Regular, Menlo, Consolas, monospace;
		font-size: 0.85rem;
	}
	.text {
		color: var(--ink);
		line-height: 1.45;
		overflow-wrap: anywhere;
	}
	.text :global(:is(h1, h2, h3, h4)) {
		margin: 0.8rem 0 0.4rem;
		color: var(--ink-bright);
	}
	.text :global(:is(p, ul, ol, pre, table)) {
		margin: 0 0 0.5rem;
	}
	.text :global(:is(ul, ol)) {
		padding-inline-start: 1.4rem;
	}
	.text :global(strong) {
		color: var(--ink-bright);
	}
	.text :global(a) {
		color: var(--accent-bright);
	}
	.text :global(code) {
		font-family: ui-monospace, SFMono-Regular, Menlo, Consolas, monospace;
		font-size: 0.9em;
	}
	.refused,
	.text :global(.refused) {
		color: var(--quiet);
		font-style: italic;
	}
	.text :global(img) {
		max-inline-size: 100%;
	}
	.text :global(button.keep) {
		font: inherit;
		color: var(--accent-bright);
		background: none;
		border: 1px dashed var(--rule);
		border-radius: 4px;
		padding: 0.2rem 0.5rem;
		cursor: pointer;
	}
</style>
