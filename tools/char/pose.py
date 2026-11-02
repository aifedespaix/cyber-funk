"""Pose un personnage humanoïde sur un véhicule (cinématique inverse) et enregistre la pose comme action Blender.

Chaque pose est une action d'une seule image `pose_<nom>` : le bassin va au siège, les mains aux poignées, les pieds
aux pédales ou aux repose-pieds. Les os animés sont en repère du véhicule (le personnage est posé au centre du véhicule,
sans transformation), le GLB exporté porte donc la pose sans que le jeu ait à la calculer. L'échelle du pilote est
animée sur l'os racine : une même armature sert au conducteur de la voiture (plus grand) et au passager des motos.

Usage (depuis la racine du dépôt) :
  blender -b -P tools/char/pose.py -- <dossier char-src/xxx> <poses.json> [<nom de pose> ...]
`poses.json` : voir tools/char/poses_manul.json.
"""
import bpy, sys, os, json, math, mathutils
from mathutils import Vector, Matrix

a = sys.argv[sys.argv.index("--") + 1:]
dossier = os.path.abspath(a[0])
cfg = json.load(open(a[1], encoding="utf-8"))
noms = a[2:] or list(cfg["poses"])

bpy.ops.wm.open_mainfile(filepath=os.path.join(dossier, "char.blend"))
arm = next(o for o in bpy.data.objects if o.type == "ARMATURE")
B = cfg["bones"]
dg = bpy.context.evaluated_depsgraph_get

def vers_mode(mode):
    bpy.context.view_layer.objects.active = arm
    arm.select_set(True)
    bpy.ops.object.mode_set(mode=mode)

def repos():
    """Toutes les poses d'os à zéro : le FBX livre une pose (bras levés) qui n'est pas la pose de repos."""
    for pb in arm.pose.bones:
        pb.location = (0, 0, 0); pb.scale = (1, 1, 1)
        pb.rotation_mode = "QUATERNION"; pb.rotation_quaternion = (1, 0, 0, 0)
        for c in list(pb.constraints):
            pb.constraints.remove(c)
    bpy.context.view_layer.update()

def monde_repos(nom, queue=False):
    b = arm.data.bones[nom]
    return arm.matrix_world @ (b.tail_local if queue else b.head_local)

def vide(nom, pos):
    e = bpy.data.objects.new(nom, None)
    bpy.context.scene.collection.objects.link(e)
    e.location = Vector(pos)
    return e

def pos_monde(nom, queue=False):
    d = dg()
    ev = arm.evaluated_get(d)
    pb = ev.pose.bones[nom]
    return ev.matrix_world @ (pb.tail if queue else pb.head)

def ik(os_, longueur, cible, pole):
    pb = arm.pose.bones[os_]
    c = pb.constraints.new("IK")
    c.target = cible; c.pole_target = pole
    c.chain_count = longueur
    return c

def choisit_angle(contrainte, os_genou, pole_pos):
    """Essaie quatre angles de pôle et garde celui qui plie le membre vers le pôle."""
    meilleur, dmin = 0, 1e9
    for deg in (0, 90, 180, -90):
        contrainte.pole_angle = math.radians(deg)
        bpy.context.view_layer.update()
        d = (pos_monde(os_genou) - Vector(pole_pos)).length
        if d < dmin:
            meilleur, dmin = deg, d
    contrainte.pole_angle = math.radians(meilleur)
    bpy.context.view_layer.update()
    return meilleur

def pose(nom, p):
    vers_mode("POSE")
    repos()
    racine = arm.pose.bones[B["root"]]
    pelvis_repos = monde_repos(B["pelvis_bone"], queue=True)
    wb = arm.matrix_world @ arm.data.bones[B["root"]].matrix_local
    k = p.get("scale", 1.0)
    lean = math.radians(p.get("lean_deg", 0.0))   # > 0 : buste penché vers l'avant (-Y)
    cible = (Matrix.Translation(Vector(p["pelvis"])) @ Matrix.Rotation(lean, 4, "X") @ Matrix.Scale(k, 4)
             @ Matrix.Translation(-pelvis_repos) @ wb)
    racine.matrix = arm.matrix_world.inverted() @ cible
    bpy.context.view_layer.update()
    # Chaînes : (os en bout de chaîne, longueur, cible, pôle, os du coude/genou pour choisir l'angle)
    vides = []
    for cote in ("l", "r"):
        for membre, longueur in (("leg", 2), ("arm", 2)):
            cle = f"{membre}_{cote}"
            if cle not in p:
                continue
            bout = B[cle]["tip"]
            t = vide(f"t_{nom}_{cle}", p[cle]["target"])
            po = vide(f"p_{nom}_{cle}", p[cle]["pole"])
            vides += [t, po]
            c = ik(bout, longueur, t, po)
            choisit_angle(c, B[cle]["mid"], p[cle]["pole"])
    # Orientation des pieds : tangage donné, le reste suit le tibia.
    bpy.context.view_layer.update()
    for cote in ("l", "r"):
        if f"foot_{cote}" in p:
            f = arm.pose.bones[B[f"foot_{cote}"]]
            f.rotation_mode = "XYZ"
            f.rotation_euler = (math.radians(p[f"foot_{cote}"]), 0, 0)
    # Poignets : `hand_roll` (°) tourne la main autour de l'avant-bras ; > 0 ou < 0 selon le côté, à régler à l'œil.
    for cote, deg in p.get("hand_roll", {}).items():
        os_ = arm.pose.bones[f"Hand1_{cote}"]
        os_.rotation_mode = "XYZ"
        os_.rotation_euler = (0, math.radians(deg), 0)
    # Prise automatique : `wheel` = {centre, normale, rayon (ligne centrale de la jante), tube (rayon de la section)}.
    # Pour chaque doigt on cherche la fraction de la flexion maximale `grip` pour laquelle son bout vient toucher le tube :
    # les phalanges se referment juste assez pour saisir la jante, sans la traverser ni rester ouvertes.
    def distance_jante(pt, w):
        c, n = Vector(w["centre"]), Vector(w["normale"]).normalized()
        d = pt - c
        ax = d.dot(n)
        return math.hypot((d - n * ax).length - w["rayon"], ax)
    for cote, phalanges in p.get("grip", {}).items():
        w = p.get("wheel")
        for doigt in ("fb", "fc", "fd", "fe"):
            os_doigt = [arm.pose.bones[f"{doigt}{r}_{cote}"] for r in (1, 2, 3, 4)]
            maxi = (phalanges[0], phalanges[0], phalanges[1], phalanges[2])
            meilleur, cout = 1.0, 1e9
            for i in range(0, 41):
                t = i / 40
                for b, deg in zip(os_doigt, maxi):
                    b.rotation_mode = "XYZ"
                    b.rotation_euler = (math.radians(deg * t), 0, 0)
                if w is None:
                    break
                bpy.context.view_layer.update()
                c_ = abs(distance_jante(pos_monde(f"{doigt}4_{cote}", queue=True), w) - w["tube"])
                if c_ < cout:
                    meilleur, cout = t, c_
            for b, deg in zip(os_doigt, maxi):
                b.rotation_euler = (math.radians(deg * meilleur), 0, 0)
            if w:
                print("RAPPORT prise", cote, doigt, "flexion", round(meilleur, 2), "écart au tube (m)", round(cout, 3))
    # Tête : `head_deg` > 0 relève le regard (le cou tourne autour de sa base, en sens inverse du buste penché).
    if p.get("head_deg"):
        bpy.context.view_layer.update()
        cou = arm.pose.bones["Neck"]
        pivot = cou.head.copy()
        rot = Matrix.Translation(pivot) @ Matrix.Rotation(-math.radians(p["head_deg"]), 4, "X") @ Matrix.Translation(-pivot)
        cou.matrix = rot @ cou.matrix
        bpy.context.view_layer.update()
    # Enregistre la pose visuelle (contraintes résolues) dans une action, puis retire les contraintes.
    bpy.ops.pose.select_all(action="SELECT")
    bpy.ops.nla.bake(frame_start=1, frame_end=1, only_selected=False, visual_keying=True,
                     clear_constraints=True, use_current_action=False, bake_types={"POSE"})
    act = arm.animation_data.action
    act.name = f"pose_{nom}"
    act.use_fake_user = True
    arm.animation_data.action = None
    for v in vides:
        bpy.data.objects.remove(v)
    vers_mode("OBJECT")

for n in noms:
    pose(n, cfg["poses"][n])
    print("RAPPORT pose", n, "ok")
arm.animation_data_create()
arm.animation_data.action = bpy.data.actions[f"pose_{noms[0]}"]
sortie = os.path.join(dossier, "char_posed.blend")
bpy.ops.wm.save_as_mainfile(filepath=sortie, compress=True)
print("RAPPORT sauvé", sortie)
