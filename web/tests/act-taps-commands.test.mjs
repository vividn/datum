// Every button on a desktop page that holds a commands pane, tapped
// (da-0o5z9): the shipped trees have had none since da-mwp4m, so no other
// sweep reaches the commands widget. The how is `support/taps.mjs`.

import { everyButton } from './support/taps.mjs';
import { DESKTOP, along, widgetPane } from './support/layout.mjs';

// The CLI, the fields and the commands, as `main`'s desktop tree.
const desk = along('column', widgetPane('repl', 'cli'), widgetPane('bar', 'widgetbar'), widgetPane('commands', 'command'));

everyButton('commands-pane desktop', { ...DESKTOP, desk }, ['fields: coffee', 'fields: run', 'command: undo']);
