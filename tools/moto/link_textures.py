"""Relie les textures de <dossier>/textures aux matériaux d'un .blend de moto, puis exporte le glTF brut.

Les FBX livrés ne référencent aucune image : on retrouve la texture d'un matériau par son nom
(`Carcass1` -> `Moto_full_model_Carcass1_BaseColor.jpg`, `borracha.002` -> `borracha_albedo.jpg`).
Usage : blender -b -P tools/moto/link_textures.py -- <dossier moto-src/moto-N>
"""
import bpy, sys, os, re

dossier = os.path.abspath(sys.argv[sys.argv.index("--") + 1])
bpy.ops.wm.open_mainfile(filepath=os.path.join(dossier, "moto.blend"))
tex_dir = os.path.join(dossier, "textures")

CANAUX = {"basecolor": "base", "albedo": "base", "metallic": "metal", "roughness": "rough",
          "normal": "normal", "emissive": "emit", "ao": "ao"}

# prefixe normalisé -> {canal: fichier}
jeux = {}
for f in os.listdir(tex_dir):
    stem, _ = os.path.splitext(f)
    m = re.match(r"(.+)_([A-Za-z]+)$", stem)
    if m and m.group(2).lower() in CANAUX:
        jeux.setdefault(m.group(1).lower(), {})[CANAUX[m.group(2).lower()]] = f

def trouve(nom):
    base = re.sub(r"\.\d+$", "", nom).lower()
    # correspondance exacte, sinon suffixe (`moto_full_model_carcass1`, `base_low_rama_tyl`)
    cands = [k for k in jeux if k == base] or [k for k in jeux if k.endswith("_" + base)]
    return jeux[cands[0]] if cands else None

def image(nom, couleur):
    img = bpy.data.images.load(os.path.join(tex_dir, nom), check_existing=True)
    img.colorspace_settings.name = "sRGB" if couleur else "Non-Color"
    return img

sans = []
for mat in bpy.data.materials:
    jeu = trouve(mat.name)
    if not jeu:
        sans.append(mat.name)
        continue
    mat.use_nodes = True
    nt = mat.node_tree
    bsdf = next((n for n in nt.nodes if n.type == "BSDF_PRINCIPLED"), None)
    if bsdf is None:
        continue
    for l in list(nt.links):
        if l.to_node == bsdf and l.to_socket.name in ("Base Color", "Metallic", "Roughness", "Emission Color", "Normal"):
            nt.links.remove(l)
    def tex(canal, couleur, socket, via=None):
        if canal not in jeu:
            return
        n = nt.nodes.new("ShaderNodeTexImage")
        n.image = image(jeu[canal], couleur)
        if via:
            nm = nt.nodes.new("ShaderNodeNormalMap")
            nt.links.new(n.outputs["Color"], nm.inputs["Color"])
            nt.links.new(nm.outputs["Normal"], bsdf.inputs[socket])
        else:
            nt.links.new(n.outputs["Color"], bsdf.inputs[socket])
    tex("base", True, "Base Color")
    tex("metal", False, "Metallic")
    tex("rough", False, "Roughness")
    tex("normal", False, "Normal", via=True)
    if "emit" in jeu:
        tex("emit", True, "Emission Color")
        bsdf.inputs["Emission Strength"].default_value = 1.0
print("RAPPORT materiaux sans texture :", sans)
print("RAPPORT lies :", len(bpy.data.materials) - len(sans), "/", len(bpy.data.materials))

bpy.ops.wm.save_as_mainfile(filepath=os.path.join(dossier, "moto.blend"), compress=True)
bpy.ops.export_scene.gltf(filepath=os.path.join(dossier, "moto_raw.glb"), export_format="GLB",
                          export_yup=True, export_apply=True)
