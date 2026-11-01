"""Fige la pose d'un pilote dans la géométrie et l'ajoute (ou non) à un véhicule, puis exporte le glTF brut.

Le lecteur de GLB du jeu ne sait lire que des maillages statiques (ni peau, ni animation) : la pose enregistrée par
`pose.py` est donc appliquée au maillage (armature résolue) et le pilote devient un maillage ordinaire, à sa place dans le
repère du véhicule. Matériau du pilote renommé `body_paint` (masque de peinture dans l'occlusion, voir `build-glb.sh`).

Usage : blender -b -P tools/char/bake_rider.py -- <char-src/xxx> <action pose_xxx> <sortie.glb> [<véhicule.blend>]
  sans véhicule : seul le pilote est exporté (conducteur de la voiture, ajouté à l'exécution).
"""
import bpy, sys, os, re, json
from mathutils import Matrix, Vector

a = sys.argv[sys.argv.index("--") + 1:]
dossier, action, sortie = os.path.abspath(a[0]), a[1], os.path.abspath(a[2])
vehicule = os.path.abspath(a[3]) if len(a) > 3 else None
posed = os.path.join(dossier, "char_posed.blend")

if vehicule:
    bpy.ops.wm.open_mainfile(filepath=vehicule)
else:
    bpy.ops.wm.read_factory_settings(use_empty=True)
# Moto : on isole d'abord ses roues (`spin_F`, `spin_R`, origine sur l'essieu) : le jeu les fait tourner.
if vehicule:
    sys.path.insert(0, os.path.join(os.path.dirname(os.path.abspath(__file__)), "..", "moto"))
    import split_wheels
    numero = int(re.search(r"moto-(\d)", vehicule.replace("\\", "/")).group(1))
    print("RAPPORT roues", numero, split_wheels.split(numero))
# Le pilote pivote autour de son bassin (selle) : l'origine de son objet y est posée.
cfg = json.load(open(os.path.join(os.path.dirname(os.path.abspath(__file__)), "poses_manul.json"), encoding="utf-8"))
bassin = Vector(cfg["poses"][action.removeprefix("pose_")]["pelvis"])
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
# Les doigts sont animés en angles d'Euler (`grip`), mais le mode de rotation n'est pas conservé dans le .blend : sans ceci
# l'action est ignorée et les doigts restent tendus.
for pb in arm.pose.bones:
    if re.fullmatch(r"f[a-e]\d_[lr]", pb.name):
        pb.rotation_mode = "XYZ"
sc.frame_set(1)
bpy.context.view_layer.update()

# Maillage déformé par la pose : on l'évalue, on le copie dans un objet sans peau, en coordonnées du monde.
dg = bpy.context.evaluated_depsgraph_get()
figes = []
for m in meshes:
    ev = m.evaluated_get(dg)
    donnees = bpy.data.meshes.new_from_object(ev, preserve_all_data_layers=True, depsgraph=dg)
    donnees.transform(Matrix.Translation(-bassin) @ m.matrix_world)
    o = bpy.data.objects.new("rider_mesh", donnees)
    o.location = bassin
    sc.collection.objects.link(o)
    # Pivot vide au bassin (voir `split_wheels.pivot`) : le jeu incline ce nœud, que la compression ne déplace pas.
    pivot = bpy.data.objects.new("rider", None)
    sc.collection.objects.link(pivot)
    pivot.location = bassin
    bpy.context.view_layer.update()
    o.parent = pivot
    o.matrix_parent_inverse = pivot.matrix_world.inverted()
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
