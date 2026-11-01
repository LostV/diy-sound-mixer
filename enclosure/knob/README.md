# Temporary knob

A printed stand-in for the 30 × 17 mm aluminium knobs, for the encoder's 6 mm
D-shaft (Bourns PEC11R-4215F, Electrokit 41017819). Use it until the real
knobs arrive.

Print `knob.stl` as it opens: top face down, no supports, PLA, 0.2 mm layers.
Push it straight down onto the shaft with the flat of the bore lined up with
the flat of the shaft. The pointer groove on top lines up with the flat.

If it is too tight or too loose, change `BORE_D` or `BORE_FLAT` in `knob.py`
and rebuild from the repository root (FreeCAD 1.x):

```
freecadcmd -c "exec(open('enclosure/knob/knob.py').read())"
```
