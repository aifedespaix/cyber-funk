import GUI from 'lil-gui';
import { GEARS } from './rig';
import { LIGHT_DEFS, type LightSettings } from './lights';
import type { AppState, Gear } from './state';

/** Ce que le panneau demande à l'application. */
export interface UiHooks {
  loadModel(path: string): void;
  reload(): void;
  setFps(on: boolean): void;
  resetView(): void;
  copyLights(): void;
  gizmo(id: string | null): void;
  /** Vrai si le modèle courant permet le décollage (les 4 roues sont isolables). */
  hoverSupported(): boolean;
}

/** Interpole `fn(0..1)` sur `ms` millisecondes. */
function tween(ms: number, fn: (k: number) => void): Promise<void> {
  return new Promise((done) => {
    const t0 = performance.now();
    const step = () => {
      const k = Math.min((performance.now() - t0) / ms, 1);
      fn(k);
      if (k < 1) requestAnimationFrame(step);
      else done();
    };
    step();
  });
}

export interface Ui {
  setModels(paths: string[], current: string): void;
}

export function buildUi(s: AppState, lights: Record<string, LightSettings>, hooks: UiHooks): Ui {
  const gui = new GUI({ title: 'Banc d\'essai voiture' });
  const holder = { model: '' };

  const fm = gui.addFolder('Modèle');
  fm.add({ reload: () => hooks.reload() }, 'reload').name('↻ Recharger');
  // Le sélecteur est le dernier élément du dossier : `options()` le recrée en fin de liste.
  let modelCtl = fm.add(holder, 'model', ['(aucun)']).name('Voiture');

  const fd = gui.addFolder('Conduite');
  fd.add(s, 'steering', -1, 1, 0.01).name('Braquage (volant + roues)');
  fd.add(s, 'speedKmh', -60, 200, 1).name('Vitesse (km/h)');
  fd.add({ go: () => void tween(1500, (k) => (s.speedKmh = 80 * k)) }, 'go').name('▶ Rouler à 80 km/h');
  fd.add({ stop: () => (s.speedKmh = 0) }, 'stop').name('■ Arrêt');
  fd.add({ sweep: () => void tween(3200, (k) => (s.steering = Math.sin(k * Math.PI * 2))) }, 'sweep').name('▶ Balayer le volant');

  const fp = gui.addFolder('Pédales et levier');
  fp.add(s, 'clutch', 0, 1, 0.01).name('Embrayage');
  fp.add(s, 'brake', 0, 1, 0.01).name('Frein');
  fp.add(s, 'throttle', 0, 1, 0.01).name('Accélérateur');
  fp.add(s, 'gear', GEARS).name('Rapport');
  fp.add(
    {
      cycle: async () => {
        for (const key of ['clutch', 'brake', 'throttle'] as const) {
          await tween(500, (k) => (s[key] = k));
          await tween(500, (k) => (s[key] = 1 - k));
        }
      },
    },
    'cycle',
  ).name('▶ Cycle des pédales');
  fp.add(
    {
      gears: async () => {
        for (const g of ['N', '1', '2', '3', '4', '5', '6', 'R', 'N'] as Gear[]) {
          s.gear = g;
          await tween(550, () => undefined);
        }
      },
    },
    'gears',
  ).name('▶ Parcourir les rapports');

  const fl = gui.addFolder('Phares, feux, essuie-glaces');
  fl.add(s, 'headlights').name('Phares');
  fl.add(s, 'highBeams').name('Feux de route');
  fl.add(s, 'braking').name('Feux stop (forcés)');
  fl.add(s, 'blinker', ['off', 'left', 'right', 'hazard']).name('Clignotants');
  fl.add(s, 'wipers', ['off', 'intermittent', 'slow', 'fast']).name('Essuie-glaces');

  const fc = gui.addFolder('Couleurs');
  fc.addColor(s, 'neonPrimary').name('Néon primaire');
  fc.addColor(s, 'neonSecondary').name('Néon secondaire');
  fc.add(s, 'neonIntensity', 0, 3, 0.01).name('Intensité néons');
  fc.add(s, 'glassOpacity', 0, 1, 0.01).name('Opacité vitres');

  const fh = gui.addFolder('Mode volant');
  fh.add(s, 'hoverOn').name('Décoller').listen().onChange((on: boolean) => {
    if (on && !hooks.hoverSupported()) {
      s.hoverOn = false;
      console.warn('Décollage impossible : les 4 roues (steer_FL/FR, spin_RL/RR) ne sont pas isolables dans ce modèle.');
    }
  });
  fh.add(s, 'hoverHeight', 0.1, 1.6, 0.01).name('Hauteur (m)');
  fh.add(s, 'hoverIntensity', 0, 3, 0.01).name('Intensité lueur');
  fh.add(s, 'hoverFollowNeon').name('Couleur = néons');
  fh.addColor(s, 'hoverColor').name('Couleur répulseur');

  const fv = gui.addFolder('Caméra');
  fv.add(s, 'fps').name('Vue conducteur (V)').listen().onChange((on: boolean) => hooks.setFps(on));
  fv.add(s, 'fov', 40, 120, 1).name('Champ de vision FPS');
  fv.add({ reset: () => hooks.resetView() }, 'reset').name('Recentrer');

  const fx = gui.addFolder('Lumières projetées');
  fx.add(s, 'showMarkers').name('Afficher les repères');
  fx.add({ copy: () => hooks.copyLights() }, 'copy').name('Copier la config (JSON)');
  for (const def of LIGHT_DEFS) {
    const cfg = lights[def.id];
    if (!cfg) continue;
    const f = fx.addFolder(def.id).close();
    f.add(cfg, 'enabled').name('Active');
    f.add(cfg, 'intensity', 0, def.spot ? 8000 : 100, def.spot ? 10 : 0.1).name('Intensité');
    f.add(cfg, 'distance', 0.5, 120, 0.5).name('Portée (m)');
    if (def.spot) {
      f.add(cfg, 'angle', 0.1, 1.2, 0.01).name('Angle');
      f.add(cfg, 'penumbra', 0, 1, 0.01).name('Pénombre');
    }
    f.addColor(cfg, 'color').name('Couleur');
    f.add(cfg, 'neonRole', ['none', 'primary', 'secondary']).name('Suit le néon');
    f.add(cfg.offset, 'x', -3, 3, 0.01).name('Décalage X');
    f.add(cfg.offset, 'y', -3, 3, 0.01).name('Décalage Y');
    f.add(cfg.offset, 'z', -3, 3, 0.01).name('Décalage Z');
    f.add({ g: () => hooks.gizmo(def.id) }, 'g').name('✥ Déplacer à la souris');
  }
  fx.add({ g: () => hooks.gizmo(null) }, 'g').name('Masquer le gizmo');

  const fr = gui.addFolder('Rendu').close();
  fr.add(s, 'bloom', 0, 2, 0.01).name('Bloom');
  fr.add(s, 'bloomThreshold', 0, 2, 0.01).name('Seuil du bloom');
  fr.add(s, 'exposure', 0.2, 3, 0.01).name('Exposition');

  // Les scénarios (tween) modifient l'état en douce : on rafraîchit l'affichage des curseurs.
  setInterval(() => gui.controllersRecursive().forEach((c) => c.updateDisplay()), 150);

  return {
    setModels(paths, current) {
      holder.model = current;
      modelCtl = modelCtl.options(paths.length ? paths : ['(aucun)']).onChange((p: string) => hooks.loadModel(p));
      modelCtl.updateDisplay();
    },
  };
}
