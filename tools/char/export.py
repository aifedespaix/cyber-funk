"""Exporte un personnage en glTF brut `char_raw.glb` (squelette et poids conservés, matériau `body_paint`).

Si `char_posed.blend` existe (sortie de `pose.py`), ses actions `pose_<véhicule>` sont exportées comme animations d'une
image : le jeu joue `pose_car`, `pose_moto1`... pour poser le pilote. Sinon l'export est celui de `char.blend`, sans animation.
La pose courante est remise à zéro : le GLB est en T au repos (le FBX livrait une pose bras levés).

Usage : blender -b -P tools/char/export.py -- <dossier char-src/xxx>
"""
import bpy, sys, os

dossier = os.path.abspath(sys.argv[sys.argv.index("--") + 1])
posed = os.path.join(dossier, "char_posed.blend")
bpy.ops.wm.open_mainfile(filepath=posed if os.path.exists(posed) else os.path.join(dossier, "char.blend"))
bpy.data.materials[0].name = "body_paint"   # un seul matériau : le shader de teinte le repère par ce nom
arm = next(o for o in bpy.data.objects if o.type == "ARMATURE")
poses = [a for a in bpy.data.actions if a.name.startswith("pose_")]
for a in list(bpy.data.actions):
    if a not in poses:
        bpy.data.actions.remove(a)
if arm.animation_data:
    arm.animation_data.action = None
for pb in arm.pose.bones:
    pb.location = (0, 0, 0); pb.scale = (1, 1, 1)
    pb.rotation_mode = "QUATERNION"; pb.rotation_quaternion = (1, 0, 0, 0)
print("RAPPORT actions", [a.name for a in poses])
bpy.ops.export_scene.gltf(filepath=os.path.join(dossier, "char_raw.glb"), export_format="GLB", export_yup=True,
                          export_apply=False, export_skins=True, export_animations=bool(poses),
                          export_animation_mode="ACTIONS", export_force_sampling=True,
                          export_optimize_animation_size=False)
