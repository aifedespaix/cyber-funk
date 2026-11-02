"""Importe un personnage (FBX riggé), relie ses textures, le range au repère du projet et sauve `char.blend`.

Repère du projet : mètres, +Y haut, le personnage regarde +Z (glTF), pieds à l'origine. En repère Blender (Z haut) :
avant = -Y. Les FBX livrés ont chacun leur échelle (le manul fait 16 unités de haut, le robot 2,2) ; on n'applique
l'échelle qu'au nœud racine `perso` : le squelette et les poids ne sont pas touchés.

Usage : blender -b -P tools/char/prepare.py -- <dossier char-src/xxx> <fichier.fbx> <hauteur m> <rotation Z deg>
"""
import bpy, sys, os, math, mathutils, json

a = sys.argv[sys.argv.index("--") + 1:]
dossier, fbx, hauteur, rot_z = os.path.abspath(a[0]), a[1], float(a[2]), float(a[3])
TEX = {  # canal -> fichier, par personnage
    "manul": dict(base="RobotMANUL_Base_color.png", metal="RobotMANUL_Metallic.png", rough="RobotMANUL_Roughness.png",
                  normal="RobotMANUL_Normal_OpenGL.png"),
    "robot": dict(base="T2.png", metal="M.png", rough="R.png", normal="N.png", emit="Ee.png"),
}[os.path.basename(dossier)]

bpy.ops.wm.read_factory_settings(use_empty=True)
chemin = os.path.join(dossier, "source", fbx)
try:
    bpy.ops.import_scene.fbx(filepath=chemin)
except RuntimeError:
    bpy.ops.wm.fbx_import(filepath=chemin)

for o in list(bpy.data.objects):
    if o.type in ("CAMERA", "LIGHT"):
        bpy.data.objects.remove(o)

# Matériau unique : on relie les textures.
for mat in bpy.data.materials:
    mat.use_nodes = True
    nt = mat.node_tree
    bsdf = next(n for n in nt.nodes if n.type == "BSDF_PRINCIPLED")
    for l in list(nt.links):
        if l.to_node == bsdf:
            nt.links.remove(l)
    def tex(canal, couleur, socket, normal=False):
        if canal not in TEX:
            return
        n = nt.nodes.new("ShaderNodeTexImage")
        n.image = bpy.data.images.load(os.path.join(dossier, "textures", TEX[canal]), check_existing=True)
        n.image.colorspace_settings.name = "sRGB" if couleur else "Non-Color"
        if normal:
            nm = nt.nodes.new("ShaderNodeNormalMap")
            nt.links.new(n.outputs["Color"], nm.inputs["Color"])
            nt.links.new(nm.outputs["Normal"], bsdf.inputs["Normal"])
        else:
            nt.links.new(n.outputs["Color"], bsdf.inputs[socket])
    tex("base", True, "Base Color"); tex("metal", False, "Metallic"); tex("rough", False, "Roughness")
    tex("normal", False, "Normal", normal=True)
    if "emit" in TEX:
        tex("emit", True, "Emission Color")
        bsdf.inputs["Emission Strength"].default_value = 1.0

racine = bpy.data.objects.new("perso", None)
bpy.context.scene.collection.objects.link(racine)
for o in bpy.data.objects:
    if o is not racine and o.parent is None:
        o.parent = racine
bpy.context.view_layer.update()
racine.rotation_euler.z = math.radians(rot_z)
bpy.context.view_layer.update()

def emprise():
    mn = mathutils.Vector((1e9,) * 3); mx = mathutils.Vector((-1e9,) * 3)
    for o in bpy.data.objects:
        if o.type == "MESH":
            # Emprise du maillage déformé par l'armature en pose de repos : on prend les sommets monde.
            for v in o.data.vertices:
                w = o.matrix_world @ v.co
                for i in range(3):
                    mn[i] = min(mn[i], w[i]); mx[i] = max(mx[i], w[i])
    return mn, mx

mn, mx = emprise()
racine.scale = (hauteur / (mx.z - mn.z),) * 3
bpy.context.view_layer.update()
mn, mx = emprise()
racine.location = (-(mn.x + mx.x) / 2, -(mn.y + mx.y) / 2, -mn.z)
bpy.context.view_layer.update()
mn, mx = emprise()
print("RAPPORT emprise", tuple(round(x, 3) for x in mn), tuple(round(x, 3) for x in mx))
for o in bpy.data.objects:
    if o.type == "MESH":
        print("RAPPORT mesh", o.name, "poids", len(o.vertex_groups), "modifs", [m.type for m in o.modifiers])
bpy.ops.wm.save_as_mainfile(filepath=os.path.join(dossier, "char.blend"), compress=True)
