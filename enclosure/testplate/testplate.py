"""Test plate for the knob panel: two encoders, the mic-mute button and a row
of heat-set insert holes. Measurements come from docs/maal-deler.md.

Coordinates: the front face lies on z = 0 and the back points up (+z), so
the plate prints face down without supports. Units are mm.

Build with FreeCAD 1.x:
    freecadcmd -c "exec(open('enclosure/testplate/testplate.py').read())"
Writes STEP and STL files next to this script.
"""

import os

import FreeCAD as App
import Mesh
import Part

V = App.Vector
OUT = os.path.dirname(os.path.abspath(__file__)) if "__file__" in globals() else os.path.join(os.getcwd(), "enclosure", "testplate")

# Plate
PLATE_W, PLATE_D, PLATE_T = 125.0, 55.0, 2.0
CORNER_R = 4.0
ROW_Y = PLATE_D / 2

# Encoder (E1-E5). The plate is clamped between the body and the nut, so it
# must stay under E2 - E4 = 2.2 mm thick.
ENC_X = (20.0, 60.0)  # 40 mm apart: 10 mm between two 30 mm knobs
ENC_HOLE = 6.84 + 0.4  # E1 + clearance
ENC_BODY = (12.7, 13.5)  # E3: x along the sides with the thin legs
ENC_FIT = 0.3  # play around the body in the pocket
POCKET_WALL, POCKET_H = 1.6, 4.0  # body is 6.5 high, so the legs stay free

# Mic-mute LED button (L1-L14)
BTN_X = 95.0
BTN_FACE_T = 1.0  # plate is thinned here so the cap stands 2.3 mm proud
BTN_COLLAR_HOLE = 8.26 + 0.4  # L11 + clearance
BTN_BODY = 12.0 + 0.3  # L1/L2 + fit
BTN_HEIGHT = 3.1 + 0.7  # L3 + L14: back of the body to the top of the white part
BTN_TAB_SLOT = 3.0  # gap in the pocket walls for the tabs and LED legs (L6, L7)

# Retaining bar behind the button, held by two M3 screws in heat-set inserts.
# The legs leave the body at its edges (switch legs 12.5 x 5 mm apart, LED legs
# at the tab tips), so the bar bridges over them and a pillar in the middle of
# the body holds the button. For testing the legs carry female dupont plugs,
# so the bar clears their tops and is narrow where their wires pass beside it.
# Spacer tubes on the bar stand on the posts, so the first plate still fits.
BTN_BACK = BTN_FACE_T + BTN_HEIGHT  # back of the button body above the front face
DUPONT_H, DUPONT_ROOM = 14.5, 1.5  # plug housing length, and margin above it
POST_DY = 12.0  # posts at BTN_X, ROW_Y +- POST_DY
POST_D, POST_TOP = 8.0, 7.0  # post top height above the front face
BRIDGE_Z = BTN_BACK + DUPONT_H + DUPONT_ROOM  # underside of the bar
INSERT_HOLE = 4.9  # best of the ladder for the 5 mm inserts (G1, G2)
BAR_W, BAR_L, BAR_T = 8.0, 32.0, 4.0
BAR_NARROW = 2.4  # middle part, between the switch legs 5 mm apart
PILLAR = 6.0  # square, inside the body outline, clear of all legs
SCREW_HOLE = 3.4
SCREW_HEAD_D, SCREW_HEAD_SINK = 6.2, 3.0  # counterbore so an M3x20 reaches the insert

# Insert ladder: same post, four hole sizes, smallest at the front edge
LADDER_X = 113.0
LADDER_Y = (9.0, 21.0, 33.0, 45.0)
LADDER_HOLES = (4.0, 4.3, 4.6, 4.9)
INSERT_DEPTH = POST_TOP - 1.0  # leaves a 1 mm skin on the front face


def box(x0, y0, z0, dx, dy, dz):
    return Part.makeBox(dx, dy, dz, V(x0, y0, z0))


def centered_box(cx, cy, z0, dx, dy, dz):
    return box(cx - dx / 2, cy - dy / 2, z0, dx, dy, dz)


def cylinder(cx, cy, z0, d, h):
    return Part.makeCylinder(d / 2, h, V(cx, cy, z0))


def rounded_plate():
    plate = box(0, 0, 0, PLATE_W, PLATE_D, PLATE_T)
    vertical = [e for e in plate.Edges if abs(e.Vertexes[0].Z - e.Vertexes[1].Z) > 1e-6]
    return plate.makeFillet(CORNER_R, vertical)


def ring_pocket(cx, cy, inner_x, inner_y, z0, h):
    outer = centered_box(cx, cy, z0, inner_x + 2 * POCKET_WALL, inner_y + 2 * POCKET_WALL, h)
    return outer.cut(centered_box(cx, cy, z0, inner_x, inner_y, h))


def insert_post(cx, cy, hole):
    post = cylinder(cx, cy, PLATE_T, POST_D, POST_TOP - PLATE_T)
    return post, cylinder(cx, cy, POST_TOP - INSERT_DEPTH, hole, INSERT_DEPTH)


def build_plate():
    plate = rounded_plate()
    adds, cuts = [], []

    for x in ENC_X:
        adds.append(ring_pocket(x, ROW_Y, ENC_BODY[0] + ENC_FIT, ENC_BODY[1] + ENC_FIT, PLATE_T, POCKET_H))
        cuts.append(cylinder(x, ROW_Y, 0, ENC_HOLE, PLATE_T))

    # Button: walls around the body, open on both tab sides.
    walls = ring_pocket(BTN_X, ROW_Y, BTN_BODY, BTN_BODY, PLATE_T, BTN_BACK - PLATE_T)
    walls = walls.cut(centered_box(BTN_X, ROW_Y, 0, BTN_BODY + 4 * POCKET_WALL, BTN_TAB_SLOT, 20))
    adds.append(walls)
    tabs_w = 12.0 + 2 * 1.5 + 0.6
    cuts.append(centered_box(BTN_X, ROW_Y, BTN_FACE_T, BTN_BODY, BTN_BODY, PLATE_T))
    cuts.append(centered_box(BTN_X, ROW_Y, BTN_FACE_T, tabs_w, BTN_TAB_SLOT, PLATE_T))
    cuts.append(cylinder(BTN_X, ROW_Y, 0, BTN_COLLAR_HOLE, PLATE_T))

    for dy in (-POST_DY, POST_DY):
        post, hole = insert_post(BTN_X, ROW_Y + dy, INSERT_HOLE)
        adds.append(post)
        cuts.append(hole)

    for y, d in zip(LADDER_Y, LADDER_HOLES):
        post, hole = insert_post(LADDER_X, y, d)
        adds.append(post)
        cuts.append(hole)

    for a in adds:
        plate = plate.fuse(a)
    for c in cuts:
        plate = plate.cut(c)
    return plate.removeSplitter()


def build_bar():
    """Bar that holds the button from behind. Printed flat, pillar and spacers
    up; in use they point down, the spacers onto the posts and the pillar onto
    the back of the button."""
    spacer_h = BRIDGE_Z - POST_TOP
    # Wide only over the spacer tubes, from the inner edge of a tube to the end.
    end_from, end_to = POST_DY - POST_D / 2, BAR_L / 2
    bar = centered_box(0, 0, 0, BAR_NARROW, BAR_L, BAR_T)
    for sign in (-1, 1):
        bar = bar.fuse(centered_box(0, sign * (end_from + end_to) / 2, 0, BAR_W, end_to - end_from, BAR_T))
    bar = bar.fuse(centered_box(0, 0, BAR_T, PILLAR, PILLAR, BRIDGE_Z - BTN_BACK))
    for dy in (-POST_DY, POST_DY):
        bar = bar.fuse(cylinder(0, dy, BAR_T, POST_D, spacer_h))
        bar = bar.cut(cylinder(0, dy, 0, SCREW_HOLE, BAR_T + spacer_h))
        bar = bar.cut(cylinder(0, dy, 0, SCREW_HEAD_D, SCREW_HEAD_SINK))
    return bar.removeSplitter()


def export(shape, name):
    shape.exportStep(os.path.join(OUT, name + ".step"))
    mesh = Mesh.Mesh(shape.tessellate(0.02))
    mesh.write(os.path.join(OUT, name + ".stl"))
    bb = shape.BoundBox
    print(f"{name}: {bb.XLength:.1f} x {bb.YLength:.1f} x {bb.ZLength:.1f} mm, "
          f"volume {shape.Volume / 1000:.1f} cm3, valid={shape.isValid()}")


export(build_plate(), "testplate")
export(build_bar(), "button-bar")
