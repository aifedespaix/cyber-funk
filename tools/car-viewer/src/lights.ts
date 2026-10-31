import * as THREE from 'three';
import type { AppState } from './state';

export type NeonRole = 'none' | 'primary' | 'secondary';

/** Réglages d'une lumière. Ils vivent hors du modèle pour survivre aux rechargements. */
export interface LightSettings {
  enabled: boolean;
  intensity: number;
  distance: number;
  angle: number;
  penumbra: number;
  color: string;
  neonRole: NeonRole;
  /** Décalage par rapport au repère `anchor_*` du modèle (repère du châssis, en mètres). */
  offset: THREE.Vector3;
}

interface LightDef {
  id: string;
  anchor: string;
  spot: boolean;
  settings: Omit<LightSettings, 'offset' | 'enabled'>;
}

const spot = { intensity: 1500, distance: 55, angle: 0.5, penumbra: 0.5, color: '#fff4e0', neonRole: 'none' as NeonRole };
const point = (intensity: number, distance: number, neonRole: NeonRole) => ({
  intensity, distance, angle: 0, penumbra: 0, color: '#ffffff', neonRole,
});

export const LIGHT_DEFS: LightDef[] = [
  { id: 'headlight_L', anchor: 'anchor_headlight_L', spot: true, settings: spot },
  { id: 'headlight_R', anchor: 'anchor_headlight_R', spot: true, settings: spot },
  { id: 'underglow', anchor: 'anchor_underglow', spot: false, settings: point(14, 7, 'primary') },
  { id: 'cabin', anchor: 'anchor_cabin_light', spot: false, settings: point(5, 3, 'secondary') },
  { id: 'dash', anchor: 'anchor_dash_light', spot: false, settings: point(4, 2.5, 'primary') },
  { id: 'front_logo', anchor: 'anchor_front_logo_light', spot: false, settings: point(3, 2, 'primary') },
  { id: 'rear_logo', anchor: 'anchor_rear_logo_light', spot: false, settings: point(3, 2, 'secondary') },
];

export function defaultLightSettings(): Record<string, LightSettings> {
  return Object.fromEntries(
    LIGHT_DEFS.map((d) => [d.id, { ...d.settings, enabled: true, offset: new THREE.Vector3() }]),
  );
}

interface Entry {
  def: LightDef;
  light: THREE.SpotLight | THREE.PointLight;
  target?: THREE.Object3D;
  marker: THREE.Mesh;
  anchor: THREE.Vector3 | null;
}

/** Lumières projetées posées sur les repères `anchor_*` du modèle (les zones émissives, elles, sont dans le GLB). */
export class LightRig {
  readonly entries = new Map<string, Entry>();

  constructor(
    chassis: THREE.Object3D,
    root: THREE.Object3D,
    private readonly settings: Record<string, LightSettings>,
    private readonly s: AppState,
  ) {
    root.updateMatrixWorld(true);
    for (const def of LIGHT_DEFS) {
      const a = root.getObjectByName(def.anchor);
      const anchor = a ? chassis.worldToLocal(a.getWorldPosition(new THREE.Vector3())) : null;
      const light = def.spot ? new THREE.SpotLight(0xffffff, 0, 0, 0.5, 0.5, 2) : new THREE.PointLight(0xffffff, 0, 0, 2);
      light.name = `light_${def.id}`;
      const marker = new THREE.Mesh(new THREE.SphereGeometry(0.05), new THREE.MeshBasicMaterial({ color: 0xffff00, depthTest: false }));
      marker.renderOrder = 10;
      light.add(marker);
      chassis.add(light);
      let target: THREE.Object3D | undefined;
      if (light instanceof THREE.SpotLight) {
        target = new THREE.Object3D();
        chassis.add(target);
        light.target = target;
      }
      this.entries.set(def.id, { def, light, target, marker, anchor });
    }
  }

  /** Les lumières dont le repère manque dans le modèle (signalées dans le panneau de contrôle). */
  missingAnchors(): string[] {
    return [...this.entries.values()].filter((e) => !e.anchor).map((e) => e.def.anchor);
  }

  update(primary: THREE.Color, secondary: THREE.Color, hoverGlow: number): void {
    const s = this.s;
    for (const [id, e] of this.entries) {
      const cfg = this.settings[id];
      if (!cfg) continue;
      const base = e.anchor ?? new THREE.Vector3(0, 0, 0);
      e.light.position.copy(base).add(cfg.offset);
      e.light.distance = cfg.distance;
      const color =
        cfg.neonRole === 'primary' ? primary : cfg.neonRole === 'secondary' ? secondary : new THREE.Color(cfg.color);
      e.light.color.copy(color);
      let on = cfg.enabled && e.anchor !== null;
      let intensity = cfg.intensity;
      if (e.light instanceof THREE.SpotLight && e.target) {
        on = on && s.headlights;
        const high = s.highBeams;
        e.light.angle = high ? cfg.angle * 0.6 : cfg.angle;
        e.light.penumbra = cfg.penumbra;
        intensity *= high ? 2.5 : 1;
        e.target.position.copy(e.light.position).add(new THREE.Vector3(0, -0.8, 20));
      } else {
        intensity *= s.neonIntensity;
        if (id === 'underglow') intensity *= 1 + 3 * hoverGlow;
      }
      e.light.intensity = on ? intensity : 0;
      e.marker.visible = s.showMarkers;
      (e.marker.material as THREE.MeshBasicMaterial).color.copy(color);
    }
  }

  /** Configuration à reporter dans le jeu : position absolue (repère du châssis) et décalage par rapport au repère. */
  toJSON(): Record<string, unknown> {
    const round = (v: number) => Math.round(v * 1000) / 1000;
    const out: Record<string, unknown> = {};
    for (const [id, e] of this.entries) {
      const cfg = this.settings[id];
      if (!cfg) continue;
      out[id] = {
        anchor: e.def.anchor,
        position: e.light.position.toArray().map(round),
        offset: cfg.offset.toArray().map(round),
        intensity: cfg.intensity,
        distance: cfg.distance,
        ...(e.def.spot ? { angle: cfg.angle, penumbra: cfg.penumbra } : {}),
        color: cfg.neonRole === 'none' ? cfg.color : `neon_${cfg.neonRole}`,
        enabled: cfg.enabled,
      };
    }
    return out;
  }

  /** Après un déplacement au gizmo, mémorise le décalage par rapport au repère. */
  syncOffsetFromLight(id: string): void {
    const e = this.entries.get(id);
    const cfg = this.settings[id];
    if (!e || !cfg) return;
    cfg.offset.copy(e.light.position).sub(e.anchor ?? new THREE.Vector3());
  }

  dispose(): void {
    for (const e of this.entries.values()) {
      e.light.dispose();
      e.marker.geometry.dispose();
      (e.marker.material as THREE.Material).dispose();
      e.light.removeFromParent();
      e.target?.removeFromParent();
    }
    this.entries.clear();
  }
}
