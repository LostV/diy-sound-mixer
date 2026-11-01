"""Design concept for the finished mixer: the shell on a desk, with the knobs
turning, the screen following and the mic button lighting up.

This is a look-and-feel render, not a print model. Sizes follow the real
parts (5" CrowPanel, the black 30 x 17 mm knobs, 40 mm knob spacing, the red LED
button), so the proportions are right.

Run with Blender 5.x from the repository root:
    blender -b -P enclosure/concept/concept.py -- --variant white --mode still
Modes: still (one frame), anim (all frames as PNG), blend (save the scene).
Options: --variant white|black, --frame N, --res WxH, --samples N, --out DIR
"""

import argparse
import math
import os
import sys

import bmesh
import bpy
from mathutils import Vector

ROOT = os.getcwd()
FONTS = "C:/Windows/Fonts/"
FPS, FRAMES = 30, 300

# Shell, in metres. Profile in the YZ plane, front edge towards -Y.
SHELL_W = 0.200
PROFILE = [(-0.0625, 0.0), (-0.0625, 0.018), (-0.015, 0.024), (0.055, 0.075), (0.0625, 0.075), (0.0625, 0.0)]
DECK_ANGLE = math.atan2(0.024 - 0.018, -0.015 + 0.0625)
FACE_ANGLE = math.atan2(0.075 - 0.024, 0.055 + 0.015)
KNOB_Y = -0.040
KNOB_X = (-0.072, -0.032, 0.008, 0.048)
MIC_X = 0.082

# Screen: CrowPanel 5", 800 x 480 px on a 108 x 65 mm active area.
PX = 0.108 / 800
CHANNELS = [
    ("Spill", "Steam", (0.18, 0.62, 0.38)),
    ("Media", "Google Chrome", (0.98, 0.40, 0.08)),
    ("Chat", "Discord", (0.36, 0.40, 0.95)),
    ("Mic", "Headset Microphone", (0.90, 0.26, 0.26)),
]

# Knob values over time: (frame, value). Eased between keys.
VALUES = [
    [(0, 80), (100, 80), (150, 45), (265, 45), (295, 70)],
    [(0, 65), (150, 65), (195, 90), (300, 90)],
    [(0, 90), (195, 90), (230, 60), (300, 60)],
    [(0, 100), (300, 100)],
]
MIC_PRESS = (240, 248)  # cap goes down and up; the mic mutes at the bottom
MIC_MUTED_AT = 244

SHELLS = {
    "white": {"color": (0.80, 0.80, 0.78), "rough": 0.62, "ring": 0.85},
    "black": {"color": (0.022, 0.022, 0.024), "rough": 0.55, "ring": 0.07},
}


def ease(t):
    return t * t * (3 - 2 * t)


def value_at(keys, frame):
    if frame <= keys[0][0]:
        return keys[0][1]
    for (f0, v0), (f1, v1) in zip(keys, keys[1:]):
        if f0 <= frame <= f1:
            return v0 + (v1 - v0) * ease((frame - f0) / max(1, f1 - f0))
    return keys[-1][1]


def muted_at(frame):
    return frame >= MIC_MUTED_AT


# ---------------------------------------------------------------- helpers

def link(obj, parent=None):
    bpy.context.scene.collection.objects.link(obj)
    if parent:
        obj.parent = parent
    return obj


def mesh_obj(name, bm, parent=None, mat=None):
    me = bpy.data.meshes.new(name)
    bm.to_mesh(me)
    bm.free()
    obj = link(bpy.data.objects.new(name, me), parent)
    if mat:
        obj.data.materials.append(mat)
    return obj


def box(name, size, loc=(0, 0, 0), parent=None, mat=None, bevel=0.0):
    bm = bmesh.new()
    bmesh.ops.create_cube(bm, size=1.0)
    bmesh.ops.scale(bm, vec=size, verts=bm.verts)
    obj = mesh_obj(name, bm, parent, mat)
    obj.location = loc
    if bevel:
        m = obj.modifiers.new("bevel", "BEVEL")
        m.width, m.segments = bevel, 3
    return obj


def cylinder(name, r, h, loc=(0, 0, 0), parent=None, mat=None, segs=64, bevel=0.0):
    bm = bmesh.new()
    bmesh.ops.create_cone(bm, cap_ends=True, segments=segs, radius1=r, radius2=r, depth=h)
    bmesh.ops.translate(bm, vec=(0, 0, h / 2), verts=bm.verts)
    obj = mesh_obj(name, bm, parent, mat)
    obj.location = loc
    if bevel:
        m = obj.modifiers.new("bevel", "BEVEL")
        m.width, m.segments, m.limit_method = bevel, 3, "ANGLE"
    for p in obj.data.polygons:
        p.use_smooth = True
    return obj


def rounded_rect(name, w, h, r, loc, parent, mat):
    bm = bmesh.new()
    v = [bm.verts.new((x * w / 2, y * h / 2, 0)) for x, y in ((-1, -1), (1, -1), (1, 1), (-1, 1))]
    bm.faces.new(v)
    obj = mesh_obj(name, bm, parent, mat)
    obj.location = loc
    m = obj.modifiers.new("round", "BEVEL")
    m.affect, m.width, m.segments = "VERTICES", min(r, w / 2 - 1e-5, h / 2 - 1e-5), 8
    return obj


def mat_principled(name, color, rough=0.5, metal=0.0, emission=None, strength=0.0, alpha=1.0, aniso=0.0):
    m = bpy.data.materials.new(name)
    m.use_nodes = True
    b = m.node_tree.nodes["Principled BSDF"]
    b.inputs["Base Color"].default_value = (*color, 1)
    b.inputs["Roughness"].default_value = rough
    b.inputs["Metallic"].default_value = metal
    if aniso:
        b.inputs["Anisotropic"].default_value = aniso
    if emission:
        b.inputs["Emission Color"].default_value = (*emission, 1)
        b.inputs["Emission Strength"].default_value = strength
    if alpha < 1:
        b.inputs["Alpha"].default_value = alpha
        m.surface_render_method = "BLENDED"
    return m


def mat_emission(name, color, strength=1.0, alpha=1.0):
    m = bpy.data.materials.new(name)
    m.use_nodes = True
    nt = m.node_tree
    nt.nodes.clear()
    out = nt.nodes.new("ShaderNodeOutputMaterial")
    em = nt.nodes.new("ShaderNodeEmission")
    em.inputs["Color"].default_value = (*color, 1)
    em.inputs["Strength"].default_value = strength
    if alpha < 1:
        tr = nt.nodes.new("ShaderNodeBsdfTransparent")
        mix = nt.nodes.new("ShaderNodeMixShader")
        mix.inputs["Fac"].default_value = alpha
        nt.links.new(tr.outputs[0], mix.inputs[1])
        nt.links.new(em.outputs[0], mix.inputs[2])
        nt.links.new(mix.outputs[0], out.inputs["Surface"])
        m.surface_render_method = "BLENDED"
    else:
        nt.links.new(em.outputs[0], out.inputs["Surface"])
    return m


def text(name, body, size, loc, parent, mat, font, align="CENTER"):
    cu = bpy.data.curves.new(name, "FONT")
    cu.body, cu.size, cu.font = body, size, font
    cu.align_x, cu.align_y = align, "CENTER"
    obj = link(bpy.data.objects.new(name, cu), parent)
    obj.location = loc
    obj.data.materials.append(mat)
    return obj


def arc(name, cx, cy, radius, width, parent, mat, z):
    cu = bpy.data.curves.new(name, "CURVE")
    cu.dimensions = "3D"
    cu.bevel_depth = width / 2
    cu.bevel_resolution = 2
    cu.bevel_factor_mapping_end = "SPLINE"
    sp = cu.splines.new("POLY")
    n = 72
    sp.points.add(n - 1)
    for i in range(n):
        a = math.radians(225 - 270 * i / (n - 1))
        sp.points[i].co = (cx + radius * math.cos(a), cy + radius * math.sin(a), 0, 1)
    obj = link(bpy.data.objects.new(name, cu), parent)
    obj.location.z = z
    obj.scale.z = 0.02
    obj.data.materials.append(mat)
    return obj


def socket(sockets, identifier):
    return next(s for s in sockets if s.identifier == identifier)


def ui(px, py):
    """Screen pixel to screen-local metres (origin at the centre)."""
    return ((px - 400) * PX, (240 - py) * PX)


# ---------------------------------------------------------------- scene

def build_world():
    sc = bpy.context.scene
    for o in list(bpy.data.objects):
        bpy.data.objects.remove(o)
    w = bpy.data.worlds.new("room") if not sc.world else sc.world
    sc.world = w
    w.use_nodes = True
    bg = w.node_tree.nodes["Background"]
    bg.inputs["Color"].default_value = (0.012, 0.014, 0.022, 1)
    bg.inputs["Strength"].default_value = 1.0


def build_desk():
    wood = bpy.data.materials.new("walnut")
    wood.use_nodes = True
    nt = wood.node_tree
    b = nt.nodes["Principled BSDF"]
    b.inputs["Roughness"].default_value = 0.45
    wave = nt.nodes.new("ShaderNodeTexWave")
    wave.bands_direction = "X"
    wave.inputs["Scale"].default_value = 5.0
    wave.inputs["Distortion"].default_value = 2.5
    wave.inputs["Detail"].default_value = 8.0
    wave.inputs["Detail Scale"].default_value = 2.0
    ramp = nt.nodes.new("ShaderNodeValToRGB")
    ramp.color_ramp.elements[0].color = (0.11, 0.06, 0.035, 1)
    ramp.color_ramp.elements[1].color = (0.16, 0.09, 0.05, 1)
    nt.links.new(wave.outputs["Fac"], ramp.inputs["Fac"])
    nt.links.new(ramp.outputs["Color"], b.inputs["Base Color"])
    box("desk", (1.8, 0.85, 0.03), (0, 0.12, -0.015), mat=wood, bevel=0.003)

    wall = mat_principled("wall", (0.05, 0.05, 0.06), 0.9)
    box("wall", (4, 0.02, 2), (0, 0.62, 0.9), mat=wall)
    strip = mat_emission("led strip", (0.45, 0.18, 0.95), 25)
    box("led strip", (1.2, 0.01, 0.006), (0, 0.605, 0.22), mat=strip)

    mat_felt = mat_principled("desk mat", (0.03, 0.03, 0.035), 0.95)
    box("desk mat", (0.9, 0.36, 0.003), (0.08, -0.01, 0.0015), mat=mat_felt, bevel=0.0012)


def build_monitor():
    dark = mat_principled("monitor body", (0.02, 0.02, 0.022), 0.4)
    box("monitor stand base", (0.26, 0.2, 0.012), (0, 0.38, 0.006), mat=dark, bevel=0.004)
    box("monitor stand neck", (0.06, 0.03, 0.30), (0, 0.44, 0.16), mat=dark, bevel=0.006)
    box("monitor", (0.62, 0.035, 0.37), (0, 0.42, 0.40), mat=dark, bevel=0.006)

    # Wallpaper with the PC app snapped to the bottom left, as it is used.
    m = bpy.data.materials.new("monitor screen")
    m.use_nodes = True
    nt = m.node_tree
    nt.nodes.clear()
    out = nt.nodes.new("ShaderNodeOutputMaterial")
    em = nt.nodes.new("ShaderNodeEmission")
    em.inputs["Strength"].default_value = 2.2
    tc = nt.nodes.new("ShaderNodeTexCoord")
    grad = nt.nodes.new("ShaderNodeTexGradient")
    ramp = nt.nodes.new("ShaderNodeValToRGB")
    ramp.color_ramp.elements[0].color = (0.10, 0.04, 0.22, 1)
    ramp.color_ramp.elements[1].color = (0.02, 0.16, 0.22, 1)
    img = nt.nodes.new("ShaderNodeTexImage")
    img.image = bpy.data.images.load(os.path.join(ROOT, "docs", "screenshots", "mixer-window.png"))
    img.extension = "CLIP"
    mapping = nt.nodes.new("ShaderNodeMapping")
    # The window covers about 905 x 690 of a 2560 x 1440 screen, bottom left.
    mapping.inputs["Scale"].default_value = (2560 / 905, 1440 / 690, 1)
    mix = nt.nodes.new("ShaderNodeMix")
    mix.data_type = "RGBA"
    nt.links.new(tc.outputs["UV"], grad.inputs["Vector"])
    nt.links.new(grad.outputs["Fac"], ramp.inputs["Fac"])
    nt.links.new(tc.outputs["UV"], mapping.inputs["Vector"])
    nt.links.new(mapping.outputs["Vector"], img.inputs["Vector"])
    nt.links.new(img.outputs["Alpha"], socket(mix.inputs, "Factor_Float"))
    nt.links.new(ramp.outputs["Color"], socket(mix.inputs, "A_Color"))
    nt.links.new(img.outputs["Color"], socket(mix.inputs, "B_Color"))
    nt.links.new(socket(mix.outputs, "Result_Color"), em.inputs["Color"])
    nt.links.new(em.outputs[0], out.inputs["Surface"])

    bpy.ops.mesh.primitive_plane_add(size=1)
    scr = bpy.context.active_object
    scr.name = "monitor screen"
    scr.scale = (0.60, 0.34, 1)
    scr.rotation_euler = (math.radians(90), 0, 0)
    scr.location = (0, 0.4020, 0.405)
    scr.data.materials.append(m)


def build_keyboard_mouse():
    case = mat_principled("keyboard case", (0.03, 0.03, 0.033), 0.5)
    keys = mat_principled("keycaps", (0.06, 0.06, 0.065), 0.65)
    kb = box("keyboard", (0.36, 0.125, 0.018), (0.02, -0.06, 0.012), mat=case, bevel=0.004)
    kb.rotation_euler.x = math.radians(3)
    key = box("key", (0.0165, 0.0165, 0.008), (-0.1575, -0.045, 0.011), parent=kb, mat=keys, bevel=0.0018)
    ax = key.modifiers.new("cols", "ARRAY")
    ax.count, ax.relative_offset_displace = 18, (1.15, 0, 0)
    ay = key.modifiers.new("rows", "ARRAY")
    ay.count, ay.relative_offset_displace = 5, (0, 1.15, 0)

    mouse_mat = mat_principled("mouse", (0.03, 0.03, 0.033), 0.35)
    bpy.ops.mesh.primitive_uv_sphere_add(radius=0.03, location=(0.32, -0.05, 0.012))
    mouse = bpy.context.active_object
    mouse.name = "mouse"
    mouse.scale = (0.68, 1.0, 0.42)
    bpy.ops.object.shade_smooth()
    mouse.data.materials.append(mouse_mat)

    mug_mat = mat_principled("mug", (0.75, 0.73, 0.70), 0.3)
    cylinder("mug", 0.042, 0.10, (0.62, 0.12, 0.0), mat=mug_mat, bevel=0.003)
    coffee = mat_principled("coffee", (0.03, 0.015, 0.01), 0.1)
    cylinder("coffee", 0.038, 0.002, (0.62, 0.12, 0.09), mat=coffee)


def build_shell(variant):
    bm = bmesh.new()
    left = [bm.verts.new((-SHELL_W / 2, y, z)) for y, z in PROFILE]
    face = bm.faces.new(left)
    ext = bmesh.ops.extrude_face_region(bm, geom=[face])
    bmesh.ops.translate(bm, vec=(SHELL_W, 0, 0), verts=[e for e in ext["geom"] if isinstance(e, bmesh.types.BMVert)])
    bmesh.ops.recalc_face_normals(bm, faces=bm.faces)
    shell = mesh_obj("shell", bm)
    m = shell.modifiers.new("bevel", "BEVEL")
    m.width, m.segments, m.limit_method = 0.005, 6, "ANGLE"
    m.harden_normals = True
    for p in shell.data.polygons:
        p.use_smooth = True

    s = SHELLS[variant]
    mat = mat_principled("shell", s["color"], s["rough"])
    shell.data.materials.append(mat)

    feet = mat_principled("rubber feet", (0.01, 0.01, 0.01), 0.9)
    for x in (-0.08, 0.08):
        for y in (-0.05, 0.05):
            cylinder("foot", 0.006, 0.002, (x, y, -0.002), shell, feet)
    return shell


def build_deck(shell, variant):
    z = PROFILE[1][1] + (KNOB_Y - PROFILE[1][0]) * math.tan(DECK_ANGLE)
    deck = link(bpy.data.objects.new("deck frame", None), shell)
    deck.location = (0, KNOB_Y, z)
    deck.rotation_euler.x = DECK_ANGLE

    knob_mats = knob_materials()
    knobs = []
    for i, x in enumerate(KNOB_X):
        color = CHANNELS[i][2]
        ring = mat_principled(f"ring {i}", color, 0.55)
        # Colour ring around the knob, printed in the channel colour (AMS).
        bm = bmesh.new()
        bmesh.ops.create_circle(bm, cap_ends=False, segments=96, radius=0.0185)
        inner = bmesh.ops.create_circle(bm, cap_ends=False, segments=96, radius=0.0162)
        bm.verts.ensure_lookup_table()
        bmesh.ops.bridge_loops(bm, edges=bm.edges[:])
        r = mesh_obj(f"ring {i}", bm, deck, ring)
        r.location = (x, 0, 0.0003)
        sol = r.modifiers.new("thick", "SOLIDIFY")
        sol.thickness = 0.0004

        pivot = link(bpy.data.objects.new(f"knob {i}", None), deck)
        pivot.location = (x, 0, 0.0004)
        black_knob(f"knob body {i}", pivot, knob_mats)
        knobs.append(pivot)

    # Mic button: red cap in a light collar, sunk in a small square bezel.
    bezel_col = (0.65, 0.65, 0.63) if variant == "white" else (0.05, 0.05, 0.055)
    bezel = mat_principled("button bezel", bezel_col, 0.5)
    rounded = box("mic bezel", (0.016, 0.016, 0.0008), (MIC_X, 0, 0.0004), deck, bezel, bevel=0.0004)
    collar = mat_principled("collar", (0.85, 0.83, 0.80), 0.35)
    cylinder("mic collar", 0.00413, 0.00115, (MIC_X, 0, 0.0005), deck, collar, bevel=0.0002)
    led = bpy.data.materials.new("mic led")
    led.use_nodes = True
    b = led.node_tree.nodes["Principled BSDF"]
    b.inputs["Base Color"].default_value = (0.55, 0.02, 0.02, 1)
    b.inputs["Roughness"].default_value = 0.15
    b.inputs["Emission Color"].default_value = (1.0, 0.05, 0.03, 1)
    b.inputs["Coat Weight"].default_value = 1.0
    cap = cylinder("mic cap", 0.00345, 0.00115, (MIC_X, 0, 0.00165), deck, led, bevel=0.0003)
    glow = bpy.data.lights.new("mic glow", "POINT")
    glow.color, glow.energy, glow.shadow_soft_size = (1, 0.08, 0.04), 0.0, 0.002
    gl = link(bpy.data.objects.new("mic glow", glow), deck)
    gl.location = (MIC_X, 0, 0.006)
    return knobs, cap, b, glow


def black_knob(name, parent, mats):
    """30 x 17 mm black anodised knob, as ordered: smooth glossy side, a spun
    (radially brushed) top, a bright diamond-cut chamfer round the top edge and
    a white pointer line."""
    side, top, bright = mats
    r, h, ch = 0.0150, 0.0170, 0.0010
    cylinder(name, r, h - ch, (0, 0, 0), parent, side, segs=128)
    bm = bmesh.new()
    bmesh.ops.create_cone(bm, cap_ends=True, segments=128, radius1=r, radius2=r - ch, depth=ch)
    bmesh.ops.translate(bm, vec=(0, 0, h - ch / 2), verts=bm.verts)
    rim = mesh_obj(name + " chamfer", bm, parent, bright)
    for p in rim.data.polygons:
        p.use_smooth = True
    cylinder(name + " top", r - ch - 0.0001, 0.00005, (0, 0, h - 0.00004), parent, top, segs=128)
    box(name + " pointer", (0.0007, 0.0040, 0.0002), (0, r - ch - 0.0030, h), parent, bright)


def knob_materials():
    side = mat_principled("knob side", (0.015, 0.015, 0.017), 0.18, metal=1.0)
    bright = mat_principled("knob bright", (0.92, 0.92, 0.94), 0.12, metal=1.0)
    # Spun top: fine radial streaks in the roughness.
    top = bpy.data.materials.new("knob top")
    top.use_nodes = True
    nt = top.node_tree
    b = nt.nodes["Principled BSDF"]
    b.inputs["Base Color"].default_value = (0.02, 0.02, 0.022, 1)
    b.inputs["Metallic"].default_value = 1.0
    tc = nt.nodes.new("ShaderNodeTexCoord")
    sep = nt.nodes.new("ShaderNodeSeparateXYZ")
    ang = nt.nodes.new("ShaderNodeMath")
    ang.operation = "ARCTAN2"
    noise = nt.nodes.new("ShaderNodeTexNoise")
    noise.inputs["Scale"].default_value = 40.0
    comb = nt.nodes.new("ShaderNodeCombineXYZ")
    rng = nt.nodes.new("ShaderNodeMapRange")
    rng.inputs["To Min"].default_value = 0.12
    rng.inputs["To Max"].default_value = 0.40
    nt.links.new(tc.outputs["Object"], sep.inputs[0])
    nt.links.new(sep.outputs["Y"], ang.inputs[0])
    nt.links.new(sep.outputs["X"], ang.inputs[1])
    nt.links.new(ang.outputs[0], comb.inputs["X"])
    nt.links.new(comb.outputs[0], noise.inputs["Vector"])
    nt.links.new(noise.outputs["Fac"], rng.inputs["Value"])
    nt.links.new(rng.outputs["Result"], b.inputs["Roughness"])
    return side, top, bright


def build_screen(shell):
    p0, p1 = PROFILE[2], PROFILE[3]
    mid = ((p0[0] + p1[0]) / 2, (p0[1] + p1[1]) / 2)
    frame = link(bpy.data.objects.new("screen frame", None), shell)
    frame.location = (0, mid[0], mid[1])
    frame.rotation_euler.x = FACE_ANGLE

    glass_black = mat_principled("panel glass", (0.005, 0.005, 0.006), 0.08)
    box("screen glass", (0.124, 0.078, 0.0012), (0, 0.0015, 0.0003), frame, glass_black, bevel=0.0006)

    ui_root = link(bpy.data.objects.new("ui", None), frame)
    ui_root.location = (0, 0.0015, 0.0010)

    # Background gradient, like the panel firmware.
    m = bpy.data.materials.new("ui background")
    m.use_nodes = True
    nt = m.node_tree
    nt.nodes.clear()
    out = nt.nodes.new("ShaderNodeOutputMaterial")
    em = nt.nodes.new("ShaderNodeEmission")
    em.inputs["Strength"].default_value = 1.0
    tc = nt.nodes.new("ShaderNodeTexCoord")
    mapping = nt.nodes.new("ShaderNodeMapping")
    mapping.inputs["Rotation"].default_value = (0, 0, math.radians(-25))
    grad = nt.nodes.new("ShaderNodeTexGradient")
    ramp = nt.nodes.new("ShaderNodeValToRGB")
    ramp.color_ramp.elements[0].color = (0.13, 0.05, 0.26, 1)
    ramp.color_ramp.elements.new(0.5).color = (0.08, 0.03, 0.10, 1)
    ramp.color_ramp.elements[2].color = (0.02, 0.17, 0.20, 1)
    nt.links.new(tc.outputs["UV"], mapping.inputs["Vector"])
    nt.links.new(mapping.outputs["Vector"], grad.inputs["Vector"])
    nt.links.new(grad.outputs["Fac"], ramp.inputs["Fac"])
    nt.links.new(ramp.outputs["Color"], em.inputs["Color"])
    nt.links.new(em.outputs[0], out.inputs["Surface"])
    bpy.ops.mesh.primitive_plane_add(size=1)
    bgp = bpy.context.active_object
    bgp.name = "ui background"
    bgp.parent = ui_root
    bgp.scale = (0.108, 0.065, 1)
    bgp.data.materials.append(m)

    light = load_font("segoeuil.ttf")
    regular = load_font("segoeui.ttf")
    semi = load_font("seguisb.ttf")
    white = mat_emission("ui white", (1, 1, 1), 1.6)
    dim = mat_emission("ui dim", (0.62, 0.62, 0.68), 1.2)
    card_mat = mat_emission("ui card", (0.75, 0.75, 0.95), 0.25, alpha=0.10)
    edge_mat = mat_emission("ui card edge", (0.8, 0.8, 1.0), 0.5, alpha=0.16)
    pill_mat = mat_emission("ui pill", (0.8, 0.8, 1.0), 0.4, alpha=0.14)
    track_mat = mat_emission("ui track", (0.5, 0.5, 0.6), 0.6, alpha=0.25)

    text("ui title", "DIY Sound Mixer", 16 * PX * 1.3, (*ui(28, 32), 0.0002), ui_root, dim, regular, "LEFT")
    text("ui status", "Tilkoblet", 14 * PX * 1.3, (*ui(786, 32), 0.0002), ui_root, dim, regular, "RIGHT")
    cylinder("ui status dot", 4 * PX, 0.00005, (*ui(702, 32), 0.0002), ui_root, mat_emission("ui green", (0.2, 0.85, 0.6), 2))

    cards = []
    for i, (name, app, color) in enumerate(CHANNELS):
        left = 18 + i * 194
        cx = left + 90
        z = 0.00004
        rounded_rect(f"card edge {i}", 182 * PX, 386 * PX, 16 * PX, (*ui(cx, 254), z), ui_root, edge_mat)
        rounded_rect(f"card {i}", 179 * PX, 383 * PX, 15 * PX, (*ui(cx, 254), z * 2), ui_root, card_mat)
        cmat = mat_emission(f"ui {name}", color, 2.2)
        cylinder(f"dot {i}", 5 * PX, 0.00005, (*ui(left + 22, 92), z * 3), ui_root, cmat)
        text(f"name {i}", name, 20 * PX * 1.35, (*ui(left + 36, 92), z * 3), ui_root, white, semi, "LEFT")
        acx, acy = ui(cx, 206)
        arc(f"track {i}", acx, acy, 68 * PX, 6 * PX, ui_root, track_mat, z * 3)
        bar = arc(f"arc {i}", acx, acy, 78 * PX, 14 * PX, ui_root, cmat, z * 4)
        num = text(f"value {i}", "100", 46 * PX * 1.35, (acx, acy, z * 4), ui_root, white, light)
        pill = rounded_rect(f"pill {i}", 120 * PX, 42 * PX, 21 * PX, (*ui(cx, 331), z * 3), ui_root, pill_mat)
        label = text(f"mute {i}", "Mute", 16 * PX * 1.35, (*ui(cx, 331), z * 4), ui_root, white, regular)
        text(f"app {i}", app, 14 * PX * 1.3, (*ui(cx, 392), z * 3), ui_root, dim, regular)
        cards.append({"arc": bar, "value": num, "pill": pill, "label": label, "color": cmat})

    cover = mat_principled("screen cover", (0, 0, 0), 0.04, alpha=0.06)
    box("screen cover", (0.122, 0.076, 0.0002), (0, 0.0015, 0.0013), frame, cover)
    return cards


def load_font(file):
    path = os.path.join(FONTS, file)
    return bpy.data.fonts.load(path, check_existing=True) if os.path.exists(path) else None


# ---------------------------------------------------------------- animation

def animate(knobs, cards, cap, led_bsdf, glow):
    sc = bpy.context.scene
    sc.render.fps = FPS
    sc.frame_start, sc.frame_end = 1, FRAMES
    muted_pill = mat_emission("ui pill muted", (1.0, 0.2, 0.2), 1.6, alpha=0.55)
    cards[3]["pill"].data = cards[3]["pill"].data.copy()  # its own slot to swap
    for f in range(0, FRAMES + 1):
        for i, keys in enumerate(VALUES):
            v = value_at(keys, f)
            knobs[i].rotation_euler.z = math.radians(135 - 270 * v / 100)
            knobs[i].keyframe_insert("rotation_euler", index=2, frame=f)
            cards[i]["arc"].data.bevel_factor_end = max(0.001, v / 100)
            cards[i]["arc"].data.keyframe_insert("bevel_factor_end", frame=f)
    # Mic press: cap travels 0.5 mm.
    for f, dz in ((MIC_PRESS[0], 0), (MIC_PRESS[0] + 4, -0.0005), (MIC_PRESS[1], 0)):
        cap.location.z = 0.00165 + dz
        cap.keyframe_insert("location", index=2, frame=f)
    for f, on in ((MIC_MUTED_AT - 1, 0), (MIC_MUTED_AT + 2, 1)):
        led_bsdf.inputs["Emission Strength"].default_value = 1.0 * on
        led_bsdf.inputs["Emission Strength"].keyframe_insert("default_value", frame=f)
        glow.energy = 0.25 * on
        glow.keyframe_insert("energy", frame=f)
    # Mic card: the arc dims and the pill turns red when muted.
    mic_arc = cards[3]["color"].node_tree.nodes["Emission"].inputs["Strength"]
    for f, s in ((MIC_MUTED_AT - 1, 2.2), (MIC_MUTED_AT + 2, 0.25)):
        mic_arc.default_value = s
        mic_arc.keyframe_insert("default_value", frame=f)

    def on_frame(scene, depsgraph=None):
        f = scene.frame_current
        for i, keys in enumerate(VALUES):
            cards[i]["value"].data.body = str(round(value_at(keys, f)))
        m = muted_at(f)
        cards[3]["label"].data.body = "Muted" if m else "Mute"
        cards[3]["pill"].material_slots[0].material = muted_pill if m else bpy.data.materials["ui pill"]

    bpy.app.handlers.frame_change_pre.clear()
    bpy.app.handlers.frame_change_pre.append(on_frame)
    on_frame(sc)


def build_camera_lights(shell):
    sc = bpy.context.scene
    target = link(bpy.data.objects.new("camera target", None))
    cam_data = bpy.data.cameras.new("camera")
    cam_data.lens = 50
    cam_data.dof.use_dof = True
    # Keep the knobs sharp: focus on the knob row, not on where the camera aims.
    focus = link(bpy.data.objects.new("focus", None), shell)
    focus.location = (0, KNOB_Y, 0.03)
    cam_data.dof.focus_object = focus
    cam_data.dof.aperture_fstop = 4.0
    cam = link(bpy.data.objects.new("camera", cam_data))
    sc.camera = cam
    track = cam.constraints.new("TRACK_TO")
    track.target = target

    # Wide desk shot, push in on the knobs, then a low hero angle.
    shots = [
        (1, (-0.10, -1.50, 0.56), (-0.12, 0.12, 0.27)),
        (80, (-0.58, -0.45, 0.20), (-0.40, 0.00, 0.04)),
        (150, (-0.55, -0.30, 0.14), (-0.44, -0.03, 0.03)),
        (215, (-0.36, -0.30, 0.12), (-0.39, -0.03, 0.03)),
        (250, (-0.24, -0.26, 0.10), (-0.32, -0.02, 0.028)),
        (300, (-0.66, -0.36, 0.16), (-0.39, 0.00, 0.035)),
    ]
    for f, loc, tgt in shots:
        cam.location, target.location = loc, tgt
        cam.keyframe_insert("location", frame=f)
        target.keyframe_insert("location", frame=f)

    key = bpy.data.lights.new("key", "AREA")
    key.energy, key.size, key.color = 60, 0.6, (1.0, 0.86, 0.72)
    k = link(bpy.data.objects.new("key", key))
    k.location = (-0.9, -0.5, 1.0)
    k.rotation_euler = (math.radians(50), 0, math.radians(-55))
    rim = bpy.data.lights.new("rim", "AREA")
    rim.energy, rim.size, rim.color = 25, 0.4, (0.55, 0.35, 1.0)
    r = link(bpy.data.objects.new("rim", rim))
    r.location = (-0.2, 0.5, 0.5)
    r.rotation_euler = (math.radians(-120), 0, 0)
    fill = bpy.data.lights.new("fill", "AREA")
    fill.energy, fill.size, fill.color = 8, 1.0, (0.7, 0.8, 1.0)
    fl = link(bpy.data.objects.new("fill", fill))
    fl.location = (0.6, -0.8, 0.6)
    fl.rotation_euler = (math.radians(55), 0, math.radians(35))


def place_mixer(shell):
    shell.location = (-0.40, 0.0, 0.003)
    shell.rotation_euler.z = math.radians(12)


def render_setup(res, samples):
    sc = bpy.context.scene
    sc.render.engine = "BLENDER_EEVEE"
    sc.eevee.taa_render_samples = samples
    try:
        sc.eevee.use_raytracing = True
    except AttributeError:
        pass
    sc.render.resolution_x, sc.render.resolution_y = res
    sc.render.resolution_percentage = 100
    sc.view_settings.view_transform = "AgX"
    sc.view_settings.look = "AgX - Medium High Contrast"
    sc.render.image_settings.file_format = "PNG"


def main():
    argv = sys.argv[sys.argv.index("--") + 1:] if "--" in sys.argv else []
    ap = argparse.ArgumentParser()
    ap.add_argument("--variant", default="white", choices=SHELLS)
    ap.add_argument("--mode", default="still", choices=("still", "anim", "blend"))
    ap.add_argument("--frame", type=int, default=270)
    ap.add_argument("--res", default="1920x1080")
    ap.add_argument("--samples", type=int, default=64)
    ap.add_argument("--out", default=os.path.join(ROOT, "enclosure", "concept", "renders"))
    a = ap.parse_args(argv)
    os.makedirs(a.out, exist_ok=True)

    build_world()
    build_desk()
    build_monitor()
    build_keyboard_mouse()
    shell = build_shell(a.variant)
    knobs, cap, led, glow = build_deck(shell, a.variant)
    cards = build_screen(shell)
    place_mixer(shell)
    build_camera_lights(shell)
    animate(knobs, cards, cap, led, glow)
    render_setup(tuple(int(x) for x in a.res.split("x")), a.samples)

    sc = bpy.context.scene
    if a.mode == "blend":
        bpy.ops.wm.save_as_mainfile(filepath=os.path.join(a.out, f"concept-{a.variant}.blend"))
    elif a.mode == "still":
        sc.frame_set(a.frame)
        sc.render.filepath = os.path.join(a.out, f"{a.variant}-f{a.frame}.png")
        bpy.ops.render.render(write_still=True)
    else:
        sc.render.filepath = os.path.join(a.out, a.variant, "frame_")
        bpy.ops.render.render(animation=True)


main()
