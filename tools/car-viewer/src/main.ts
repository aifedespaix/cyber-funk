import * as THREE from 'three';
import { TransformControls } from 'three/examples/jsm/controls/TransformControls.js';
import { renderCheck } from './check';
import { Hover } from './hover';
import { defaultLightSettings, LightRig } from './lights';
import { disposeModel, loadModel } from './loader';
import { CarRig } from './rig';
import { Stage } from './stage';
import { defaultState } from './state';
import { buildUi } from './ui';
import { CockpitView } from './view';

interface Car {
  path: string;
  root: THREE.Object3D;
  hover: Hover;
  rig: CarRig;
  lights: LightRig;
}

const $ = <T extends HTMLElement>(id: string) => document.getElementById(id) as T;
const statusEl = $('status');
const checkEl = $('check');

const state = defaultState();
const lightSettings = defaultLightSettings();
const stage = new Stage($<HTMLCanvasElement>('view'));
const view = new CockpitView(stage, () => state.fps);
let car: Car | null = null;
let currentPath = '';
let savedOrbitPos = stage.camera.position.clone();
let note = '';

// Gizmo de déplacement des lumières.
const gizmo = new TransformControls(stage.camera, stage.canvas);
gizmo.setMode('translate');
gizmo.setSize(0.7);
stage.scene.add(gizmo.getHelper());
let gizmoId: string | null = null;
let dragging = false;
gizmo.addEventListener('dragging-changed', (e) => (dragging = Boolean(e.value)));
gizmo.addEventListener('objectChange', () => {
  if (gizmoId && car) car.lights.syncOffsetFromLight(gizmoId);
});

function attachGizmo(id: string | null): void {
  gizmoId = id;
  const entry = id && car ? car.lights.entries.get(id) : undefined;
  if (entry) {
    state.showMarkers = true;
    gizmo.attach(entry.light);
  } else {
    gizmo.detach();
  }
}

function setFps(on: boolean): void {
  state.fps = on;
  if (on) {
    savedOrbitPos = stage.camera.position.clone();
    attachGizmo(null);
  } else {
    stage.camera.position.copy(savedOrbitPos);
    stage.camera.fov = 50;
    stage.camera.near = 0.02;
    stage.camera.updateProjectionMatrix();
  }
}

const ui = buildUi(state, lightSettings, {
  loadModel: (p) => void load(p),
  reload: () => void load(currentPath),
  setFps,
  resetView: () => {
    view.reset();
    stage.camera.position.set(5, 2.2, 6);
    stage.orbit.target.set(0, 0.3, 0.3);
  },
  copyLights: () => {
    const json = JSON.stringify(car?.lights.toJSON() ?? {}, null, 2);
    console.log(json);
    navigator.clipboard.writeText(json).then(
      () => (note = 'config des lumières copiée'),
      () => (note = 'copie impossible (voir la console)'),
    );
  },
  gizmo: attachGizmo,
  hoverSupported: () => car?.hover.supported ?? false,
});

async function listModels(): Promise<string[]> {
  const res = await fetch('/api/models');
  const items = (await res.json()) as { path: string }[];
  return items.map((i) => i.path).filter((p) => !p.includes('/2k/'));
}

/** Charge (ou recharge) un modèle en gardant l'état de l'interface, la caméra et les réglages de lumières. */
async function load(path: string, attempt = 0): Promise<void> {
  if (!path) return;
  statusEl.textContent = `chargement de ${path}…`;
  try {
    const model = await loadModel(path);
    const previous = car;
    attachGizmo(null);
    if (previous) {
      previous.lights.dispose();
      previous.hover.dispose();
      stage.scene.remove(previous.root);
      disposeModel(previous.root);
    }
    const groundY = new THREE.Box3().setFromObject(model.root).min.y;
    const hover = new Hover(model.root, state, groundY);
    hover.progress = previous?.hover.progress ?? 0;
    stage.scene.add(model.root);
    const rig = new CarRig(model.root, state);
    const lights = new LightRig(hover.chassis, model.root, lightSettings, state);
    stage.setGround(groundY);
    car = { path, root: model.root, hover, rig, lights };
    currentPath = path;
    const missing = renderCheck(checkEl, model, path);
    const anchors = lights.missingAnchors();
    note = `${new Date().toLocaleTimeString('fr-FR')} · ${missing} manquant(s)${anchors.length ? ' · lumières sans repère' : ''}`;
  } catch (err) {
    console.error(err);
    // Blender peut encore être en train d'écrire : on réessaie un peu plus tard.
    if (attempt < 3) setTimeout(() => void load(path, attempt + 1), 1200);
    else note = `échec du chargement de ${path}`;
  }
}

const clock = new THREE.Clock();
const primary = new THREE.Color();
const secondary = new THREE.Color();

function frame(): void {
  const dt = Math.min(clock.getDelta(), 0.1);
  const t = clock.elapsedTime;
  primary.set(state.neonPrimary);
  secondary.set(state.neonSecondary);
  stage.setLook(state.bloom, state.bloomThreshold, state.exposure);
  if (car) {
    car.rig.update(dt, t);
    car.hover.update(dt, t, primary, secondary);
    car.lights.update(primary, secondary, car.hover.glow);
    if (state.fps) view.update(car.root, car.hover.chassis, state.fov);
  }
  stage.orbit.enabled = !state.fps && !dragging;
  if (!state.fps) stage.orbit.update();
  stage.render();
  const phase = car ? car.hover.phase : '-';
  statusEl.textContent = `${currentPath || 'aucun modèle'} · volant ${phase} · ${note}`;
  requestAnimationFrame(frame);
}

window.addEventListener('keydown', (e) => {
  if (e.key.toLowerCase() === 'v' && !(e.target instanceof HTMLInputElement)) setFps(!state.fps);
});

// Rechargement à chaud : un export Blender ou build-glb.sh change le GLB sur disque.
import.meta.hot?.on('glb-changed', (data: { path: string }) => {
  void listModels().then((paths) => ui.setModels(paths, currentPath));
  if (data.path === currentPath) void load(currentPath);
});

void (async () => {
  const paths = await listModels();
  // Par défaut : l'export brut de Blender, sinon le GLB de production.
  const wanted = new URLSearchParams(location.search).get('model');
  const first = (wanted && paths.includes(wanted) ? wanted : null) ?? paths.find((p) => p.endsWith('car_raw.glb')) ?? paths[0] ?? '';
  ui.setModels(paths, first);
  await load(first);
  requestAnimationFrame(frame);
})();
