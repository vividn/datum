#!/usr/bin/env python3
"""Derive the app's icon set from `static/icons/source.png`.

The source is vividn's, and it is the only file here anyone edits: every
other PNG under `static/icons/` (and `static/favicon.png`) is output, and
this script is the whole pipeline that makes them. When a new source
arrives it is one copy over `source.png` and one run of this.

    python3 scripts/build-icons.py

Pillow is the only requirement and it is not a project dependency — it is
not in `web/package.json` and nothing in the build or the test suite runs
this. Icons are committed; this exists so that they can be remade.

The three shapes an installed app needs:

  * `any`       — the source scaled, edge to edge. What a launcher that
                  does no masking of its own shows.
  * `maskable`  — the source inside the centre 80%, the rest padded in the
                  manifest's `background_color`. Android applies a mask of
                  its own choosing (circle, squircle, teardrop) to the full
                  square, so anything outside that safe zone is liable to
                  be cut off.
  * apple-touch — 180x180 and opaque. iOS composites the icon onto black
                  rather than onto the page, so transparency reads as a
                  hole.
"""

from pathlib import Path

from PIL import Image

ICONS = Path(__file__).resolve().parent.parent / "static" / "icons"
STATIC = ICONS.parent

# The manifest's `background_color`. Two files have to agree on it: keep
# this equal to `background_color` in static/manifest.json.
BACKGROUND = (0x0A, 0x0A, 0x1A)

# The fraction of a maskable icon's side the artwork is allowed to fill.
SAFE_ZONE = 0.8


def scaled(source: Image.Image, side: int) -> Image.Image:
    """`source` at `side`x`side`, resampled smoothly.

    LANCZOS rather than the default NEAREST because 512 is an UPSCALE of a
    250px source: nearest-neighbour at 2.05x makes visibly ragged edges of
    what is otherwise a clean picture.
    """
    return source.resize((side, side), Image.Resampling.LANCZOS)


def flattened(image: Image.Image) -> Image.Image:
    """`image` over the background colour, with no alpha channel left."""
    canvas = Image.new("RGB", image.size, BACKGROUND)
    canvas.paste(image, (0, 0), image if image.mode in ("RGBA", "LA") else None)
    return canvas


def maskable(source: Image.Image, side: int) -> Image.Image:
    """`source` centred in the safe zone of a `side`x`side` icon."""
    inner = round(side * SAFE_ZONE)
    canvas = Image.new("RGB", (side, side), BACKGROUND)
    art = scaled(source, inner)
    offset = (side - inner) // 2
    canvas.paste(art, (offset, offset), art if art.mode in ("RGBA", "LA") else None)
    return canvas


def main() -> None:
    source = Image.open(ICONS / "source.png")

    scaled(source, 192).save(ICONS / "icon-192.png")
    scaled(source, 512).save(ICONS / "icon-512.png")
    maskable(source, 192).save(ICONS / "icon-192-maskable.png")
    maskable(source, 512).save(ICONS / "icon-512-maskable.png")
    flattened(scaled(source, 180)).save(ICONS / "apple-touch-icon.png")

    # The tab icon, at the root because that is where a browser guesses one
    # is and where app.html has always pointed.
    scaled(source, 64).save(STATIC / "favicon.png")


if __name__ == "__main__":
    main()
