# Parity audit

Notes comparing daydumb against the original datum
(`/home/cans/gt/daydumb/reference/datum-v2`), one file per surface:
[commands](commands.md), [parsing](parsing.md), [sync](sync.md),
[views](views.md).

Reference images live here too:

- `nowview_with_history_reference.png` — the original datum's
  `nowview --history 15m` at 400×300, captured by vividn on 2026-09-09 and
  the picture da-nk84 rebuilt daydumb's nowview against: lanes filled
  edge to edge, no legend, and a chunk for each field's current state
  right of the now-line.
- `nowview_with_history_daydumb.png` — daydumb's own
  `nowview --history 15m --width 400 --height 300` after da-nk84, for the
  same comparison. Lane placement differs because the fields differ: both
  renderers put a field's lane at a hashed `y` with a hashed height unless
  its spec names one (`allFieldsSvg.ts` in the original,
  `Presentation::resolve_position` here), so which lanes touch and which leave black
  between them is a property of the field names, not of the drawing. The
  reference's thin lighter strip inside the blue lane is one such field —
  a spec'd `height`, overlapping its neighbour — not a mark inside another
  field's lane.
