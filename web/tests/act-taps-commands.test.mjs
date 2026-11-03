// Every button on a desktop page that holds a commands pane, tapped
// (da-0o5z9). The shipped trees close on one again (da-h3jcu), but this
// page is the one the sweep was written for. The how is `support/taps.mjs`.

import { everyButton } from './support/taps.mjs';
import { DESKTOP, along, widgetPane } from './support/layout.mjs';

// The CLI, the fields and the commands, as `main`'s desktop tree.
const desk = along('column', widgetPane('repl', 'cli'), widgetPane('bar', 'widgetbar'), widgetPane('commands', 'command'));

everyButton('commands-pane desktop', { ...DESKTOP, desk }, ['command: undo']);
