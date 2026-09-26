// Every button on the shipped desktop page, tapped (da-y0ocz): a double
// outline is exactly a tap that wrote or ran. The how is `support/taps.mjs`.

import { everyButton } from './support/taps.mjs';
import { DESKTOP } from './support/layout.mjs';

// The shipped tree, not the suite's DESK: it has the commands pane.
everyButton('desktop', { ...DESKTOP, desk: null }, ['fields: coffee', 'fields: run', 'command: undo']);
