"""Génère les icônes d'application depuis assets/images/logo-app.svg.

Sortie (versionnée, un build n'a besoin d'aucun outil externe) : assets/images/icons/icon-<n>.png
pour n dans 16, 32, 48, 64, 128, 256, 512, et app.ico (16/32/48/64/128/256).

Outils requis, sur la machine du développeur seulement : ImageMagick (`magick`, avec son délégué SVG)
et Python 3 avec Pillow. Usage : python3 -I tools/icons/generate.py
Ce script ne fait pas partie du build.
"""

import subprocess
import sys
import tempfile
from pathlib import Path

from PIL import Image

RACINE = Path(__file__).resolve().parents[2]
SOURCE = RACINE / "assets" / "images" / "logo-app.svg"
SORTIE = RACINE / "assets" / "images" / "icons"
TAILLES_PNG = [16, 32, 48, 64, 128, 256, 512]
TAILLES_ICO = [16, 32, 48, 64, 128, 256]


def main() -> int:
    SORTIE.mkdir(parents=True, exist_ok=True)
    with tempfile.TemporaryDirectory() as tmp:
        base = Path(tmp) / "base.png"
        # Rendu à 1024 px puis réduction LANCZOS : plus net aux petites tailles que le rendu direct.
        subprocess.run(
            ["magick", "-background", "none", "-density", "768", str(SOURCE),
             "-resize", "1024x1024", f"PNG32:{base}"],
            check=True,
        )
        grand = Image.open(base).convert("RGBA")
        images = {}
        for n in TAILLES_PNG:
            img = grand.resize((n, n), Image.LANCZOS)
            img.save(SORTIE / f"icon-{n}.png", optimize=True)
            images[n] = img
    images[256].save(
        SORTIE / "app.ico",
        format="ICO",
        sizes=[(n, n) for n in TAILLES_ICO],
        append_images=[],
    )
    print(f"icônes écrites dans {SORTIE}")
    return 0


if __name__ == "__main__":
    sys.exit(main())
