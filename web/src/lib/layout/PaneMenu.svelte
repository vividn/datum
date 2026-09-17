<!--
	The menu — of a pane, opened by its title or the tab in front
	(PaneFrame.svelte), and of the whole page, opened by the active root
	tab's name (RootBar.svelte). One component, one list of items
	(`menu.ts`); what differs between the two is the `look` of the button
	and which parts of that list apply.

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

	An item with `glyph` draws that character before its label — `Close`,
	the last item of every menu, wears an `×` (da-ygc9). It is decoration,
	so it is `aria-hidden`: the item's name is still its label alone.

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
	import { tick } from 'svelte';
	import type { Item } from './menu.js';

	/** The items, from the one builder both menus use (`menu.ts`). */
	export let items: Item[];
	/**
	 * Names the menu for assistive tech: which pane it belongs to. It names
	 * the BUTTON too, where the button has no words of its own — a trigger
	 * with words (a tab) is named by them — unless the caller passes an
	 * `aria-label` of its own, which is what a trigger whose whole word is
	 * a GLYPH does (the root bar's gear, da-pp1a).
	 */
	export let label: string;
	export let title = 'pane menu';
	/**
	 * What the button that opens the menu looks like: the pane's own title
	 * — a button with no look of its own, laid over the title's box
	 * (da-9vtw) — or one of its tabs, which is what the root bar's active
	 * tab is too (da-wjob). Everything else about the menu is the same
	 * either way, and the attributes a trigger needs beyond these (a tab's
	 * `role` and `aria-selected`) are passed straight through to the
	 * button — a `class` among them is ADDED to the look's own rather than
	 * replacing it, which is how a root tab wears its `modified`
	 * (RootBar.svelte). `badge` is the square the root bar's `+` is, for a
	 * button whose whole word is a glyph.
	 */
	export let look: 'title' | 'tab' | 'badge' | 'plain' = 'title';

	const LOOK: Record<typeof look, string> = {
		title: 'title',
		tab: 'control tab',
		badge: 'control badge',
		// A button with no look at all, for a trigger whose appearance is
		// its caller's own — the bottom bar's logo (da-y992, Brand.svelte),
		// which is a word in a typeface and not a control in a frame.
		plain: 'plain'
	};

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

	// Placed from its own size: below the button when there is room, above
	// it when the button is near the bottom (a bottom bar, a left-hand
	// bar's end), and never past either edge of the screen — a menu item
	// off-screen is one nobody can reach.
	//
	// Called again whenever the menu's box changes, because a menu can
	// GROW after it has opened: the logo's list of databases is read off
	// OPFS while the menu is already on the screen (da-y992), and a menu
	// that fitted under a bottom bar as two items hung half off the screen
	// as four. Placement follows the size, rather than a size measured
	// once.
	function place() {
		if (!open || !button) return;
		const box = button.getBoundingClientRect();
		const width = menu?.offsetWidth ?? 0;
		const height = menu?.offsetHeight ?? 0;
		const below = box.bottom + 2;
		const top = below + height <= window.innerHeight ? below : Math.max(4, box.top - 2 - height);
		at = { top, left: fit(box.left, width, window.innerWidth) };
	}

	async function show() {
		open = true;
		await tick();
		place();
		// The first item that can be focused — or the menu itself, when
		// none can: a menu may be all facts and no acts, and one holding
		// focus nowhere is one Escape cannot close.
		const all = enabled(menu);
		if (all.length > 0) focusItem(all, 0);
		else menu?.focus();
	}

	function close(returnFocus = true) {
		if (!open) return;
		open = false;
		sub = null;
		if (returnFocus) button.focus();
	}

	// Beside its item — clear of the menu, to its right when there is room
	// and to its left when there is not — and, like the menu, never
	// off-screen. Only when neither side fits does it drop BELOW the item,
	// the one placement that lies over the items under it: the pointer's
	// way in then crosses one of them, which closes the submenu it was
	// going to (da-13bc, and Close is under `Bar` now — da-ygc9).
	async function showSub(item: (typeof items)[number], opener: HTMLButtonElement, focus = true) {
		sub = { item, opener };
		await tick();
		const box = opener.getBoundingClientRect();
		const own = menu?.getBoundingClientRect();
		const width = subMenu?.offsetWidth ?? 0;
		const height = subMenu?.offsetHeight ?? 0;
		const right = (own?.right ?? box.right) + 2;
		const left = (own?.left ?? box.left) - 2 - width;
		const beside = right + width <= window.innerWidth - 4 ? right : left >= 4 ? left : null;
		subAt = {
			top: fit(beside === null ? box.bottom + 2 : box.top, height, window.innerHeight),
			left: fit(beside ?? box.left, width, window.innerWidth)
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

	/**
	 * Out to `document.body`, and — when `replace` is given — placed again
	 * on every change of the node's own box (see `place`).
	 */
	function portal(node: HTMLElement, replace?: () => void) {
		document.body.appendChild(node);
		const watch = replace && new ResizeObserver(() => replace());
		watch?.observe(node);
		return {
			destroy: () => {
				watch?.disconnect();
				node.remove();
			}
		};
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
	{...$$restProps}
	type="button"
	class="{LOOK[look]} {$$restProps.class ?? ''}"
	bind:this={button}
	aria-haspopup="menu"
	aria-expanded={open}
	aria-label={$$restProps['aria-label'] ?? ($$slots.default ? undefined : label)}
	{title}
	on:keydown
	on:pointerenter
	on:focus
	on:click={() => (open ? close() : show())}><slot /></button
>

{#if open}
	<div
		class="menu"
		role="menu"
		tabindex="-1"
		aria-label={label}
		bind:this={menu}
		use:portal={place}
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
					role={item.checked === undefined ? 'menuitem' : 'menuitemradio'}
					aria-checked={item.checked}
					class:opens={item.sub}
					class:marked={item.marked}
					disabled={item.disabled}
					aria-haspopup={item.sub ? 'menu' : undefined}
					aria-expanded={item.sub ? sub?.item === item : undefined}
					on:pointerenter={(e) => hover(e, item)}
					on:click={(e) => (item.sub ? showSub(item, e.currentTarget) : choose(item))}
				>{#if item.glyph}<span class="glyph" aria-hidden="true">{item.glyph}</span> {/if}{item.label}</button
				>
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
	/* The pane's own title as the button (da-9vtw): no look of its own, laid
	   over the title's box — the words under it are the frame's h2, or the
	   widget's where it draws its own, and the controls a widget puts there
	   (the custom pane's ✎) are lifted above it (PaneFrame.svelte). It
	   fills that box, which is a finger's 24px where the pointer is coarse
	   and the row's own height where it is not. */
	.title {
		position: absolute;
		inset: 0;
		padding: 0;
		background: none;
		border: none;
		color: inherit;
		font: inherit;
		cursor: pointer;
		pointer-events: auto;
	}

	.title:focus-visible {
		outline: 1px solid var(--accent);
		outline-offset: -1px;
	}

	/* Nothing of its own: the caller draws it (see `LOOK`). */
	.plain {
		display: block;
		padding: 0;
		background: none;
		border: none;
		color: inherit;
		font: inherit;
		text-align: inherit;
		cursor: pointer;
	}

	.menu {
		position: fixed;
		z-index: 20;
		display: flex;
		flex-direction: column;
		min-width: 10rem;
		padding: 0.25rem;
		background: var(--ground);
		border: 1px solid var(--rule);
		border-radius: 6px;
		/* A shadow is black at an opacity, not a colour from the palette. */
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
		color: var(--ink);
		font: inherit;
		cursor: pointer;
	}

	.menu button:hover:not(:disabled),
	.menu button:focus-visible {
		background: var(--tint);
		color: var(--ink-bright);
		outline: none;
	}

	/* Marked: the act the orange dot is about (da-wjob). The same amber the
	   dot and the modified tab wear, so the eye joins the three. Colour and
	   weight, not a ground, so hover above still reads. */
	.menu button.marked {
		color: var(--warn);
		font-weight: bold;
	}

	/* The glyph before a label (da-ygc9): the same size as the words, muted
	   so the words lead, and a fixed width so every label starts where the
	   ones without a glyph do. */
	.menu button .glyph {
		inline-size: 0.8em;
		color: var(--muted);
	}

	.menu button:hover:not(:disabled) .glyph,
	.menu button:focus-visible .glyph {
		color: inherit;
	}

	.menu button:disabled {
		color: var(--quiet);
		cursor: default;
	}

	.menu button.opens::after {
		content: '▸';
		margin-inline-start: auto;
		color: var(--muted);
	}

	/* A row: its label where an item's words are, its buttons at the end,
	   each a fingertip's 24px and more (reachable.test.mjs). */
	.row {
		display: flex;
		align-items: center;
		gap: 2px;
		padding-inline-start: 0.5rem;
		color: var(--ink);
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
		color: var(--muted);
	}
	.icons button[aria-checked='true'] {
		color: var(--accent);
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
		background: var(--rule);
	}
</style>
