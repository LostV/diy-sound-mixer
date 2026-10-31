# Test plate

A small plate that checks the fit of the encoders, the mic-mute LED button and
M3 heat-set inserts before the full enclosure is drawn. Part measurements are
in [docs/maal-deler.md](../../docs/maal-deler.md); the print and test guide (in
Norwegian) is [docs/test-2-testplate.md](../../docs/test-2-testplate.md).

| File | Part |
|------|------|
| `testplate.stl` / `.step` | The plate. Prints face down, no supports. |
| `button-bar.stl` / `.step` | Bar that holds the LED button from behind with two M3 screws. |

Rebuild after changing a measurement in `testplate.py` (FreeCAD 1.x, run from
the repository root):

```
freecadcmd -c "exec(open('enclosure/testplate/testplate.py').read())"
```
