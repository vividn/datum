<!--
	The setup widget's Data rows (da-zqnt3), vividn, 2026-10-14: «simple
	buttons to select if optional or not (an = button in the middle that
	acts exactly like an = sign in the cli?) maybe with an optional? header.
	Only after clicking it does the default value field become visible to
	the right», «handles to drag and reorder the keys», and «at the
	beginning have (-k) button that if you click let's you select -R -SR -C
	as well with a little explanation of what each does».

	Each row is a key as the CLI declares it: its mode is the flag (`-k`,
	`-K`, `-R`, `-SR`, `-C`, `KEY_MODES`), `=` makes it `key=` and shows
	the default box for `key=value`, and the order is the order its words
	bind in. One row takes the rest of the line (`cycle`). On a card a new
	key starts as `-K`: a card needs an id. The Data tab's rows (`detail`)
	also carry each key's type, and whether its value is fixed — an assign,
	which takes no word. The paragraph that explained them is gone
	(da-zz9rn, vividn: «the data explanation is too long, maybe just get rid
	of it? put hints next to pickers»): `+ key` says it in its hint.
-->
<script lang="ts">
	import { tick } from 'svelte';
	import { cycle, KEY_MODES, KEY_TYPES, modeOf, moved, type KeyRow } from './track.js';

	/** The rows, in the order their words bind. */
	export let keys: KeyRow[];
	/** Whether the field is a card, whose new key starts in its id. */
	export let card = false;
	/** The Data tab's rows: each key's type and whether it is fixed. */
	export let detail = false;

	let list: HTMLUListElement;
	/** The row a handle is dragging. */
	let held: KeyRow | null = null;

	const add = () => (keys = [...keys, { name: '', kind: 'required', value: '', value_type: 'any', in_slug: card }]);
	const line = (key: KeyRow) =>
		[KEY_MODES.find((m) => m.mode === modeOf(key))?.line, !key.rest && key.kind === 'assign' && 'fixed: every entry gets this value, and it takes no word']
			.filter(Boolean)
			.join('; ');

	/** `=`, as on the CLI: `key` required, `key=` optional, `key=value` with a default. */
	function eq(key: KeyRow) {
		key.kind = key.kind !== 'required' ? 'required' : key.value ? 'default' : 'optional';
		keys = keys;
	}

	// A drag follows the pointer on the window: the row moves in the DOM as
	// it goes, which would end a capture on the handle.
	function grab(e: PointerEvent, key: KeyRow) {
		e.preventDefault();
		held = key;
		const drop = () => {
			held = null;
			removeEventListener('pointermove', drag);
			removeEventListener('pointerup', drop);
			removeEventListener('pointercancel', drop);
		};
		addEventListener('pointermove', drag);
		addEventListener('pointerup', drop);
		addEventListener('pointercancel', drop);
	}

	function drag(e: PointerEvent) {
		if (!held) return;
		const rows = [...list.querySelectorAll(':scope > li.key')];
		const at = rows.findIndex((li) => e.clientY < li.getBoundingClientRect().bottom);
		const [from, to] = [keys.indexOf(held), at < 0 ? rows.length - 1 : at];
		if (to !== from) keys = moved(keys, from, to);
	}

	/** The arrow keys move a row as a drag does, and the handle keeps the focus. */
	async function nudge(e: KeyboardEvent, i: number) {
		const to = i + ({ ArrowUp: -1, ArrowDown: 1 }[e.key] ?? 0);
		if (to === i || to < 0 || to >= keys.length) return;
		e.preventDefault();
		keys = moved(keys, i, to);
		await tick();
		list.querySelectorAll<HTMLElement>(':scope > li.key .handle')[to]?.focus();
	}
</script>

<ul class="data" bind:this={list}>
	{#if keys.length}<li class="head" aria-hidden="true"><span>optional?</span></li>{/if}
	{#each keys as key, i (key)}
		{@const explained = line(key)}
		<li class="key" class:held={held === key} data-mode={modeOf(key)}>
			<button
				type="button"
				class="handle"
				aria-label="move {key.name || 'this key'}: drag, or the arrow keys"
				on:pointerdown={(e) => grab(e, key)}
				on:keydown={(e) => nudge(e, i)}>⠿</button
			>
			<button
				type="button"
				class="mode"
				aria-label="how {key.name || 'this key'} is declared: -{modeOf(key)}, press for the next"
				on:click={() => (cycle(keys, key), (keys = keys))}>-{modeOf(key)}</button
			>
			{#if key.rest === 'C'}
				<input class="name" value="comment" disabled aria-label="the comment" />
			{:else}
				<input
					class="name"
					bind:value={key.name}
					placeholder="key"
					aria-label="the key's name"
					spellcheck="false"
					autocapitalize="none"
				/>
			{/if}
			{#if !key.rest}
				<!-- A fixed key's = would undo «fixed», which only the Data tab shows: so it has none. -->
				{#if key.kind !== 'assign'}<button
					type="button"
					class="eq"
					aria-pressed={key.kind !== 'required'}
					aria-label="{key.name || 'this key'} is optional"
					on:click={() => eq(key)}>=</button
				>{/if}
				{#if key.kind !== 'required'}
					<input
						class="value"
						bind:value={key.value}
						placeholder={key.kind === 'assign' ? 'value' : 'default'}
						aria-label="the {key.kind === 'assign' ? 'value' : 'default'} of {key.name || 'this key'}"
						on:input={() => key.kind !== 'assign' && (key.kind = key.value ? 'default' : 'optional')}
					/>
				{/if}
			{/if}
			<button type="button" class="remove" aria-label="remove {key.name}" on:click={() => (keys = keys.filter((k) => k !== key))}
				>✕</button
			>
			{#if explained}<span class="line">{explained}</span>{/if}
			{#if detail && !key.rest}
				<span class="flags">
					<label
						><input
							type="checkbox"
							class="fixed"
							checked={key.kind === 'assign'}
							on:change={(e) => ((key.kind = e.currentTarget.checked ? 'assign' : key.value ? 'default' : 'required'), (keys = keys))}
						/> fixed</label
					>
					<select class="type" bind:value={key.value_type} aria-label="the type of {key.name}">
						{#each KEY_TYPES as type (type)}<option>{type}</option>{/each}
					</select>
				</span>
			{/if}
		</li>
	{/each}
</ul>
<button type="button" class="add" title="a value each entry carries, typed after the field, one word per key" on:click={add}
	>+ key</button
>

<style>
	.data {
		display: flex;
		flex-direction: column;
		gap: 0.3rem;
		margin: 0;
		padding: 0;
		list-style: none;
	}

	/* handle · mode · name · = · default · ✕, the header's word over the =. */
	li {
		display: grid;
		grid-template-columns: 1.4rem 2.8rem minmax(0, 1fr) 1.8rem minmax(0, 1fr) auto;
		align-items: center;
		gap: 0.3rem;
	}

	.head span {
		grid-column: 4 / 6;
		color: var(--muted);
		font-size: 0.7rem;
	}

	.held {
		outline: 1px dashed var(--accent);
	}

	.name {
		grid-column: 3;
	}

	.eq {
		grid-column: 4;
	}

	.value {
		grid-column: 5;
	}

	.remove {
		grid-column: 6;
	}

	.line,
	.flags {
		grid-column: 3 / -1;
		color: var(--muted);
		font-size: 0.7rem;
	}

	.flags {
		display: flex;
		align-items: center;
		gap: 0.75rem;
	}

	.flags select {
		margin-inline-start: auto;
		background: var(--raised);
		border: 1px solid var(--rule);
		border-radius: 4px;
		color: var(--ink-bright);
		font: inherit;
	}

	input:not([type='checkbox']) {
		box-sizing: border-box;
		min-inline-size: 0;
		min-block-size: 24px;
		background: var(--raised);
		border: 1px solid var(--rule);
		border-radius: 4px;
		padding: 0.25rem 0.4rem;
		color: var(--ink-bright);
		font: inherit;
	}

	input:disabled {
		color: var(--muted);
	}

	button {
		box-sizing: border-box;
		min-block-size: 24px;
		min-inline-size: 24px;
		background: var(--raised);
		border: 1px solid var(--rule);
		border-radius: 4px;
		padding: 0.2rem 0.45rem;
		color: inherit;
		font: inherit;
		cursor: pointer;
	}

	/* The handle takes the touch: the panel would scroll under it. */
	.handle {
		border: 0;
		background: none;
		padding: 0;
		color: var(--muted);
		cursor: grab;
		touch-action: none;
	}

	.mode {
		padding-inline: 0.2rem;
		color: var(--accent);
	}

	.eq[aria-pressed='true'] {
		border-color: var(--accent);
		color: var(--ink-bright);
	}

	.add {
		align-self: flex-start;
		padding: 0.2rem 0.8rem;
	}
</style>
