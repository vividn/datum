// The commands widget's submenus (da-fg4w4): a command in one of them is
// drawn only once the submenu is open. Read off the widget's own table
// (`MENUS` in #lib/commands.ts), so no test keeps a second list of what is
// where.

import { MENUS } from '../../src/lib/commands.ts';

/** The submenu of the base row that draws `name`, if any. */
export const menuOf = (name) => ['write', 'read', 'fields', 'settings'].find((menu) => MENUS[menu].some((c) => c.name === name));

/**
 * The button for command `name` in `bar` (a `.bar.commands` locator), its
 * submenu opened first where the bar is not drawing it: a loaded pill draws
 * every command that takes it, flat, and a subcommand is in no submenu.
 */
export async function reach(bar, name) {
	const button = bar.locator(`button[data-command="${name}"]`);
	const menu = menuOf(name);
	if (menu && (await button.count()) === 0) {
		await bar.locator(`button[data-menu="${menu}"]`).click();
		await button.waitFor();
	}
	return button;
}
