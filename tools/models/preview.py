"""Превью мешей в Blender (Cycles, CPU). Координаты на входе — как в игре (Y вверх, лицо в -Z)."""

import math
import os

import bpy
import numpy as np
from mathutils import Vector


def to_blender(v):
    v = np.asarray(v)
    return np.stack([v[:, 0], -v[:, 2], v[:, 1]], axis=1)


def reset():
    bpy.ops.wm.read_factory_settings(use_empty=True)
    sc = bpy.context.scene
    sc.render.engine = "CYCLES"
    sc.cycles.device = "CPU"
    sc.cycles.samples = int(os.environ.get("SAMPLES", "24"))
    sc.cycles.use_denoising = True
    sc.view_settings.view_transform = "AgX"
    w = bpy.data.worlds.new("w")
    w.use_nodes = True
    w.node_tree.nodes["Background"].inputs[0].default_value = (0.42, 0.45, 0.5, 1)
    w.node_tree.nodes["Background"].inputs[1].default_value = 0.5
    sc.world = w


def material(name, color=(0.8, 0.8, 0.8), image=None, rough=0.6, metal=0.0, normal=None, mr=None, vcol=False):
    m = bpy.data.materials.new(name)
    m.use_nodes = True
    nt = m.node_tree
    bsdf = nt.nodes["Principled BSDF"]
    bsdf.inputs["Roughness"].default_value = rough
    bsdf.inputs["Metallic"].default_value = metal
    bsdf.inputs["Base Color"].default_value = (*color, 1)
    if image:
        t = nt.nodes.new("ShaderNodeTexImage")
        t.image = bpy.data.images.load(image, check_existing=True)
        if vcol:
            mix = nt.nodes.new("ShaderNodeMix")
            mix.data_type = "RGBA"
            mix.blend_type = "MULTIPLY"
            mix.inputs[0].default_value = 1.0
            a = nt.nodes.new("ShaderNodeVertexColor")
            nt.links.new(t.outputs[0], mix.inputs[6])
            nt.links.new(a.outputs[0], mix.inputs[7])
            nt.links.new(mix.outputs[2], bsdf.inputs["Base Color"])
        else:
            nt.links.new(t.outputs[0], bsdf.inputs["Base Color"])
    if mr:
        t = nt.nodes.new("ShaderNodeTexImage")
        t.image = bpy.data.images.load(mr, check_existing=True)
        t.image.colorspace_settings.name = "Non-Color"
        sep = nt.nodes.new("ShaderNodeSeparateColor")
        nt.links.new(t.outputs[0], sep.inputs[0])
        nt.links.new(sep.outputs[1], bsdf.inputs["Roughness"])
        nt.links.new(sep.outputs[2], bsdf.inputs["Metallic"])
    if normal:
        t = nt.nodes.new("ShaderNodeTexImage")
        t.image = bpy.data.images.load(normal, check_existing=True)
        t.image.colorspace_settings.name = "Non-Color"
        nm = nt.nodes.new("ShaderNodeNormalMap")
        nt.links.new(t.outputs[0], nm.inputs[1])
        nt.links.new(nm.outputs[0], bsdf.inputs["Normal"])
    return m


def add_mesh(name, v, faces, uv=None, mat=None, smooth=True, colors=None):
    me = bpy.data.meshes.new(name)
    me.from_pydata(to_blender(v).tolist(), [], [list(map(int, f)) for f in faces])
    if uv is not None:
        lay = me.uv_layers.new()
        loops = np.zeros((len(me.loops), 2))
        k = 0
        for f in faces:
            for vi in f:
                loops[k] = uv[vi]
                k += 1
        lay.data.foreach_set("uv", loops.ravel())
    if colors is not None:
        ca = me.color_attributes.new("Col", "FLOAT_COLOR", "POINT")
        c = np.c_[colors[:, :3], np.ones(len(colors))]
        ca.data.foreach_set("color", c.ravel())
    me.polygons.foreach_set("use_smooth", [smooth] * len(me.polygons))
    ob = bpy.data.objects.new(name, me)
    bpy.context.scene.collection.objects.link(ob)
    if mat:
        ob.data.materials.append(mat)
    return ob


def camera_and_lights(target=(0, 0, 1.0), dist=3.2, yaw=0.0, lens=50, height=None):
    sc = bpy.context.scene
    cam = bpy.data.objects.new("cam", bpy.data.cameras.new("cam"))
    cam.data.lens = lens
    sc.collection.objects.link(cam)
    t = Vector(target)
    h = target[2] if height is None else height
    cam.location = Vector((t.x + math.sin(yaw) * dist, t.y + math.cos(yaw) * dist, h))
    cam.rotation_euler = (t - cam.location).to_track_quat("-Z", "Y").to_euler()
    sc.camera = cam
    for loc, e in (((3, -4, 4), 900), ((-4, -2, 2.5), 300), ((0, 4, 3), 500)):
        l = bpy.data.objects.new("l", bpy.data.lights.new("l", "AREA"))
        l.data.energy = e
        l.data.size = 3
        l.location = loc
        l.rotation_euler = (Vector((0, 0, 1)) - Vector(loc)).to_track_quat("-Z", "Y").to_euler()
        sc.collection.objects.link(l)
    return cam


def floor():
    me = bpy.data.meshes.new("floor")
    me.from_pydata([(-5, -5, 0), (5, -5, 0), (5, 5, 0), (-5, 5, 0)], [], [(0, 1, 2, 3)])
    ob = bpy.data.objects.new("floor", me)
    ob.data.materials.append(material("floor", (0.3, 0.28, 0.25), rough=0.9))
    bpy.context.scene.collection.objects.link(ob)


def render(path, w=900, h=900):
    sc = bpy.context.scene
    sc.render.resolution_x = w
    sc.render.resolution_y = h
    sc.render.filepath = path
    bpy.ops.render.render(write_still=True)
