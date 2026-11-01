# Shell

The final enclosure: a wedge with the four knobs and the mic-mute button on a
sloped deck, and the CrowPanel 5" screen in the face behind it. The guide (in
Norwegian) is [docs/skall.md](../../docs/skall.md); part measurements are in
[docs/maal-deler.md](../../docs/maal-deler.md).

| File | Part |
|------|------|
| `shell.stl` / `.step` | The body, open at the bottom. Prints standing on its back. |
| `bottom.stl` / `.step` | Bottom plate with posts for the Pico. Prints flat. |
| `ring.stl` / `.step` | Colour ring under a knob. One print job per colour. |
| `assembly.step` | Everything put together, with dummy screen, knobs, encoders and Pico. For viewing only. |

The button bar is the one from the test plate (`enclosure/testplate/button-bar`).

The screen numbers come from Elecrow's drawing of the DLE06050B board
(`5inch_ESP32S3_Size.pdf` on the product page). Check them with a caliper when
the screen arrives: glass size, glass front to PCB front, and hole spacing.

Rebuild after changing a measurement in `shell.py` (FreeCAD 1.x, run from the
repository root). The script also checks that the screen, the USB-C plug, the
encoders, the button bar and the Pico stay clear of the shell:

```
freecadcmd -c "exec(open('enclosure/shell/shell.py').read())"
```
