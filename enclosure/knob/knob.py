"""Temporary printed knob for the encoder (Bourns PEC11R-4215F, 6 mm D-shaft),
the same size as the aluminium knobs on order (30 x 17 mm). Shaft
measurements are in docs/maal-deler.md (E6-E8).

The knob is modelled upside down, top face on z = 0, the way it prints: the
top comes out smooth and the bore needs no supports. Units are mm.

Build with FreeCAD 1.x from the repository root:
    freecadcmd -c "exec(open('enclosure/knob/knob.py').read())"
"""

import math
import os

import FreeCAD as App
import Mesh
import Part

V = App.Vector
OUT = os.path.join(os.getcwd(), "enclosure", "knob")

KNOB_D, KNOB_H = 30.0, 17.0
TOP_CHAMFER = 1.2

# D-bore. A 6.0 mm shaft with the flat 4.5 mm from the round side; printed
# holes come out a little small, so the bore is opened up. If the knob is too
# tight or too loose, change these two numbers.
BORE_D = 6.25
BORE_FLAT = 4.6
BORE_DEPTH = 9.0  # shaft is 9.3 above the neck (E7); keeps the knob off the bushing

# Underside: a hub around the bore and an outer wall, hollow between them
HUB_D, WALL_T, RECESS_DEPTH = 12.0, 2.4, 11.0

# Grip flutes around the side
FLUTES, FLUTE_D = 30, 2.0

# Pointer groove on the top
POINTER_W, POINTER_DEPTH = 1.2, 0.8


def body():
    knob = Part.makeCylinder(KNOB_D / 2, KNOB_H)
    top_edge = [e for e in knob.Edges if abs(e.BoundBox.ZMax) < 1e-6 and e.Curve.TypeId == "Part::GeomCircle"]
    knob = knob.makeChamfer(TOP_CHAMFER, top_edge)

    flutes = []
    for i in range(FLUTES):
        a = 2 * math.pi * i / FLUTES
        r = KNOB_D / 2 + FLUTE_D * 0.2
        flutes.append(Part.makeCylinder(FLUTE_D / 2, KNOB_H, V(r * math.cos(a), r * math.sin(a), TOP_CHAMFER)))
    for f in flutes:
        knob = knob.cut(f)

    recess = Part.makeCylinder(KNOB_D / 2 - WALL_T, RECESS_DEPTH, V(0, 0, KNOB_H - RECESS_DEPTH))
    recess = recess.cut(Part.makeCylinder(HUB_D / 2, RECESS_DEPTH, V(0, 0, KNOB_H - RECESS_DEPTH)))
    knob = knob.cut(recess)

    # D-bore from the underside: a round hole with a flat filled back in.
    bore = Part.makeCylinder(BORE_D / 2, BORE_DEPTH, V(0, 0, KNOB_H - BORE_DEPTH))
    flat_x = BORE_FLAT - BORE_D / 2  # flat side sits at +x
    bore = bore.cut(Part.makeBox(BORE_D, BORE_D, BORE_DEPTH, V(flat_x, -BORE_D / 2, KNOB_H - BORE_DEPTH)))
    knob = knob.cut(bore)

    # Pointer on the top, lined up with the flat of the shaft.
    knob = knob.cut(Part.makeBox(KNOB_D / 2, POINTER_W, POINTER_DEPTH, V(2.0, -POINTER_W / 2, 0)))
    return knob.removeSplitter()


def export(shape, name):
    shape.exportStep(os.path.join(OUT, name + ".step"))
    Mesh.Mesh(shape.tessellate(0.02)).write(os.path.join(OUT, name + ".stl"))
    bb = shape.BoundBox
    print(f"{name}: {bb.XLength:.1f} x {bb.YLength:.1f} x {bb.ZLength:.1f} mm, "
          f"volume {shape.Volume / 1000:.1f} cm3, valid={shape.isValid()}")


export(body(), "knob")
