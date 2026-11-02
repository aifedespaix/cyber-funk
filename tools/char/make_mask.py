"""Masque de peinture d'un personnage : les pixels de la teinte de carrosserie dans la texture de couleur.

Le masque (canal R, blanc = peinture) est embarqué dans le GLB comme texture d'occlusion (même convention que la voiture,
voir `tools/car/inject-mask.mjs`) ; le shader ne repeint que ces zones, le métal nu et la saleté restent intacts.
Écrit aussi la luminance moyenne de la peinture d'origine, utile à la teinte par luminance du shader.

Usage : python -I tools/char/make_mask.py <base_color.png> <teinte min> <teinte max> <saturation min> <sortie.png>
  teintes en degrés (0-360).
"""
import sys
import numpy as np
from PIL import Image, ImageFilter

src, h0, h1, smin, sortie = sys.argv[1], float(sys.argv[2]), float(sys.argv[3]), float(sys.argv[4]), sys.argv[5]
Image.MAX_IMAGE_PIXELS = None
img = Image.open(src).convert("RGB")
if img.width > 2048:
    img = img.resize((2048, 2048), Image.LANCZOS)
hsv = np.asarray(img.convert("HSV"), dtype=np.float32)
h = hsv[..., 0] * 360.0 / 255.0
s = hsv[..., 1] / 255.0
v = hsv[..., 2] / 255.0
masque = ((h >= h0) & (h <= h1) & (s >= smin) & (v >= 0.25)).astype(np.uint8) * 255
m = Image.fromarray(masque).filter(ImageFilter.MaxFilter(3)).filter(ImageFilter.GaussianBlur(0.8))
m = m.resize((1024, 1024), Image.LANCZOS)
m.convert("RGB").save(sortie)
rgb = np.asarray(img, dtype=np.float32) / 255.0
sel = masque > 0
lin = rgb[sel] ** 2.2
lum = (0.2126 * lin[:, 0] + 0.7152 * lin[:, 1] + 0.0722 * lin[:, 2]).mean()
print("MASQUE couverture=%.1f%% luminance_moyenne=%.4f teinte_moyenne=%.1f" % (100 * sel.mean(), lum, h[sel].mean()))
