"""Final shell for the mixer: a wedge with four knobs and the mic-mute button
on a flat deck in front, and the CrowPanel 5" screen in the face behind it.

Parts (STEP + STL next to this script):
    shell        the body, open at the bottom
    bottom       the bottom plate and the back wall in one L-shaped part, with
                 posts for the Pico. Off, it opens the back, so the screen's
                 upper screws and inserts can be reached
    ring         colour ring under a knob; print one per colour, no AMS swaps
    assembly     shell, bottom, rings and dummy parts for viewing only

The button bar is the one from the test plate (enclosure/testplate/button-bar).

Coordinates: x to the right seen from the front, y from the front edge (y = 0)
to the back, z up from the desk. Units are mm. Measurements come from
docs/maal-deler.md and Elecrow's drawing of the screen (DLE06050B,
"5inch_ESP32S3_Size.pdf").

Build with FreeCAD 1.x:
    freecadcmd -c "exec(open('enclosure/shell/shell.py').read())"
"""

import math
import os

import FreeCAD as App
import Mesh
import Part

V = App.Vector
OUT = os.path.dirname(os.path.abspath(__file__)) if "__file__" in globals() else os.path.join(os.getcwd(), "enclosure", "shell")

# Body
W = 216.0  # wide enough for a straight USB-C plug beside the screen
WALL = 2.0  # deck, screen face, front, top and back
SIDE_WALL = 2.4
FRONT_H = 32.0  # front edge height, and the deck's; the button bar needs it
# The deck is flat: a few degrees of slope print as visible layer steps.
DECK_ANGLE, DECK_L = 0.0, 56.0
FACE_ANGLE = 40.0  # screen face, from horizontal
TOP_L = 8.0  # flat strip behind the screen
ROUND_FRONT, ROUND_TOP = 5.0, 4.0  # outer rounds on the long edges
ROUND_SIDE = 2.0  # outer rounds around both side faces
ROUND_CREASE = 8.0  # soft bend from the deck up into the screen face

# Screen: Elecrow CrowPanel 5" ESP32-S3 IPS (DLE06050B). Front of the glass to
# the front of the PCB is 5.7; the board is 137 x 84 with M3 holes 130.03 x
# 76.95 apart, and the glass (touch + LCD) is 120.7 x 75.9, centred on it.
PCB_W, PCB_H, PCB_T = 137.0, 84.0, 1.6
HOLE_DX, HOLE_DY = 130.03, 76.95
GLASS_W, GLASS_H = 120.7, 75.9
GLASS_TO_PCB = 5.7
BACK_PARTS = 6.45  # tallest parts behind the PCB
GLASS_FIT = 0.2  # gap around the glass in the cut-out
GLASS_SINK = 0.6  # glass sits this far below the face
GLASS_MARGIN = 6.0  # face from the deck crease and the top edge to the glass
USB_U = 42.0  # USB-C on the left short edge, halfway up, on the back side
SCREEN_POST_D = 7.0

# Knobs and button on the deck, along the deck from its front edge
ROW_S = 26.0
KNOB_X = (-77.0, -35.0, 7.0, 49.0)  # 42 apart: 12 mm between 30 mm knobs
MIC_X = 87.0

# Encoder (E1-E5): the deck is clamped between the body and the nut, so deck
# plus ring must stay under E2 - E4 = 2.2 mm.
ENC_HOLE = 6.84 + 0.4
ENC_BODY = (12.7, 13.5)  # x along the sides with the thin legs
ENC_FIT = 0.3
POCKET_WALL, POCKET_H = 1.6, 4.0

# Colour ring: sits flush in a recess around each knob, clamped by the nut
RING_OD, RING_T = 37.0, 0.8
RING_FIT = 0.4

# Mic-mute LED button (L1-L14), as on the test plate
BTN_FACE_T = 1.0
BTN_COLLAR_HOLE = 8.26 + 0.4
BTN_BODY = 12.0 + 0.3
BTN_HEIGHT = 3.1 + 0.7
BTN_TAB_SLOT = 3.0
BTN_BACK = BTN_FACE_T + BTN_HEIGHT
POST_DY = 12.0
POST_D, POST_TOP = 8.0, 7.0
BAR_DEPTH = BTN_BACK + 14.5 + 1.5 + 4.0  # lowest point of the test plate bar

# Heat-set inserts (G2: M3, 5.0 OD, 4 long)
INSERT_HOLE = 4.9
INSERT_DEPTH = 5.0  # insert + 1 mm

# Bottom part: the bottom plate and the back wall in one L. The plate sits
# inside the walls, flush with their lower edge; the back wall goes up to a
# skirt under the shell's rounded top edge, behind tabs that hold it in line.
PLATE_T = 4.0
PLATE_FIT = 0.3
BACK_T = 2.0  # back wall of the L; the screen's back parts come within 0.2 mm
SKIRT_H = 8.0  # the shell keeps this much of the back, under the top edge
BACK_GAP = 0.2  # between the L's back wall and the skirt
TAB_W, TAB_T, TAB_H = 20.0, 1.6, 6.0
TAB_X = (-88.0, 88.0)  # beside the screen, clear of the tool paths to its upper posts
BOSS_D, BOSS_H = 9.0, 10.0
BOSS_INSET = 3.5  # boss centre from the inner walls; the boss runs 1 mm into the side wall
BACK_BOSS_GAP = 0.5  # the back bosses stand this far in front of the L's back wall
BOSS_INSERT_DEPTH = 6.0
SCREW_HOLE = 3.4
SCREW_HEAD_D, SCREW_HEAD_SINK = 6.5, 3.0  # M3x6 socket head sits flush

# Raspberry Pi Pico 2 WH, upside down so the header pins point up
PICO_L, PICO_W = 51.0, 21.0
PICO_HOLES = (47.0, 11.4)
PICO_X, PICO_Y = 15.0, 108.0  # centre; long side along x, USB end to the right
PICO_POST_D, PICO_POST_H = 5.0, 5.0
PICO_SCREW_HOLE = 1.8  # M2 screw cuts its own thread

# Cable slots in the corner of the L, open below, so the cables go in from
# underneath
CABLE_SLOTS = (-85.0, 70.0)  # screen USB-C on the left, Pico on the right
CABLE_W, CABLE_H, CABLE_IN = 7.0, 10.0, 7.0  # up the back wall, into the plate

# Profile corners (y, z)
_da, _fa = math.radians(DECK_ANGLE), math.radians(FACE_ANGLE)
FACE_L = GLASS_MARGIN + GLASS_H + GLASS_MARGIN
P_FRONT = (0.0, FRONT_H)
P_CREASE = (DECK_L * math.cos(_da), FRONT_H + DECK_L * math.sin(_da))
P_RIDGE = (P_CREASE[0] + FACE_L * math.cos(_fa), P_CREASE[1] + FACE_L * math.sin(_fa))
DEPTH = P_RIDGE[0] + TOP_L
HEIGHT = P_RIDGE[1]
PROFILE = [(0.0, 0.0), P_FRONT, P_CREASE, P_RIDGE, (DEPTH, HEIGHT), (DEPTH, 0.0)]
ROUNDS = {P_FRONT: ROUND_FRONT, P_CREASE: ROUND_CREASE, P_RIDGE: ROUND_TOP, (DEPTH, HEIGHT): ROUND_TOP}


def box(x0, y0, z0, dx, dy, dz):
    return Part.makeBox(dx, dy, dz, V(x0, y0, z0))


def centered_box(cx, cy, z0, dx, dy, dz):
    return box(cx - dx / 2, cy - dy / 2, z0, dx, dy, dz)


def cylinder(cx, cy, z0, d, h):
    return Part.makeCylinder(d / 2, h, V(cx, cy, z0))


def prism(points, x0, x1):
    wire = Part.makePolygon([V(x0, y, z) for y, z in points] + [V(x0, *points[0])])
    return Part.Face(wire).extrude(V(x1 - x0, 0, 0))


def round_long_edges(solid, corners):
    """Round the edges along x that pass through the given (y, z) corners."""
    edges = []
    for (y, z), r in corners.items():
        for e in solid.Edges:
            a, b = e.Vertexes[0].Point, e.Vertexes[-1].Point
            if abs(a.y - b.y) < 1e-6 and abs(a.z - b.z) < 1e-6 and math.hypot(a.y - y, a.z - z) < 0.5:
                edges.append((e, r))
    for e, r in edges:
        solid = solid.makeFillet(r, [solid.Edges[[i for i, f in enumerate(solid.Edges) if f.isSame(e)][0]]])
    return solid


# Features on the deck and the screen face are drawn flat in a local frame:
# x as above, y along the surface (up the slope), z out of the surface. Depth
# into the shell is negative z.
def deck_frame():
    return App.Placement(V(0, *P_FRONT), App.Rotation(V(1, 0, 0), DECK_ANGLE))


def face_frame():
    return App.Placement(V(0, *P_CREASE), App.Rotation(V(1, 0, 0), FACE_ANGLE))


def placed(shape, frame):
    s = shape.copy()
    s.Placement = frame.multiply(s.Placement)
    return s


def down_cyl(x, y, d, depth0, depth1):
    """Cylinder into the surface, from depth0 to depth1 below it."""
    return cylinder(x, y, -depth1, d, depth1 - depth0)


def down_box(x, y, dx, dy, depth0, depth1):
    return centered_box(x, y, -depth1, dx, dy, depth1 - depth0)


def frame_ring(x, y, inner_x, inner_y, depth0, depth1):
    outer = down_box(x, y, inner_x + 2 * POCKET_WALL, inner_y + 2 * POCKET_WALL, depth0, depth1)
    return outer.cut(down_box(x, y, inner_x, inner_y, depth0 - 0.1, depth1 + 0.1))


def deck_features():
    adds, cuts = [], []
    for x in KNOB_X:
        adds.append(frame_ring(x, ROW_S, ENC_BODY[0] + ENC_FIT, ENC_BODY[1] + ENC_FIT, WALL - 0.1, WALL + POCKET_H))
        cuts.append(down_cyl(x, ROW_S, ENC_HOLE, -1, WALL + 1))
        cuts.append(down_cyl(x, ROW_S, RING_OD + RING_FIT, -1, RING_T))

    # Button: walls around the body, open on both tab sides; the deck is
    # thinned to BTN_FACE_T so the cap stands proud as on the test plate.
    walls = frame_ring(MIC_X, ROW_S, BTN_BODY, BTN_BODY, WALL - 0.1, BTN_BACK)
    walls = walls.cut(down_box(MIC_X, ROW_S, BTN_BODY + 4 * POCKET_WALL, BTN_TAB_SLOT, 0, 30))
    adds.append(walls)
    tabs_w = 12.0 + 2 * 1.5 + 0.6
    cuts.append(down_box(MIC_X, ROW_S, BTN_BODY, BTN_BODY, BTN_FACE_T, WALL + 0.1))
    cuts.append(down_box(MIC_X, ROW_S, tabs_w, BTN_TAB_SLOT, BTN_FACE_T, WALL + 0.1))
    cuts.append(down_cyl(MIC_X, ROW_S, BTN_COLLAR_HOLE, -1, WALL + 1))
    for dy in (-POST_DY, POST_DY):
        adds.append(down_cyl(MIC_X, ROW_S + dy, POST_D, WALL - 0.1, POST_TOP))
        cuts.append(down_cyl(MIC_X, ROW_S + dy, INSERT_HOLE, POST_TOP - (POST_TOP - 1.0), POST_TOP + 0.1))
    return [placed(a, deck_frame()) for a in adds], [placed(c, deck_frame()) for c in cuts]


GLASS_U = GLASS_MARGIN  # glass bottom edge, up the face from the crease
PCB_U = GLASS_U + GLASS_H / 2  # centre of the board up the face
PCB_DEPTH = GLASS_SINK + GLASS_TO_PCB  # face to the front of the PCB


def screen_holes():
    return [(sx * HOLE_DX / 2, PCB_U + sy * HOLE_DY / 2) for sx in (-1, 1) for sy in (-1, 1)]


def face_features():
    adds, cuts = [], []
    cuts.append(down_box(0, PCB_U, GLASS_W + 2 * GLASS_FIT, GLASS_H + 2 * GLASS_FIT, -1, WALL + 1))
    for x, y in screen_holes():
        adds.append(down_cyl(x, y, SCREEN_POST_D, WALL - 0.1, PCB_DEPTH))
        cuts.append(down_cyl(x, y, INSERT_HOLE, PCB_DEPTH - INSERT_DEPTH, PCB_DEPTH + 0.1))
    return [placed(a, face_frame()) for a in adds], [placed(c, face_frame()) for c in cuts]


def inner_profile():
    face = Part.Face(Part.makePolygon([V(0, y, z) for y, z in PROFILE] + [V(0, *PROFILE[0])]))
    return face.makeOffset2D(-WALL, join=2)


SPLIT_Z = HEIGHT - SKIRT_H  # the L's back wall reaches up to here
BACK_IN = DEPTH - BACK_T - PLATE_FIT  # inner face of the L's back wall


def boss_positions():
    xi = W / 2 - SIDE_WALL - BOSS_INSET
    back = BACK_IN - BACK_BOSS_GAP - BOSS_D / 2
    return [(sx * xi, y) for sx in (-1, 1) for y in (WALL + BOSS_INSET, back)]


def build_shell():
    outer = round_long_edges(prism(PROFILE, -W / 2, W / 2), ROUNDS)
    try:
        side_edges = [e for e in outer.Edges if abs(abs(e.BoundBox.XMin) - W / 2) < 1e-6 and abs(e.BoundBox.XLength) < 1e-6]
        outer = outer.makeFillet(ROUND_SIDE, side_edges)
    except Part.OCCError as err:
        print("side rounds skipped:", err)

    inner_face = inner_profile()
    inner_face.translate(V(-W / 2 + SIDE_WALL, 0, 0))
    inner = inner_face.extrude(V(W - 2 * SIDE_WALL, 0, 0))
    inner_rounds = {}
    for (y, z), r in ROUNDS.items():
        best = min(inner.Vertexes, key=lambda v: math.hypot(v.Point.y - y, v.Point.z - z))
        # The crease bends inwards, so its inside round is the larger one.
        inner_rounds[(best.Point.y, best.Point.z)] = r + WALL if (y, z) == P_CREASE else max(r - WALL, 0.5)
    inner = round_long_edges(inner, inner_rounds)
    shell = outer.cut(inner)
    shell = shell.cut(box(-W / 2 + SIDE_WALL, WALL, -1, W - 2 * SIDE_WALL, DEPTH - 2 * WALL, WALL + 1.1))
    # Open back between the side walls, up to the skirt; the L closes it.
    shell = shell.cut(box(-W / 2 + SIDE_WALL, DEPTH - WALL - 0.1, -1, W - 2 * SIDE_WALL, WALL + 1.1, SPLIT_Z + 1))

    deck_adds, deck_cuts = deck_features()
    face_adds, face_cuts = face_features()
    adds, cuts = deck_adds + face_adds, deck_cuts + face_cuts
    for x, y in boss_positions():
        adds.append(cylinder(x, y, PLATE_T, BOSS_D, BOSS_H))
        cuts.append(cylinder(x, y, PLATE_T - 0.1, INSERT_HOLE, BOSS_INSERT_DEPTH + 0.1))
    for x in TAB_X:
        # A tongue just inside the L's back wall, hung from a web that joins
        # the skirt and the top above the wall.
        adds.append(centered_box(x, BACK_IN - TAB_T / 2, SPLIT_Z - TAB_H, TAB_W, TAB_T, TAB_H + 0.1))
        web_t = DEPTH - WALL + 0.5 - (BACK_IN - TAB_T)
        adds.append(centered_box(x, BACK_IN - TAB_T + web_t / 2, SPLIT_Z, TAB_W, web_t, SKIRT_H - WALL + 0.5))

    for a in adds:
        shell = shell.fuse(a)
    for c in cuts:
        shell = shell.cut(c)
    return shell.removeSplitter()


def build_bottom():
    x0 = -W / 2 + SIDE_WALL + PLATE_FIT
    plate = box(x0, WALL + PLATE_FIT, 0, -2 * x0, BACK_IN - WALL - PLATE_FIT, PLATE_T)
    plate = plate.fuse(box(x0, BACK_IN, 0, -2 * x0, BACK_T + PLATE_FIT, SPLIT_Z - BACK_GAP))
    for x in CABLE_SLOTS:
        plate = plate.cut(centered_box(x, BACK_IN + (BACK_T + PLATE_FIT) / 2 - CABLE_IN / 2, -1,
                                       CABLE_W, BACK_T + PLATE_FIT + CABLE_IN + 1, CABLE_H + 1))
    for x, y in boss_positions():
        plate = plate.cut(cylinder(x, y, -1, SCREW_HOLE, PLATE_T + 2))
        plate = plate.cut(cylinder(x, y, -1, SCREW_HEAD_D, SCREW_HEAD_SINK + 1))
    for sx in (-1, 1):
        for sy in (-1, 1):
            x, y = PICO_X + sx * PICO_HOLES[0] / 2, PICO_Y + sy * PICO_HOLES[1] / 2
            plate = plate.fuse(cylinder(x, y, PLATE_T - 0.1, PICO_POST_D, PICO_POST_H + 0.1))
            plate = plate.cut(cylinder(x, y, 1.0, PICO_SCREW_HOLE, PLATE_T + PICO_POST_H))
    return plate.removeSplitter()


def build_ring():
    ring = cylinder(0, 0, 0, RING_OD, RING_T)
    return ring.cut(cylinder(0, 0, -1, ENC_HOLE, RING_T + 2))


def build_screen_frame():
    """Flat piece of the screen face with the glass cut-out and the four posts,
    to try the real screen before printing the shell. Prints face down."""
    frame = down_box(0, PCB_U, PCB_W + 8, PCB_H + 8, 0, WALL)
    adds, cuts = face_features()
    inv = face_frame().inverse()
    for a in adds:
        frame = frame.fuse(placed(a, inv))
    for c in cuts:
        frame = frame.cut(placed(c, inv))
    frame.rotate(V(0, PCB_U, 0), V(1, 0, 0), 180)
    return frame.removeSplitter()


# Dummy parts: what the real parts take up, to check that nothing hits the
# shell and to show the assembly. Not for printing.
def dummy_screen():
    glass = down_box(0, PCB_U, GLASS_W, GLASS_H, GLASS_SINK, GLASS_SINK + 4.2)
    pcb = down_box(0, PCB_U, PCB_W, PCB_H, PCB_DEPTH, PCB_DEPTH + PCB_T)
    parts = down_box(0, PCB_U, PCB_W - 6, PCB_H - 6, PCB_DEPTH + PCB_T, PCB_DEPTH + PCB_T + BACK_PARTS)
    return placed(glass.fuse(pcb).fuse(parts), face_frame())


def dummy_usb_plug():
    """Straight USB-C plug in the left edge of the screen, with room to bend."""
    plug = down_box(-PCB_W / 2 - 13.0, GLASS_U - (PCB_H - GLASS_H) / 2 + USB_U, 30.0, 12.0,
                    PCB_DEPTH + PCB_T - 0.5, PCB_DEPTH + PCB_T + 7.0)
    return placed(plug, face_frame())


def dummy_encoders():
    shapes = []
    for x in KNOB_X:
        body = down_box(x, ROW_S, ENC_BODY[0], ENC_BODY[1], WALL, WALL + 6.5)
        wires = down_box(x, ROW_S, 14.0, 14.0, WALL + 6.5, WALL + 6.5 + 10.0)  # legs, solder, wire bend
        shapes.append(placed(body.fuse(wires), deck_frame()))
    bar = down_box(MIC_X, ROW_S, 8.0, 2 * POST_DY + POST_D, POST_TOP, BAR_DEPTH)
    shapes.append(placed(bar, deck_frame()))
    return shapes


def dummy_knobs():
    knobs = []
    for x in KNOB_X:
        knobs.append(placed(cylinder(x, ROW_S, 3.0, 30.0, 17.0), deck_frame()))
        knobs.append(placed(cylinder(x, ROW_S, 0, 11.0, 3.0), deck_frame()))  # nut
    knobs.append(placed(cylinder(MIC_X, ROW_S, -0.1, 6.9, 2.4), deck_frame()))
    return knobs


def dummy_pico():
    board = centered_box(PICO_X, PICO_Y, PLATE_T + PICO_POST_H, PICO_L, PICO_W, 1.0)
    pins = centered_box(PICO_X, PICO_Y, PLATE_T + PICO_POST_H + 1.0, PICO_L - 2, PICO_W, 8.5 + 14.5 + 3.0)
    return board.fuse(pins)


def export(shape, name, stl=True):
    shape.exportStep(os.path.join(OUT, name + ".step"))
    if stl:
        Mesh.Mesh(shape.tessellate(0.02)).write(os.path.join(OUT, name + ".stl"))
    bb = shape.BoundBox
    print(f"{name}: {bb.XLength:.1f} x {bb.YLength:.1f} x {bb.ZLength:.1f} mm, "
          f"volume {shape.Volume / 1000:.1f} cm3, valid={shape.isValid()}")


def check(shell, bottom, name, dummy):
    hit = max(shell.common(dummy).Volume, bottom.common(dummy).Volume)
    print(f"  {name}: {'clear' if hit < 0.01 else f'HITS ({hit:.1f} mm3)'}")


shell = build_shell()
bottom = build_bottom()
ring = build_ring()
export(shell, "shell")
export(bottom, "bottom")
export(ring, "ring")
export(build_screen_frame(), "screen-frame")
print(f"outside: {W:.0f} wide, {DEPTH:.1f} deep, {HEIGHT:.1f} high")

print("clearance:")
hit = shell.common(bottom).Volume
print(f"  bottom part: {'clear' if hit < 0.01 else f'HITS ({hit:.1f} mm3)'}")
check(shell, bottom, "screen", dummy_screen())
check(shell, bottom, "USB-C plug", dummy_usb_plug())
for i, d in enumerate(dummy_encoders()):
    check(shell, bottom, f"encoder {i + 1}" if i < 4 else "button bar", d)
check(shell, bottom, "Pico with dupont plugs", dummy_pico())
screen = dummy_screen()
for i, d in enumerate(dummy_encoders()):
    if screen.common(d).Volume > 0.01:
        print(f"  screen hits encoder/bar {i + 1}")
if screen.common(dummy_pico()).Volume > 0.01:
    print("  screen hits the Pico")

rings = [placed(placed(build_ring(), App.Placement(V(x, ROW_S, -RING_T), App.Rotation())), deck_frame()) for x in KNOB_X]
assembly = Part.makeCompound([shell, bottom, dummy_screen(), dummy_pico()] + rings + dummy_knobs() + dummy_encoders())
export(assembly, "assembly", stl=False)
