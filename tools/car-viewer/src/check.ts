import * as THREE from 'three';
import { EXPECTED_MATERIALS, EXPECTED_NODES } from './config';
import type { LoadedModel } from './loader';

const fmt = (v: THREE.Vector3) => `${v.x.toFixed(2)}, ${v.y.toFixed(2)}, ${v.z.toFixed(2)}`;
const line = (ok: boolean, name: string, extra = '') =>
  `<div class="${ok ? 'ok' : 'ko'}">${ok ? '✓' : '✗'} ${name} <span class="dim">${extra}</span></div>`;

/** Remplit le panneau « contrat du modèle » : ce qui est là, ce qui manque, où sont les pivots. Retourne le nombre de manques. */
export function renderCheck(el: HTMLElement, model: LoadedModel, path: string): number {
  const root = model.root;
  root.updateMatrixWorld(true);
  const mats = new Set<string>();
  root.traverse((o) => {
    const mesh = o as THREE.Mesh;
    if (!mesh.isMesh) return;
    for (const m of Array.isArray(mesh.material) ? mesh.material : [mesh.material]) mats.add(m.name);
  });
  let missing = 0;
  let html = '';
  for (const [group, names] of Object.entries(EXPECTED_NODES)) {
    html += `<h4>${group}</h4>`;
    for (const n of names) {
      const o = root.getObjectByName(n);
      if (!o) missing++;
      html += line(!!o, n, o ? fmt(o.getWorldPosition(new THREE.Vector3())) : 'manquant');
    }
  }
  html += '<h4>Matériaux</h4>';
  for (const n of EXPECTED_MATERIALS) {
    if (!mats.has(n)) missing++;
    html += line(mats.has(n), n, mats.has(n) ? '' : 'manquant');
  }
  const box = new THREE.Box3().setFromObject(root);
  const size = box.getSize(new THREE.Vector3());
  html += `<h4>Modèle</h4><div class="dim">${path}<br>${model.triangles.toLocaleString('fr-FR')} triangles · ${model.extensions.join(', ') || 'aucune extension'}<br>`;
  html += `dimensions L×H×P : ${size.x.toFixed(2)} × ${size.y.toFixed(2)} × ${size.z.toFixed(2)} m<br>`;
  html += `sol (y min) : ${box.min.y.toFixed(2)} m</div>`;
  const status = missing === 0 ? '<span class="ok">complet</span>' : `<span class="ko">${missing} manquant(s)</span>`;
  el.innerHTML = `<summary>Contrat du modèle — ${status}</summary>${html}`;
  return missing;
}
