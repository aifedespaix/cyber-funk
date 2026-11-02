"""Normalise une moto (échelle, orientation, origine) et exporte le glTF brut `moto_raw.glb`.

Convention du projet (comme la voiture) : mètres, +Y haut, la moto regarde +Z (glTF), son côté gauche est +X.
En repère Blender (Z haut) cela donne : avant = -Y, gauche = +X. L'origine est au sol, au centre de l'emprise.
Les motos livrées ont chacune leur échelle et leur axe avant ; on les range dans un nœud racine `moto`
(échelle + rotation), sans toucher aux maillages. Idempotent : sans effet si `moto` existe déjà.

Usage : blender -b -P tools/moto/normalize.py -- <dossier moto-src/moto-N> <rotation Z en degrés>
  rotation Z : 0 si l'avant du modèle est déjà -Y (motos 2 et 3), 90 si l'avant est -X (moto 1).
"""
import bpy, sys, os, math, mathutils

LONGUEUR_M = 2.2
dossier = os.path.abspath(sys.argv[sys.argv.index("--") + 1])
rot_z = float(sys.argv[sys.argv.index("--") + 2])
bpy.ops.wm.open_mainfile(filepath=os.path.join(dossier, "moto.blend"))

def emprise():
    mn = mathutils.Vector((1e9,) * 3); mx = mathutils.Vector((-1e9,) * 3)
    for o in bpy.data.objects:
        if o.type == "MESH":
            for c in o.bound_box:
                w = o.matrix_world @ mathutils.Vector(c)
                for i in range(3):
                    mn[i] = min(mn[i], w[i]); mx[i] = max(mx[i], w[i])
    return mn, mx

if "moto" not in bpy.data.objects:
    for o in list(bpy.data.objects):
        if o.type in ("CAMERA", "LIGHT"):
            bpy.data.objects.remove(o)
    racine = bpy.data.objects.new("moto", None)
    bpy.context.scene.collection.objects.link(racine)
    for o in bpy.data.objects:
        if o is not racine and o.parent is None:
            o.parent = racine
    bpy.context.view_layer.update()
    racine.rotation_euler.z = math.radians(rot_z)
    bpy.context.view_layer.update()
    mn, mx = emprise()
    racine.scale = (LONGUEUR_M / (mx.y - mn.y),) * 3   # l'avant est sur -Y : la longueur est l'étendue en Y
    bpy.context.view_layer.update()
    mn, mx = emprise()
    racine.location = (-(mn.x + mx.x) / 2, -(mn.y + mx.y) / 2, -mn.z)
    bpy.context.view_layer.update()
mn, mx = emprise()
print("RAPPORT emprise", tuple(round(x, 3) for x in mn), tuple(round(x, 3) for x in mx))

bpy.ops.wm.save_as_mainfile(filepath=os.path.join(dossier, "moto.blend"), compress=True)
bpy.ops.export_scene.gltf(filepath=os.path.join(dossier, "moto_raw.glb"), export_format="GLB",
                          export_yup=True, export_apply=True)
