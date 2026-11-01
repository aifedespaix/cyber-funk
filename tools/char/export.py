"""Exporte `char.blend` en glTF brut `char_raw.glb` (squelette et poids conservés, sans animation, matériau `body_paint`).

Usage : blender -b -P tools/char/export.py -- <dossier char-src/xxx>
"""
import bpy, sys, os

dossier = os.path.abspath(sys.argv[sys.argv.index("--") + 1])
bpy.ops.wm.open_mainfile(filepath=os.path.join(dossier, "char.blend"))
# Un seul matériau : le shader de teinte le repère par ce nom.
bpy.data.materials[0].name = "body_paint"
for a in list(bpy.data.actions):
    bpy.data.actions.remove(a)
bpy.ops.export_scene.gltf(filepath=os.path.join(dossier, "char_raw.glb"), export_format="GLB", export_yup=True,
                          export_apply=False, export_animations=False, export_skins=True)
