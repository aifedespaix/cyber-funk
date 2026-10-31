# car-viewer — banc d'essai Three.js du modèle de voiture

Valide le modèle sans lancer Bevy : contrat des nœuds et matériaux, lumières, animations, mode volant, vue conducteur.
Approximation Three.js (pas le rendu Bevy) : sert à vérifier les pivots, les zones lumineuses et le cadrage.

```bash
cd tools/car-viewer
npm install        # une fois
npm run dev        # http://localhost:5199
```

- Sélecteur « Voiture » : tous les `.glb` et `.fbx` de `assets/car-src/` et `assets/models/car/` (par défaut `car_raw.glb`).
  `?model=<chemin>` dans l'URL choisit la voiture au démarrage. Un FBX brut (ex. Dirty car) s'affiche avec ses textures
  diffuses ; le panneau de contrat liste ce qu'il reste à faire dans Blender.
- La page se recharge toute seule quand un `.glb` ou `.fbx` de `assets/car-src/` ou `assets/models/car/` change.
- `V` : vue conducteur (position de `cam_cockpit`), glisser pour regarder autour.
- Lumières projetées : réglables, déplaçables au gizmo, « Copier la config (JSON) » pour les reporter dans Bevy.
- Sens du volant et des essuie-glaces : `STEERING_WHEEL_SIGN` / `WIPER_SIGN` dans `src/config.ts`.
