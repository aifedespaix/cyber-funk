import * as THREE from 'three';
import type { Stage } from './stage';

const FALLBACK_EYE = new THREE.Vector3(0.35, 0.3, 0.2);
const FLIP = new THREE.Quaternion().setFromAxisAngle(new THREE.Vector3(0, 1, 0), Math.PI);

/**
 * Vue du conducteur. Seule la POSITION de `cam_cockpit` compte : l'orientation est celle de la voiture, qui regarde +Z
 * (une caméra Three.js regarde -Z, d'où le demi-tour). Ça évite de dépendre de l'orientation du repère dans Blender.
 */
export class CockpitView {
  yaw = 0;
  pitch = 0;
  private dragging = false;

  constructor(
    private readonly stage: Stage,
    isActive: () => boolean,
  ) {
    stage.canvas.addEventListener('pointerdown', () => {
      if (isActive()) this.dragging = true;
    });
    window.addEventListener('pointerup', () => (this.dragging = false));
    window.addEventListener('pointermove', (e) => {
      if (!this.dragging || !isActive()) return;
      this.yaw -= e.movementX * 0.004;
      this.pitch = Math.max(-1.2, Math.min(1.2, this.pitch - e.movementY * 0.004));
    });
  }

  reset(): void {
    this.yaw = 0;
    this.pitch = 0;
  }

  /** Place la caméra à l'oeil du conducteur ; retourne vrai si `cam_cockpit` existe dans le modèle. */
  update(root: THREE.Object3D, chassis: THREE.Object3D, fov: number): boolean {
    const eye = root.getObjectByName('cam_cockpit');
    const cam = this.stage.camera;
    if (eye) eye.getWorldPosition(cam.position);
    else chassis.localToWorld(cam.position.copy(FALLBACK_EYE));
    const base = chassis.getWorldQuaternion(new THREE.Quaternion()).multiply(FLIP);
    const look = new THREE.Quaternion().setFromEuler(new THREE.Euler(this.pitch, this.yaw, 0, 'YXZ'));
    cam.quaternion.copy(base).multiply(look);
    cam.fov = fov;
    cam.near = 0.02;
    cam.updateProjectionMatrix();
    return eye !== undefined;
  }
}
