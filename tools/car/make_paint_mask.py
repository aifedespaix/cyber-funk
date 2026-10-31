"""Builds the paint mask (white = bodywork paint) from the diffuse texture.
Usage: python -I make_paint_mask.py <diffuse.png> <out_mask.png>
The mask is later embedded in the GLB (occlusion texture, R channel) by build-glb.mjs."""
import sys
import numpy as np
from PIL import Image, ImageFilter

src, dst = sys.argv[1], sys.argv[2]
im = Image.open(src).convert("RGB")
a = np.asarray(im, dtype=np.float32) / 255.0
mx, mn = a.max(2), a.min(2)
d = mx - mn
s = np.where(mx > 1e-5, d / np.maximum(mx, 1e-5), 0)
r, g, b = a[..., 0], a[..., 1], a[..., 2]
h = np.zeros_like(mx)
m = d > 1e-5
rm = m & (mx == r); gm = m & (mx == g) & ~rm; bm = m & ~rm & ~gm
h[rm] = ((g - b)[rm] / d[rm]) % 6
h[gm] = (b - r)[gm] / d[gm] + 2
h[bm] = (r - g)[bm] / d[bm] + 4
h /= 6.0
# violet paint: hue ~0.66..0.86, saturated enough, not too dark
hue = np.clip((h - 0.64) / 0.04, 0, 1) * np.clip((0.88 - h) / 0.04, 0, 1)
sat = np.clip((s - 0.14) / 0.12, 0, 1)
val = np.clip((v := mx) - 0.12, 0, 0.1) / 0.1
mask = hue * sat * val
img = Image.fromarray((mask * 255).astype(np.uint8), "L")
img = img.filter(ImageFilter.MaxFilter(3)).filter(ImageFilter.GaussianBlur(0.8))   # close pin-holes, soft edge
img.save(dst)
print("paint coverage: %.1f %% of texture" % (np.asarray(img).mean() / 2.55))
