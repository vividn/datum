// Every button on the shipped desktop page, tapped (da-y0ocz): a double
// outline is exactly a tap that wrote or ran. The how is `support/taps.mjs`.

import { everyButton } from './support/taps.mjs';
import { DESKTOP } from './support/layout.mjs';

// The shipped tree, not the suite's DESK. It has had no commands pane since
// da-mwp4m, so `command: undo` left the list; a sweep of the commands
// widget on a page that holds one is da-0o5z9's.
everyButton('desktop', { ...DESKTOP, desk: null }, ['fields: coffee', 'fields: run']);
