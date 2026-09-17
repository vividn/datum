// Is this click a TAP on an entry (da-r13j), or the end of something else?
//
// A drag that selected text ends in a click too, and a reader copying a
// hid must not select the entry on the way. A shift-click is a range, and
// its mousedown is stopped from extending the text selection, so an old
// one may still be there: shift is always a tap.

export const tapped = (e: MouseEvent): boolean =>
	e.shiftKey || (globalThis.getSelection?.()?.isCollapsed ?? true);
