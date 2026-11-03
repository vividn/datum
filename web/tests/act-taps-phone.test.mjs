// Every button on the shipped phone page, tapped (da-y0ocz): a double
// outline is exactly a tap that wrote or ran. The how is `support/taps.mjs`.

import { everyButton } from './support/taps.mjs';
import { PHONE } from './support/layout.mjs';

// The phone's commands pane is cut at this height, so it is not asked for.
// At rest a field's tap opens it and acts on nothing (da-w6437): the `occur `
// survey is where `collect: coffee` has to act.
everyButton('phone', PHONE, []);
