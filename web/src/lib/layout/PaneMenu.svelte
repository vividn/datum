<!--
	The `…` menu of a pane: a badge on the frame's outline, right after the
	title or the last tab (PaneFrame.svelte), and the menu it opens.

	A real `<button>` and a real menu: the button opens it (click, Enter,
	Space), arrow keys walk the items, Home/End jump, Escape closes and
	hands focus back, Tab closes and moves on, a click anywhere else closes.
	Every item is a `<button role="menuitem">`, so the keyboard reaches all
	of them (`reachable.test.mjs` is the standard).

	An item with `sub` opens a SUBMENU beside it (click, Enter, Space,
	ArrowRight — and a mouse or a pen resting on it, without taking the
	focus; a touch keeps the tap): one row of small icon buttons, its
	groups split by a thin line — the pane's `Bar ▸` (da-nk4k). No words
	on the buttons, so each carries them as its `aria-label` and `title`.
	ArrowLeft/ArrowRight walk the row, Home/End jump, Escape — or
	ArrowLeft on the first — goes back to the item that opened it, the
	pointer entering any other item closes it, and choosing closes both,
	as choosing does (da-13bc: "bar submenu should open immediately on
	mouse over").

	An item with `row` is its label and a few short buttons on one line —
	the pane's `Split | −` (da-13bc). The buttons are menu items like any
	other, walked by ArrowUp/ArrowDown in order and by ArrowLeft/ArrowRight
	along the line, and their `title` is their name.

	The menu is `position: fixed`, moved out to `document.body` (`portal`)
	and placed from the button's box when it opens: a pane is a scroll
	container inside a scroll container, so an absolutely positioned menu
	would be clipped by the first one it met — and a left-hand bar is
	rotated, and a `fixed` box inside a transformed element is fixed to
	that element, rotation and all, not to the screen.
-->
<script lang="ts">
	import { createEventDispatcher, tick } from 'svelte';

	/** A submenu's button: `icon` is the thick bar `[x, y, w, h]` drawn in a 16px square. */
	type Choice = { label: string; title: string; icon: number[]; checked: boolean; run: () => void };
	/** A row's button: `text` is what it shows, `title` its name. */
	type Short = { text: string; title: string; run: () => void };

	export let items: { label: string; run?: () => void; sub?: Choice[][]; row?: Short[]; disabled?: boolean }[];
	/** Names the menu for assistive tech: which pane it belongs to. */
	export let label: string;
	/** What the button shows; the root bar's picker is this menu with a `+` (da-0a5o). */
	export let glyph = '…';
	export let title = 'pane menu';
	/** A faint handle filling its box — the page margin's (da-qk2k) — rather than a badge. */
	export let faint = false;

	/** `open`: the menu is about to show — a chance to refresh its items. */
	const dispatch = createEventDispatcher<{ open: void }>();

	let open = false;
	let button: HTMLButtonElement;
	let menu: HTMLDivElement | undefined;
	let at = { top: 0, left: 0 };
	/** The open submenu: its item, and the button that opened it. */
	let sub: { item: (typeof items)[number]; opener: HTMLButtonElement } | null = null;
	let subMenu: HTMLDivElement | undefined;
	let subAt = { top: 0, left: 0 };

	/** `start`, moved in so that `size` from it still ends 4px inside `limit`. */
	const fit = (start: number, size: number, limit: number) => Math.max(4, Math.min(start, limit - size - 4));

	// Placed after it has rendered, so its own size is known: below the
	// button when there is room, above it when the button is near the
	// bottom (a bottom bar, a left-hand bar's end), and never past either
	// edge of the screen — a menu item off-screen is one nobody can reach.
	async function show() {
		dispatch('open');
		open = true;
		await tick();
		const box = button.getBoundingClientRect();
		const width = menu?.offsetWidth ?? 0;
		const height = menu?.offsetHeight ?? 0;
		const below = box.bottom + 2;
		const top = below + height <= window.innerHeight ? below : Math.max(4, box.top - 2 - height);
		at = { top, left: fit(box.left, width, window.innerWidth) };
		focusItem(enabled(menu), 0);
	}

	function close(returnFocus = true) {
		if (!open) return;
		open = false;
		sub = null;
		if (returnFocus) button.focus();
	}

	// Beside its item — to the right of the menu when there is room, else
	// below the item — and, like the menu, never off-screen.
	async function showSub(item: (typeof items)[number], opener: HTMLButtonElement, focus = true) {
		sub = { item, opener };
		await tick();
		const box = opener.getBoundingClientRect();
		const right = (menu?.getBoundingClientRect().right ?? box.right) + 2;
		const width = subMenu?.offsetWidth ?? 0;
		const height = subMenu?.offsetHeight ?? 0;
		const beside = right + width <= window.innerWidth - 4;
		subAt = {
			top: fit(beside ? box.top : box.bottom + 2, height, window.innerHeight),
			left: fit(beside ? right : box.left, width, window.innerWidth)
		};
		if (focus) focusItem(enabled(subMenu), 0);
	}

	function closeSub(returnFocus = true) {
		if (returnFocus) sub?.opener.focus();
		sub = null;
	}

	/** The pointer on an item: a `sub` opens, anything else closes the open one. */
	function hover(event: PointerEvent, item?: (typeof items)[number]) {
		if (event.pointerType === 'touch') return;
		if (item?.sub) {
			if (sub?.item !== item) showSub(item, event.currentTarget as HTMLButtonElement, false);
		} else if (sub) closeSub(!!subMenu?.contains(document.activeElement));
	}

	function enabled(within: HTMLElement | undefined): HTMLButtonElement[] {
		return within ? [...within.querySelectorAll<HTMLButtonElement>('button:not(:disabled)')] : [];
	}

	function focusItem(all: HTMLButtonElement[], index: number) {
		if (all.length === 0) return;
		all[((index % all.length) + all.length) % all.length].focus();
	}

	function onMenuKey(event: KeyboardEvent) {
		const all = enabled(menu);
		const here = all.indexOf(document.activeElement as HTMLButtonElement);
		switch (event.key) {
			case 'ArrowDown':
				focusItem(all, here + 1);
				break;
			case 'ArrowUp':
				focusItem(all, here - 1);
				break;
			case 'Home':
				focusItem(all, 0);
				break;
			case 'End':
				focusItem(all, -1);
				break;
			case 'ArrowRight':
			case 'ArrowLeft': {
				const el = all[here];
				const forward = event.key === 'ArrowRight';
				if (forward && el?.hasAttribute('aria-haspopup')) el.click();
				else if (el?.parentElement?.classList.contains('row')) {
					const to = forward ? el.nextElementSibling : el.previousElementSibling;
					if (to instanceof HTMLButtonElement) to.focus();
				}
				break;
			}
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

	function onSubKey(event: KeyboardEvent) {
		const all = enabled(subMenu);
		const here = all.indexOf(document.activeElement as HTMLButtonElement);
		switch (event.key) {
			case 'ArrowRight':
				focusItem(all, here + 1);
				break;
			case 'ArrowLeft':
				if (here <= 0) closeSub();
				else focusItem(all, here - 1);
				break;
			case 'Home':
				focusItem(all, 0);
				break;
			case 'End':
				focusItem(all, -1);
				break;
			case 'Escape':
				closeSub();
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

	function choose(item: { run?: () => void }) {
		close();
		item.run?.();
	}

	function onWindowPointerDown(event: PointerEvent) {
		const target = event.target as Node;
		const inside = [button, menu, subMenu].some((el) => el?.contains(target));
		if (open && !inside) close(false);
	}
</script>

<svelte:window on:pointerdown={onWindowPointerDown} />

<button
	type="button"
	class:more={!faint}
	class:faint
	bind:this={button}
	aria-haspopup="menu"
	aria-expanded={open}
	aria-label={label}
	{title}
	on:click={() => (open ? close() : show())}>{glyph}</button
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
			{#if item.row}
				<div class="row" role="group" aria-label={item.label}>
					<span aria-hidden="true">{item.label}</span>
					{#each item.row as short (short.title)}
						<button
							type="button"
							role="menuitem"
							aria-label={short.title}
							title={short.title}
							on:pointerenter={hover}
							on:click={() => choose(short)}>{short.text}</button
						>
					{/each}
				</div>
			{:else}
				<button
					type="button"
					role="menuitem"
					class:opens={item.sub}
					disabled={item.disabled}
					aria-haspopup={item.sub ? 'menu' : undefined}
					aria-expanded={item.sub ? sub?.item === item : undefined}
					on:pointerenter={(e) => hover(e, item)}
					on:click={(e) => (item.sub ? showSub(item, e.currentTarget) : choose(item))}
				>
					{item.label}
				</button>
			{/if}
		{/each}
	</div>
{/if}

{#if open && sub}
	<div
		class="menu icons"
		role="menu"
		tabindex="-1"
		aria-label={sub.item.label}
		aria-orientation="horizontal"
		bind:this={subMenu}
		use:portal
		style:top="{subAt.top}px"
		style:left="{subAt.left}px"
		on:keydown={onSubKey}
	>
		{#each sub.item.sub ?? [] as group, g}
			{#if g > 0}<div class="sep" role="separator" aria-orientation="vertical" />{/if}
			{#each group as choice (choice.label)}
				<button
					type="button"
					role="menuitemradio"
					aria-checked={choice.checked}
					aria-label={choice.label}
					title={choice.title}
					on:click={() => choose(choice)}
				>
					<svg viewBox="0 0 16 16" width="16" height="16" aria-hidden="true">
						<rect class="frame" x="1.5" y="1.5" width="13" height="13" rx="2.5" />
						<rect x={choice.icon[0]} y={choice.icon[1]} width={choice.icon[2]} height={choice.icon[3]} rx="1" />
					</svg>
				</button>
			{/each}
		{/each}
	</div>
{/if}

<style>
	/* A rounded square with its own ground so it sits ON the outline
	   rather than under it (vividn, 17:56). On a pane's frame it is the
	   title's height where the pointer is fine and a fingertip's 24px
	   where it is not (PaneFrame.svelte's `--badge`); the root bar's are
	   24px — the floors reachable.test.mjs holds them to. It takes the
	   pointer where the line it sits on does not. */
	.more {
		flex: none;
		box-sizing: border-box;
		inline-size: var(--badge, 24px);
		block-size: var(--badge, 24px);
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

	/* The margin's handle: no ground and no outline, only the glyph, barely
	   there until it is pointed at or focused. Its box is the target — and
	   the glyph's line box, 14px in the 12px margin, is clipped to it, or
	   the right-hand one reaches 1px past the page and main scrolls
	   sideways by it (da-5xg3). */
	.faint {
		overflow: hidden;
		inline-size: 100%;
		block-size: 100%;
		display: grid;
		place-items: center;
		padding: 0;
		background: none;
		border: none;
		color: #7c7cff;
		opacity: 0.25;
		font: inherit;
		font-size: 0.75rem;
		line-height: 1;
		cursor: pointer;
	}

	.faint:hover,
	.faint:focus-visible,
	.faint[aria-expanded='true'] {
		opacity: 1;
		outline: none;
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

	.menu button.opens::after {
		content: '▸';
		margin-inline-start: auto;
		color: #55558a;
	}

	/* A row: its label where an item's words are, its buttons at the end,
	   each a fingertip's 24px and more (reachable.test.mjs). */
	.row {
		display: flex;
		align-items: center;
		gap: 2px;
		padding-inline-start: 0.5rem;
		color: #c8c8e8;
	}
	.row span {
		flex: 1;
	}
	.row button {
		box-sizing: border-box;
		inline-size: 28px;
		block-size: 28px;
		padding: 0;
		align-items: center;
		justify-content: center;
	}

	/* The submenu: one row of icons, each a fingertip's 24px and more
	   (reachable.test.mjs), muted until it is the one chosen. */
	.icons {
		flex-direction: row;
		align-items: stretch;
		gap: 2px;
		min-width: 0;
	}
	.icons button {
		box-sizing: border-box;
		inline-size: 28px;
		block-size: 28px;
		padding: 0;
		align-items: center;
		justify-content: center;
		color: #55558a;
	}
	.icons button[aria-checked='true'] {
		color: #7c7cff;
	}
	.icons svg {
		fill: currentColor;
	}
	.icons .frame {
		fill: none;
		stroke: currentColor;
	}
	.sep {
		inline-size: 1px;
		margin: 4px 3px;
		background: #2a2a4a;
	}
</style>
