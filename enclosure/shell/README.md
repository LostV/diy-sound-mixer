# Shell

The final enclosure: a wedge with the four knobs and the mic-mute button on a
flat deck, and the CrowPanel 5" screen in the sloped face behind it. The guide (in
Norwegian) is [docs/skall.md](../../docs/skall.md); part measurements are in
[docs/maal-deler.md](../../docs/maal-deler.md).

| File | Part |
|------|------|
| `shell.stl` / `.step` | The body, open at the bottom and the back. |
| `bottom.stl` / `.step` | The bottom plate and the back wall in one L, with posts for the Pico. Off, the open back gives a straight path to the screen's upper posts. Prints on the plate. |
| `ring.stl` / `.step` | Colour ring under a knob. One print job per colour. |
| `screen-frame.stl` / `.step` | Flat piece of the screen face (glass cut-out and posts) to try the screen before printing the shell. Prints face down. |
| `assembly.step` | Everything put together, with dummy screen, knobs, encoders and Pico. For viewing only. |

The button bar is the one from the test plate (`enclosure/testplate/button-bar`).

The screen numbers come from Elecrow's drawing of the DLE06050B board
(`5inch_ESP32S3_Size.pdf` on the product page). A caliper check of the real
screen matched within 1 mm; the screen frame confirms the 0.2 mm glass fit and
the hole spacing before the shell is printed.

Rebuild after changing a measurement in `shell.py` (FreeCAD 1.x, run from the
repository root). The script also checks that the screen, the USB-C plug, the
encoders, the button bar and the Pico stay clear of the shell:

```
freecadcmd -c "exec(open('enclosure/shell/shell.py').read())"
```
