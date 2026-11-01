"""Sépare les deux roues d'une moto en objets à part, `spin_F` (avant) et `spin_R` (arrière), origine sur l'essieu.

Pourquoi : le jeu fait tourner les roues (`rider::anim`). Les sources ne les isolent pas toujours (la moto 2 est un seul
maillage). Une roue est l'ensemble des îlots de géométrie (pneu, jante, disque, moyeu) dont le centre est sur l'essieu et dont
l'étendue ne dépasse pas le diamètre du pneu ; les fourches, l'axe et les freins (étendus ou décentrés) restent fixes.

Le nœud `spin_F` / `spin_R` exporté est un pivot vide sur l'essieu (translation seule) : le jeu le fait tourner autour de X.

Appelé par `tools/char/bake_rider.py` sur le `.blend` normalisé de la moto (repère Blender : Z haut, l'avant à −Y).

Les mesures (essieu `y`, `z` et diamètre du pneu, en m) viennent de l'analyse des sources par îlots ; si une moto change,
relancer l'analyse (voir assets/moto-src/README.md) et ajuster `WHEELS`.
"""
import bmesh
import bpy
from mathutils import Vector

# moto -> {roue: (y essieu, z essieu, diamètre du pneu)}
WHEELS = {
    1: {"F": (-0.82, 0.27, 0.55), "R": (0.84, 0.24, 0.55)},
    2: {"F": (-0.73, 0.37, 0.73), "R": (0.73, 0.37, 0.73)},
    3: {"F": (-0.80, 0.34, 0.62), "R": (0.74, 0.36, 0.72)},
}
MIN_VERTS = 60
CENTER_TOL = 0.06      # le centre de l'îlot est à moins de 6 cm de l'essieu
SQUARE_TOL = 0.12      # étendues y et z presque égales : un disque, pas une barre
MAX_WIDTH = 0.4        # une roue est fine ; un axe qui traverse la moto est exclu


def _islands(obj):
    """Îlots (listes d'indices de sommets) d'un maillage, avec leur boîte englobante dans le monde."""
    bm = bmesh.new()
    bm.from_mesh(obj.data)
    bm.verts.ensure_lookup_table()
    mw = obj.matrix_world
    seen, out = set(), []
    for v in bm.verts:
        if v.index in seen:
            continue
        stack, comp = [v], []
        seen.add(v.index)
        while stack:
            c = stack.pop()
            comp.append(c.index)
            for e in c.link_edges:
                w = e.other_vert(c)
                if w.index not in seen:
                    seen.add(w.index)
                    stack.append(w)
        if len(comp) < MIN_VERTS:
            continue
        ps = [mw @ bm.verts[i].co for i in comp]
        lo = Vector((min(p.x for p in ps), min(p.y for p in ps), min(p.z for p in ps)))
        hi = Vector((max(p.x for p in ps), max(p.y for p in ps), max(p.z for p in ps)))
        out.append((comp, lo, hi))
    bm.free()
    return out


def _belongs(lo, hi, axle):
    y, z, diameter = axle
    size = hi - lo
    centre = (lo + hi) / 2
    return (
        size.x < MAX_WIDTH
        and abs(centre.y - y) < CENTER_TOL
        and abs(centre.z - z) < CENTER_TOL
        and size.y <= diameter + 0.04
        and size.z <= diameter + 0.04
        and abs(size.y - size.z) < SQUARE_TOL
    )


def _separate(obj, vertex_ids):
    """Détache les sommets `vertex_ids` de `obj` dans un nouvel objet, rendu."""
    before = set(bpy.data.objects.keys())
    bpy.ops.object.select_all(action="DESELECT")
    bpy.context.view_layer.objects.active = obj
    obj.select_set(True)
    bpy.ops.object.mode_set(mode="EDIT")
    bm = bmesh.from_edit_mesh(obj.data)
    bm.verts.ensure_lookup_table()
    for v in bm.verts:
        v.select_set(False)
    for i in vertex_ids:
        bm.verts[i].select_set(True)
    bm.select_flush_mode()
    bmesh.update_edit_mesh(obj.data)
    bpy.ops.mesh.separate(type="SELECTED")
    bpy.ops.object.mode_set(mode="OBJECT")
    return [bpy.data.objects[k] for k in bpy.data.objects.keys() if k not in before]


def pivot(name, mesh_obj):
    """Un nœud vide `name`, sur l'origine de `mesh_obj`, dont `mesh_obj` devient l'enfant. C'est lui que le jeu fait tourner.

    Pourquoi : la compression du GLB (meshopt) range les sommets en entiers et reporte l'échelle, souvent non uniforme, sur le
    nœud du maillage, dont l'origine devient le centre de sa boîte. Un nœud vide, sans échelle, garde son origine exacte."""
    empty = bpy.data.objects.new(name, None)
    bpy.context.scene.collection.objects.link(empty)
    empty.location = mesh_obj.matrix_world.translation
    bpy.context.view_layer.update()
    mesh_obj.parent = empty
    mesh_obj.matrix_parent_inverse = empty.matrix_world.inverted()
    return empty


def split(moto):
    """Crée `spin_F` et `spin_R` pour la moto numéro `moto` (1 à 3). Rend le nombre de sommets déplacés par roue."""
    meshes = [o for o in bpy.data.objects if o.type == "MESH"]
    bpy.ops.object.select_all(action="DESELECT")
    for o in meshes:
        o.select_set(True)
    bpy.context.view_layer.objects.active = meshes[0]
    bpy.ops.object.make_single_user(object=True, obdata=True)
    report = {}
    for side, axle in WHEELS[moto].items():
        parts = []
        for obj in [o for o in bpy.data.objects if o.type == "MESH" and not o.name.startswith("spin_")]:
            ids = [i for comp, lo, hi in _islands(obj) if _belongs(lo, hi, axle) for i in comp]
            if ids:
                parts += _separate(obj, ids)
        if not parts:
            raise RuntimeError(f"moto {moto} : aucune géométrie de roue {side} trouvée autour de {axle}")
        bpy.ops.object.select_all(action="DESELECT")
        for p in parts:
            p.select_set(True)
        bpy.context.view_layer.objects.active = parts[0]
        if len(parts) > 1:
            bpy.ops.object.join()
        wheel = bpy.context.view_layer.objects.active
        wheel.name = f"spin_{side}_mesh"
        wheel.data.name = f"spin_{side}_mesh"
        bpy.context.scene.cursor.location = (0.0, axle[0], axle[1])
        bpy.ops.object.origin_set(type="ORIGIN_CURSOR")
        # Nœud simple : sans parent, rotation et échelle appliquées au maillage. Les parents des sources portent des rotations
        # et des échelles (parfois non uniformes) : tourner autour de l'axe local X y déformerait la roue.
        world = wheel.matrix_world.copy()
        wheel.parent = None
        wheel.matrix_world = world
        bpy.ops.object.select_all(action="DESELECT")
        wheel.select_set(True)
        bpy.context.view_layer.objects.active = wheel
        bpy.ops.object.transform_apply(location=False, rotation=True, scale=True)
        pivot(f"spin_{side}", wheel)
        report[side] = len(wheel.data.vertices)
    return report
