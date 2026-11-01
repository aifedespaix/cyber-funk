"""Fige la pose d'un pilote dans la géométrie et l'ajoute (ou non) à un véhicule, puis exporte le glTF brut.

Le lecteur de GLB du jeu ne sait lire que des maillages statiques (ni peau, ni animation) : la pose enregistrée par
`pose.py` est donc appliquée au maillage (armature résolue) et le pilote devient un maillage ordinaire, à sa place dans le
repère du véhicule. Matériau du pilote renommé `body_paint` (masque de peinture dans l'occlusion, voir `build-glb.sh`).

Usage : blender -b -P tools/char/bake_rider.py -- <char-src/xxx> <action pose_xxx> <sortie.glb> [<véhicule.blend>]
  sans véhicule : seul le pilote est exporté (conducteur de la voiture, ajouté à l'exécution).
"""
import bpy, sys, os

a = sys.argv[sys.argv.index("--") + 1:]
dossier, action, sortie = os.path.abspath(a[0]), a[1], os.path.abspath(a[2])
vehicule = os.path.abspath(a[3]) if len(a) > 3 else None
posed = os.path.join(dossier, "char_posed.blend")

if vehicule:
    bpy.ops.wm.open_mainfile(filepath=vehicule)
else:
    bpy.ops.wm.read_factory_settings(use_empty=True)
avant = {o.name for o in bpy.data.objects}
with bpy.data.libraries.load(posed, link=False) as (src, dst):
    dst.objects = list(src.objects)
    dst.actions = [n for n in src.actions if n == action]
sc = bpy.context.scene
for o in dst.objects:
    if o is not None:
        sc.collection.objects.link(o)
arm = next(o for o in dst.objects if o and o.type == "ARMATURE")
meshes = [o for o in dst.objects if o and o.type == "MESH"]
arm.animation_data_create()
arm.animation_data.action = bpy.data.actions[action]
sc.frame_set(1)
bpy.context.view_layer.update()

# Maillage déformé par la pose : on l'évalue, on le copie dans un objet sans peau, en coordonnées du monde.
dg = bpy.context.evaluated_depsgraph_get()
figes = []
for m in meshes:
    ev = m.evaluated_get(dg)
    donnees = bpy.data.meshes.new_from_object(ev, preserve_all_data_layers=True, depsgraph=dg)
    donnees.transform(m.matrix_world)
    o = bpy.data.objects.new("pilote", donnees)
    sc.collection.objects.link(o)
    figes.append(o)
# On retire le personnage d'origine (peau, armature, racine) : seuls restent les maillages figés.
noms = [o.name for o in dst.objects if o is not None]
for n in noms:
    if n in bpy.data.objects:
        bpy.data.objects.remove(bpy.data.objects[n])
for o in figes:
    for mat in o.data.materials:
        if mat is not None:
            mat.name = "body_paint"
print("RAPPORT pilote", len(figes), "objet(s),", sum(len(o.data.polygons) for o in figes), "faces")
bpy.ops.export_scene.gltf(filepath=sortie, export_format="GLB", export_yup=True, export_apply=True,
                          export_animations=False, export_skins=False)
