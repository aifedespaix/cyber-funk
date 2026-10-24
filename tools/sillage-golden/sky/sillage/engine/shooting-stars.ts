import { AdditiveBlending, Color, Group, Matrix4, Mesh, PlaneGeometry, ShaderMaterial, Vector3 } from "three";
import type { ScenePalette } from "./palette";

const POOL = 5;
const RADIUS = 760;

interface Streak {
  mesh: Mesh;
  material: ShaderMaterial;
  position: Vector3;
  direction: Vector3;
  speed: number;
  life: number;
  age: number;
}

const basis = new Matrix4();
const normal = new Vector3();
const side = new Vector3();
const UP = new Vector3(0, 1, 0);

/**
 * Étoiles filantes : un trait à tête brillante qui glisse sur la voûte. Une
 * toutes les quelques secondes, plus une de temps en temps sur un kick.
 */
export class ShootingStars {
  readonly group = new Group();
  private readonly streaks: Streak[] = [];
  private readonly geometry = new PlaneGeometry(1, 1);
  private readonly tints: Color[];
  private countdown = 1.5;
  private spawned = 0;

  constructor(palette: ScenePalette) {
    this.tints = [palette.ice, palette.mint, palette.azure, palette.rose];
    for (let i = 0; i < POOL; i++) {
      const material = new ShaderMaterial({
        transparent: true,
        depthWrite: false,
        blending: AdditiveBlending,
        uniforms: { uColor: { value: new Color() }, uAlpha: { value: 0 } },
        vertexShader: /* glsl */ `
          varying vec2 vUv;
          void main() {
            vUv = uv;
            gl_Position = projectionMatrix * modelViewMatrix * vec4(position, 1.0);
          }
        `,
        fragmentShader: /* glsl */ `
          uniform vec3 uColor;
          uniform float uAlpha;
          varying vec2 vUv;
          void main() {
            // x = 1 en tête, 0 au bout de la queue ; y centré sur l'axe du trait.
            float tail = pow(vUv.x, 3.0);
            float width = smoothstep(0.5, 0.0, abs(vUv.y - 0.5)) * mix(0.4, 1.0, vUv.x);
            float head = smoothstep(0.93, 1.0, vUv.x) * 1.6;
            float a = (tail + head) * width * uAlpha;
            gl_FragColor = vec4(uColor * a * 2.2, a);
          }
        `,
      });
      const mesh = new Mesh(this.geometry, material);
      mesh.visible = false;
      mesh.frustumCulled = false;
      this.group.add(mesh);
      this.streaks.push({ mesh, material, position: new Vector3(), direction: new Vector3(), speed: 0, life: 1, age: 1 });
    }
  }

  /** Déclenche une étoile filante tout de suite (ignoré si le pool est plein). */
  launch(): void {
    const streak = this.streaks.find((s) => s.age >= s.life);
    if (!streak) return;
    const k = this.spawned++;
    // Point de départ dans l'hémisphère avant, assez haut pour ne pas finir derrière les tours.
    const azimuth = (rand(k, 1) - 0.5) * Math.PI * 1.5;
    const elevation = 0.28 + rand(k, 2) * 0.85;
    streak.position.set(Math.sin(azimuth) * Math.cos(elevation), Math.sin(elevation), -Math.cos(azimuth) * Math.cos(elevation)).multiplyScalar(RADIUS);
    // Direction : tangente à la sphère, descendante, plutôt latérale.
    const heading = (rand(k, 3) < 0.5 ? -1 : 1) * (0.45 + rand(k, 4) * 0.5);
    normal.copy(streak.position).normalize();
    side.crossVectors(UP, normal).normalize();
    streak.direction.copy(side).multiplyScalar(heading).addScaledVector(UP, -0.35 - rand(k, 5) * 0.4);
    streak.direction.addScaledVector(normal, -streak.direction.dot(normal)).normalize();
    streak.speed = 420 + rand(k, 6) * 340;
    streak.life = 0.7 + rand(k, 7) * 0.6;
    streak.age = 0;
    streak.material.uniforms.uColor!.value.copy(this.tints[k % this.tints.length]!);
    streak.mesh.scale.set(90 + rand(k, 8) * 90, 1.6 + rand(k, 9) * 1.2, 1);
    streak.mesh.visible = true;
  }

  update(dt: number, calm: boolean): void {
    this.countdown -= dt;
    if (this.countdown <= 0) {
      this.launch();
      this.countdown = (calm ? 9 : 3) + rand(this.spawned, 10) * (calm ? 8 : 5);
    }

    for (const streak of this.streaks) {
      if (streak.age >= streak.life) {
        streak.mesh.visible = false;
        continue;
      }
      streak.age += dt;
      streak.position.addScaledVector(streak.direction, streak.speed * dt);
      const t = streak.age / streak.life;
      streak.material.uniforms.uAlpha!.value = Math.sin(Math.min(1, t) * Math.PI) ** 0.7;

      // Le plan est tourné vers le centre ; son axe X suit la direction du mouvement.
      normal.copy(streak.position).normalize().negate();
      side.crossVectors(normal, streak.direction).normalize();
      basis.makeBasis(streak.direction, side, normal);
      streak.mesh.quaternion.setFromRotationMatrix(basis);
      streak.mesh.position.copy(streak.position).addScaledVector(streak.direction, -streak.mesh.scale.x * 0.5);
    }
  }

  dispose(): void {
    this.geometry.dispose();
    for (const streak of this.streaks) streak.material.dispose();
  }
}

function rand(seed: number, salt: number): number {
  const x = Math.sin(seed * 91.7 + salt * 37.3 + 3.1) * 43758.5453;
  return x - Math.floor(x);
}
