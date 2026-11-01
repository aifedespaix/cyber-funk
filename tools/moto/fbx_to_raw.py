"""Importe le FBX d'une moto dans Blender, sauve le .blend de travail et exporte le glTF brut.

Usage : blender -b -P tools/moto/fbx_to_raw.py -- <dossier moto-src/moto-N> <fichier.fbx>
Sorties (ignorées par git) : <dossier>/moto.blend et <dossier>/moto_raw.glb
"""
import bpy, sys, os

dossier, fbx = sys.argv[sys.argv.index("--") + 1:][:2]
bpy.ops.wm.read_factory_settings(use_empty=True)
chemin = os.path.join(dossier, "source", fbx)
try:
    bpy.ops.import_scene.fbx(filepath=chemin)
except RuntimeError:
    # Blender 5.1 : l'import Python plante sur les lumières du FBX ; l'import C++ ne les lit pas.
    bpy.ops.wm.fbx_import(filepath=chemin)

# Rapport : taille, triangles, matériaux (sert à choisir le ratio de simplification).
tris = 0
for o in bpy.data.objects:
    if o.type == "MESH":
        tris += sum(len(p.vertices) - 2 for p in o.data.polygons)
print("RAPPORT objets=%d tris=%d materiaux=%d" % (len(bpy.data.objects), tris, len(bpy.data.materials)))
for o in bpy.data.objects:
    print("RAPPORT", o.type, o.name, tuple(round(x, 2) for x in o.dimensions))

bpy.ops.wm.save_as_mainfile(filepath=os.path.join(dossier, "moto.blend"), compress=True)
bpy.ops.export_scene.gltf(
    filepath=os.path.join(dossier, "moto_raw.glb"),
    export_format="GLB", export_yup=True, export_apply=True,
    export_image_format="AUTO", export_materials="EXPORT",
)
