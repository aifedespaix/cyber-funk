import * as THREE from 'three';
import { FBXLoader } from 'three/examples/jsm/loaders/FBXLoader.js';
import { GLTFLoader } from 'three/examples/jsm/loaders/GLTFLoader.js';
import { MeshoptDecoder } from 'three/examples/jsm/libs/meshopt_decoder.module.js';

const gltfLoader = new GLTFLoader().setMeshoptDecoder(MeshoptDecoder);

export interface LoadedModel {
  root: THREE.Object3D;
  extensions: string[];
  triangles: number;
}

function countTriangles(root: THREE.Object3D): number {
  let triangles = 0;
  root.traverse((o) => {
    const mesh = o as THREE.Mesh;
    if (!mesh.isMesh) return;
    const g = mesh.geometry;
    triangles += (g.index ? g.index.count : (g.attributes.position?.count ?? 0)) / 3;
  });
  return Math.round(triangles);
}

/** FBX brut (ex. « Dirty car » avant son passage dans Blender). Les textures sont résolues côté serveur (vite.config.ts). */
async function loadFbx(path: string): Promise<LoadedModel> {
  const root = await new FBXLoader().loadAsync(`/glb/${path}?t=${Date.now()}`);
  // Un FBX est souvent en centimètres : on ramène à des mètres si le modèle est démesuré.
  const size = new THREE.Box3().setFromObject(root).getSize(new THREE.Vector3());
  if (Math.max(size.x, size.y, size.z) > 30) root.scale.multiplyScalar(0.01);
  root.updateMatrixWorld(true);
  // Pose les roues sur le sol (y = 0) pour que le sol, l'éclairage et le décollage aient un repère propre.
  const box = new THREE.Box3().setFromObject(root);
  root.position.y -= box.min.y;
  const wrapper = new THREE.Group();
  wrapper.add(root);
  return { root: wrapper, extensions: ['FBX (brut)'], triangles: countTriangles(root) };
}

/** Charge un modèle du dépôt (`path` relatif à la racine). GLB : gère `EXT_meshopt_compression` et `EXT_texture_webp`. */
export async function loadModel(path: string): Promise<LoadedModel> {
  if (/\.fbx$/i.test(path)) return loadFbx(path);
  const gltf = await gltfLoader.loadAsync(`/glb/${path}?t=${Date.now()}`);
  const used = (gltf.parser.json as { extensionsUsed?: string[] }).extensionsUsed ?? [];
  return { root: gltf.scene, extensions: used, triangles: countTriangles(gltf.scene) };
}

/** Libère géométries, matériaux et textures d'un modèle remplacé. */
export function disposeModel(root: THREE.Object3D): void {
  root.traverse((o) => {
    const mesh = o as THREE.Mesh;
    if (!mesh.isMesh) return;
    mesh.geometry.dispose();
    const mats = Array.isArray(mesh.material) ? mesh.material : [mesh.material];
    for (const m of mats) {
      for (const v of Object.values(m)) if (v instanceof THREE.Texture) v.dispose();
      m.dispose();
    }
  });
}
