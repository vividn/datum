<!--
	The `…` menu of a pane: a badge on the frame's outline, right after the
	title or the last tab (PaneFrame.svelte), and the menu it opens.

	A real `<button>` and a real menu: the button opens it (click, Enter,
	Space), arrow keys walk the items, Home/End jump, Escape closes and
	hands focus back, Tab closes and moves on, a click anywhere else closes.
	Every item is a `<button role="menuitem">`, so the keyboard reaches all
	of them (`reachable.test.mjs` is the standard).

	The menu is `position: fixed`, moved out to `document.body` (`portal`)
	and placed from the button's box when it opens: a pane is a scroll
	container inside a scroll container, so an absolutely positioned menu
	would be clipped by the first one it met — and a left-hand bar is
	rotated, and a `fixed` box inside a transformed element is fixed to
	that element, rotation and all, not to the screen.
-->
<script lang="ts">
	import { tick } from 'svelte';

	export let items: { label: string; run: () => void; disabled?: boolean; checked?: boolean }[];
	/** Names the menu for assistive tech: which pane it belongs to. */
	export let label: string;

	let open = false;
	let button: HTMLButtonElement;
	let menu: HTMLDivElement | undefined;
	let at = { top: 0, left: 0 };

	// Placed after it has rendered, so its own size is known: below the
	// button when there is room, above it when the button is near the
	// bottom (a bottom bar, a left-hand bar's end), and never past either
	// edge of the screen — a menu item off-screen is one nobody can reach.
	async function show() {
		open = true;
		await tick();
		const box = button.getBoundingClientRect();
		const width = menu?.offsetWidth ?? 0;
		const height = menu?.offsetHeight ?? 0;
		const below = box.bottom + 2;
		const top = below + height <= window.innerHeight ? below : Math.max(4, box.top - 2 - height);
		at = { top, left: Math.max(4, Math.min(box.left, window.innerWidth - width - 4)) };
		focusItem(0);
	}

	function close(returnFocus = true) {
		if (!open) return;
		open = false;
		if (returnFocus) button.focus();
	}

	function enabled(): HTMLButtonElement[] {
		return menu ? [...menu.querySelectorAll<HTMLButtonElement>('button:not(:disabled)')] : [];
	}

	function focusItem(index: number) {
		const all = enabled();
		if (all.length === 0) return;
		all[((index % all.length) + all.length) % all.length].focus();
	}

	function onMenuKey(event: KeyboardEvent) {
		const all = enabled();
		const here = all.indexOf(document.activeElement as HTMLButtonElement);
		switch (event.key) {
			case 'ArrowDown':
				focusItem(here + 1);
				break;
			case 'ArrowUp':
				focusItem(here - 1);
				break;
			case 'Home':
				focusItem(0);
				break;
			case 'End':
				focusItem(-1);
				break;
			case 'Escape':
				close();
				break;
			case 'Tab':
				close(false);
				return;
			default:
				return;
		}
		event.preventDefault();
	}

	function portal(node: HTMLElement) {
		document.body.appendChild(node);
		return { destroy: () => node.remove() };
	}

	function choose(item: (typeof items)[number]) {
		close();
		item.run();
	}

	function onWindowPointerDown(event: PointerEvent) {
		const target = event.target as Node;
		if (open && !button.contains(target) && !menu?.contains(target)) close(false);
	}
</script>

<svelte:window on:pointerdown={onWindowPointerDown} />

<button
	type="button"
	class="more"
	bind:this={button}
	aria-haspopup="menu"
	aria-expanded={open}
	aria-label={label}
	title="pane menu"
	on:click={() => (open ? close() : show())}>…</button
>

{#if open}
	<div
		class="menu"
		role="menu"
		tabindex="-1"
		aria-label={label}
		bind:this={menu}
		use:portal
		style:top="{at.top}px"
		style:left="{at.left}px"
		on:keydown={onMenuKey}
	>
		{#each items as item (item.label)}
			<!-- The bar and align items are radio-like choices, and say so. -->
			<button
				type="button"
				role={item.checked === undefined ? 'menuitem' : 'menuitemradio'}
				disabled={item.disabled}
				aria-checked={item.checked === undefined ? undefined : item.checked}
				on:click={() => choose(item)}
			>
				<span class="mark">{item.checked ? '✓' : ''}</span>{item.label}
			</button>
		{/each}
	</div>
{/if}

<style>
	/* A rounded square the size of a fingertip, with its own ground so it
	   sits ON the outline rather than under it (vividn, 17:56). 24px is the
	   floor reachable.test.mjs holds it to. It takes the pointer where
	   the line it sits on does not. */
	.more {
		flex: none;
		box-sizing: border-box;
		inline-size: 24px;
		block-size: 24px;
		display: grid;
		place-items: center;
		padding: 0;
		background: #0d0d1a;
		border: 1px solid #2a2a4a;
		border-radius: 6px;
		color: #55558a;
		font: inherit;
		font-size: 0.85rem;
		line-height: 1;
		cursor: pointer;
		pointer-events: auto;
	}

	.more:hover,
	.more:focus-visible,
	.more[aria-expanded='true'] {
		color: #adadff;
		border-color: #7c7cff;
	}

	.menu {
		position: fixed;
		z-index: 20;
		display: flex;
		flex-direction: column;
		min-width: 10rem;
		padding: 0.25rem;
		background: #0d0d1a;
		border: 1px solid #2a2a4a;
		border-radius: 6px;
		box-shadow: 0 4px 16px #00000088;
		writing-mode: horizontal-tb;
		font-family: monospace;
		font-size: 0.8rem;
	}

	.menu button {
		display: flex;
		align-items: baseline;
		gap: 0.3rem;
		text-align: left;
		background: none;
		border: none;
		border-radius: 4px;
		padding: 0.25rem 0.5rem;
		color: #c8c8e8;
		font: inherit;
		cursor: pointer;
	}

	.menu button:hover:not(:disabled),
	.menu button:focus-visible {
		background: #1e1e38;
		color: #e0e0ff;
		outline: none;
	}

	.menu button:disabled {
		color: #444466;
		cursor: default;
	}

	.mark {
		display: inline-block;
		width: 1ch;
		color: #7c7cff;
	}
</style>
