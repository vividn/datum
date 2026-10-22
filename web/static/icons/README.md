# The app's icons

`source.png` is the original and the only file here anyone edits. vividn made
it on **2026-09-15** — a snapshot of his own data, drawn by datum — and it is
in the repo byte for byte as he made it: 250×250, opaque. He may replace it.

Everything else under this directory, and `../favicon.png`, is derived. To
remake them all, overwrite `source.png` and run, from `web/`:

```sh
python3 scripts/build-icons.py
```

That script is the pipeline in full, and it is the documentation of it too —
what each shape is for is written there. Pillow is all it needs, and it is
deliberately not a project dependency: nothing in the build or the test suite
runs it. The outputs are committed.

| file | size | purpose |
| --- | --- | --- |
| `icon-192.png` | 192×192 | manifest, `any` |
| `icon-512.png` | 512×512 | manifest, `any` |
| `icon-192-maskable.png` | 192×192 | manifest, `maskable` |
| `icon-512-maskable.png` | 512×512 | manifest, `maskable` |
| `apple-touch-icon.png` | 180×180 | iOS home screen, from `app.html` |
| `../favicon.png` | 64×64 | the browser tab |

Two things to know about them:

- **512 is an upscale.** The source is 250 across, so `icon-512.png` and
  `icon-512-maskable.png` are enlargements — resampled smoothly, but they
  carry no detail the source did not have. A larger source from vividn would
  need no change other than the copy and the run above.
- **The maskable pair is padded.** The artwork sits in the center 80% and the
  margin is the manifest's `background_color`, `#0a0a1a`. Android masks the
  full square to a shape of its own choosing, and the padding is what keeps
  it from cutting into the picture. `build-icons.py` holds the same color as
  a constant; if the manifest's background changes, change it there too.

There is no `favicon.ico`. Every browser the app supports reads a PNG icon,
and nothing has asked for one.
