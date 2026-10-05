// A widget's options menu, every button in it and its submenus tapped
// (da-juku7): a double outline is exactly a tap that wrote or ran. A file of
// their own for node's per-file budget — see `SHARES` in `support/taps.mjs`.

import { everyLayer } from './support/taps.mjs';
import { DESKTOP } from './support/layout.mjs';

everyLayer('desktop', { ...DESKTOP, desk: null }, 'menu', [
	'nowview: menu of the nowview pane',
	'nowview: menu of the nowview pane › page: history',
	'fields: menu of the fields pane'
]);
