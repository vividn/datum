// Every button in every sheet, dialog and menu the shipped desktop page
// opens, tapped (da-juku7): a double outline is exactly a tap that wrote or
// ran, as on the page itself. The how is `support/taps.mjs`'s `everyLayer`.
//
// This file walks the `rest` of the page's openers; the menus that reshape
// the layout are `act-taps-dialogs-layout` and `act-taps-dialogs-pane`, and
// a widget's options menu `act-taps-dialogs-menu`, a file each for node's
// per-file budget. Desktop alone: a phone walk would
// double the cost again.

import { everyLayer } from './support/taps.mjs';
import { DESKTOP } from './support/layout.mjs';

// The shipped tree, as act-taps-desktop has it. What the walk has to reach:
// the sheet «Delete plan» slipped through in (#241), its confirm, and the
// root tab's menu, whose «Save layout» writes and so is
// double (da-kk0vr).
everyLayer('desktop', { ...DESKTOP, desk: null }, 'rest', [
	'page: main',
	'page: settings › page: Backups',
	'page: settings › page: Backups › backups: Delete plan'
]);
