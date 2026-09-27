// A pane's layout menus, every button in them tapped (da-juku7): a double
// outline is exactly a tap that wrote or ran. A file of their own for
// node's per-file budget — see `SHARES` in `support/taps.mjs`.

import { everyLayer } from './support/taps.mjs';
import { DESKTOP } from './support/layout.mjs';

everyLayer('desktop', { ...DESKTOP, desk: null }, 'pane', [
	'dayview: add a tab to the dayview pane',
	'dayview: manage the dayview pane',
	'dayview: manage the dayview pane › page: Bar'
]);
