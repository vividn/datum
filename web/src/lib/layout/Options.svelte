<!--
	A widget's options (da-gvdc): one modal, drawn from the options its KIND
	declares in the registry — nothing about a particular kind is written
	here. `options.ts` opens it from the pane menu's `Options…`.

	It keeps no copy of the settings. Every row reads the widget as the live
	tree holds it now, found by its tab's id, so a setting the widget changes
	itself behind the modal (nowview's `+`) shows here at once; and every
	change is applied at once down the one path settings take (`setWidget`),
	so the layout's draft, its dot, Save and Restore all see it. A key the
	kind does not declare — written by a newer app — is never drawn and is
	kept, since a change folds into the settings rather than replacing them.

	The `<dialog>` of Ask.svelte and Confirm.svelte: Escape or ✕ closes it
	and the browser hands focus back to what opened it — the menu's button,
	which the menu focuses before it runs the item. «defaults» is `Reset
	widget`'s act.
-->
<script lang="ts">
	import { createEventDispatcher, onMount } from 'svelte';
	import { apply, layout } from './current.js';
	import { KINDS, labelOf, optionsOfKind } from './registry.js';
	import { resetWidget } from './shell.js';
	import { findTab, setWidget } from './tree.js';
	import { isWidget } from './types.js';

	export let tabId: string;

	const dispatch = createEventDispatcher<{ close: null }>();
	let dialog: HTMLDialogElement;

	onMount(() => dialog.showModal());

	$: tab = findTab($layout, tabId);
	$: widget = tab && isWidget(tab.content) ? tab.content : undefined;
	$: options = widget ? (optionsOfKind(widget.kind) ?? []) : [];
	$: defaults = (widget && KINDS[widget.kind]?.meta.defaultSettings) ?? {};
	// The widget went (closed from another tab of this layout): nothing left to set.
	$: if (dialog && !widget) dialog.close();

	$: valueOf = (key: string) => widget?.settings[key] ?? defaults[key];

	function set(key: string, value: unknown) {
		if (widget) apply((l) => setWidget(l, tabId, { ...widget, settings: { ...widget.settings, [key]: value } }));
	}
</script>

<dialog class="options" bind:this={dialog} aria-labelledby="options-title" on:close={() => dispatch('close', null)}>
	<header>
		<h2 id="options-title">{tab ? labelOf(tab) : ''} options</h2>
		<button type="button" class="x" aria-label="close" on:click={() => dialog.close()}>✕</button>
	</header>
	{#each options as option (option.key)}
		<label class="row" data-option={option.key}>
			<span>{option.label}</span>
			{#if option.type === 'boolean'}
				<input
					type="checkbox"
					checked={valueOf(option.key) === true}
					on:change={(e) => set(option.key, e.currentTarget.checked)}
				/>
				<small>default {defaults[option.key] ? 'on' : 'off'}</small>
			{:else}
				<select value={valueOf(option.key)} on:change={(e) => set(option.key, e.currentTarget.value)}>
					{#each option.choices as choice (choice.value)}
						<option value={choice.value}>{choice.label}{choice.value === defaults[option.key] ? ' · default' : ''}</option>
					{/each}
				</select>
			{/if}
		</label>
	{/each}
	<div class="buttons">
		<button type="button" on:click={() => tab && resetWidget(tab)?.()}>defaults</button>
	</div>
</dialog>

<style>
	.options {
		width: min(24rem, calc(100vw - 2rem));
		box-sizing: border-box;
		background: var(--ground);
		color: var(--ink-bright);
		border: 1px solid var(--accent);
		border-radius: 6px;
		font-family: ui-monospace, SFMono-Regular, Menlo, Consolas, monospace;
		font-size: 0.9rem;
	}

	.options::backdrop {
		background: rgb(0 0 0 / 0.6);
	}

	header {
		display: flex;
		align-items: center;
		justify-content: space-between;
	}

	h2 {
		margin: 0;
		font-size: inherit;
		font-weight: normal;
		color: var(--accent);
	}

	/* A row is one tap: the whole label toggles or opens its control. */
	.row {
		display: flex;
		flex-wrap: wrap;
		align-items: center;
		gap: 0.5rem;
		min-height: 2.75rem;
		border-top: 1px solid var(--tint);
		cursor: pointer;
	}

	.row span {
		flex: 1;
	}

	small {
		color: var(--quiet);
	}

	input[type='checkbox'] {
		width: 1.25rem;
		height: 1.25rem;
		accent-color: var(--accent);
	}

	select,
	button {
		background: var(--tint);
		border: 1px solid var(--rule);
		border-radius: 4px;
		color: inherit;
		font: inherit;
		min-height: 2rem;
	}

	button {
		padding: 0.2rem 0.8rem;
		cursor: pointer;
	}

	.x {
		padding: 0 0.6rem;
	}

	.buttons {
		display: flex;
		margin-top: 0.75rem;
	}
</style>
