//! L'arche de data glitchée (`voxel-arch.ts`) : un portique monumental qui enjambe la route, fait de 1 500 voxels flottants.
//!
//! Un seul maillage fusionné dont toute l'animation est dans le vertex shader (`landmark_arch.wesl`) : rien à mettre à jour
//! côté CPU image par image que quelques scalaires. Chaque voxel connaît sa place sur l'arche et un cycle personnel : il
//! s'assemble, tient, puis se désassemble en tournant autour de l'arche en spirale. Au passage de la voiture, les voxels
//! proches se dispersent ; un drop fait éclater l'ensemble, il se recompose en 2,4 s.
//!
//! Écarts avec Sillage (règle 7 de `CLAUDE.md`, voir la PR) : l'éclatement est le niveau d'un limiteur de flashs ; les voxels
//! qui se défont ne clignotent en rouge que si les flashs sont permis, trois fois par seconde au plus (douze dans Sillage) ;
//! les horloges du cycle et de la rotation sont intégrées ou bornées côté CPU (les produits `time × …` de Sillage téléportaient
//! les voxels à chaque variation de l'intensité).

use bevy::math::{Vec3, Vec4};
use bevy::transform::components::Transform;
use cf_render::SceneColor;

use super::flash::Burst;
use super::frame::LandmarkFrame;
use super::landmark::{Landmark, LandmarkKind, Placement, StepCtx};
use super::math::hash;
use super::part::{Look, MeshData, MeshSpec, PartPaint, PartSpec, RoadBand};

/// Nombre de voxels.
pub const VOXELS: usize = 1500;
const HALF_WIDTH: f32 = 17.5;
/// Hauteur du départ de la voûte et flèche de celle-ci : le sommet est à 34 m.
const SPRING: f32 = 19.0;
const RISE: f32 = 15.0;
const BAND: f32 = 3.4;
const DEPTH: f32 = 4.4;
/// Part de voxels qui ne s'assemblent jamais : des fragments de données qui orbitent autour de l'arche.
const ORBITERS: f32 = 0.08;
/// Un drop fait éclater l'arche ; elle se recompose en 2,4 s.
const BREAK_DECAY_S: f32 = 2.4;
/// L'horloge de rotation repart de zéro toutes les 60 s.
const SPIN_WRAP_S: f64 = 60.0;

/// Un point de la ligne médiane de l'arche, avec sa normale.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PathPoint {
    /// Position dans le plan de l'arche.
    pub x: f32,
    /// Altitude.
    pub y: f32,
    /// Normale (x).
    pub nx: f32,
    /// Normale (y).
    pub ny: f32,
}

/// Ligne médiane de l'arche : montant droit, voûte en carré arrondi, montant droit. Échantillonnée à pas régulier.
#[must_use]
pub fn arch_path() -> Vec<PathPoint> {
    let mut points = Vec::new();
    let exponent = 2.0 / 4.0;
    let mut y = 0.0;
    while y < SPRING {
        points.push(PathPoint {
            x: HALF_WIDTH,
            y,
            nx: 1.0,
            ny: 0.0,
        });
        y += 0.5;
    }
    for i in 0..=80 {
        let t = i as f32 / 80.0 * std::f32::consts::PI;
        let c = t.cos();
        let sign = if c == 0.0 { 0.0 } else { c.signum() };
        let x = HALF_WIDTH * sign * c.abs().powf(exponent);
        let y = SPRING + RISE * t.sin().abs().powf(exponent);
        let (nx, ny) = (x / HALF_WIDTH, (y - SPRING) / RISE);
        let n = nx.hypot(ny);
        let n = if n == 0.0 { 1.0 } else { n };
        points.push(PathPoint {
            x,
            y,
            nx: nx / n,
            ny: ny / n,
        });
    }
    let mut y = SPRING;
    while y >= 0.0 {
        points.push(PathPoint {
            x: -HALF_WIDTH,
            y,
            nx: -1.0,
            ny: 0.0,
        });
        y -= 0.5;
    }
    points
}

/// L'arche de voxels.
#[derive(Debug)]
pub struct VoxelArch {
    burst: Burst,
    /// Éclatement rendu par le limiteur, 0..1.
    burst_level: f32,
    car_z: f32,
    /// Horloge du cycle de montage, intégrée : `0,09 × (1 + intensité / 2)` tours par seconde.
    cycle: f64,
    permitted: bool,
    mood: Vec3,
}

impl Default for VoxelArch {
    fn default() -> Self {
        Self::new()
    }
}

impl VoxelArch {
    /// L'arche.
    #[must_use]
    pub fn new() -> Self {
        Self {
            burst: Burst::default(),
            burst_level: 0.0,
            car_z: 0.0,
            cycle: 0.0,
            permitted: false,
            mood: Vec3::ZERO,
        }
    }

    /// L'éclatement courant, 0..1.
    #[must_use]
    pub fn burst_level(&self) -> f32 {
        self.burst_level
    }

    /// Le maillage fusionné : un cube par voxel. `normal`/`uv` de chaque cube ; `tangent` = (place sur l'arche, teinte) ;
    /// `color` = (phase, agitation, sens de rotation, taille), phase négative pour un orbiteur.
    #[must_use]
    pub fn voxel_mesh() -> MeshData {
        let path = arch_path();
        let mut mesh = MeshData::default();
        for i in 0..VOXELS {
            let n = i as u32 + 1;
            let r = |k: u32| hash(n, k);
            let orbiter = r(9) < ORBITERS;
            let (target, tone) = if orbiter {
                let a = r(10) * std::f32::consts::TAU;
                let radius = 8.0 + r(11) * 10.0;
                (
                    Vec3::new(a.cos() * radius, SPRING + a.sin() * radius * 0.8, (r(12) - 0.5) * 30.0),
                    r(13),
                )
            } else {
                let index = ((r(1) * (path.len() - 1) as f32).floor() as usize).min(path.len() - 1);
                let point = path[index];
                // Entre deux bords de la bande, un peu plus de voxels sur les bords : la silhouette reste lisible.
                let edge = if r(2) < 0.35 {
                    if r(3) < 0.5 { -0.5 } else { 0.5 }
                } else {
                    r(4) - 0.5
                };
                (
                    Vec3::new(
                        point.x + point.nx * edge * BAND,
                        point.y + point.ny * edge * BAND,
                        (r(5) - 0.5) * DEPTH,
                    ),
                    index as f32 / (path.len() - 1) as f32,
                )
            };
            // x : phase du cycle ; y : agitation ; z : sens de rotation ; w : taille. Un orbiteur reste dispersé.
            let phase = if orbiter { -1.0 } else { r(6) };
            let rnd = [phase, r(7), r(8), r(14)];
            let first = mesh.push_cube(Vec3::ZERO, 1.0) as usize;
            let count = mesh.positions.len() - first;
            mesh.colors.extend(std::iter::repeat_n(rnd, count));
            mesh.tangents
                .extend(std::iter::repeat_n([target.x, target.y, target.z, tone], count));
        }
        mesh
    }
}

impl Landmark for VoxelArch {
    fn kind(&self) -> LandmarkKind {
        LandmarkKind::Arch
    }

    fn span(&self) -> f32 {
        34.0
    }

    fn spans_road(&self) -> bool {
        true
    }

    /// Au centre de la route, la voûte passe de 32 à 36 m ; les montants sont hors chaussée.
    fn road_band(&self) -> Option<RoadBand> {
        Some(RoadBand {
            lo: SPRING + RISE - BAND / 2.0 - 1.7,
            hi: SPRING + RISE + BAND / 2.0 + 1.2,
            half_depth: DEPTH / 2.0 + 3.0,
        })
    }

    fn parts(&self) -> Vec<PartSpec> {
        // Le vertex shader déplace chaque voxel : jusqu'à `1 + 0,5 + 1,4` fois son écart au cœur de l'arche (≈ 20 m) en
        // tourbillon, ±15 m en profondeur de plus. L'enveloppe est celle de ce volume, pas celle des positions de repos.
        vec![
            PartSpec::new(
                "voxels",
                MeshSpec::Custom(Box::new(Self::voxel_mesh())),
                Look::ArchVoxels,
            )
            .with_bounds(Vec3::new(-75.0, -50.0, -40.0), Vec3::new(75.0, 95.0, 40.0)),
        ]
    }

    fn show(&mut self, _place: &Placement) {
        self.burst.reset();
        self.burst_level = 0.0;
    }

    fn hide(&mut self) {
        self.burst.reset();
        self.burst_level = 0.0;
    }

    fn step(&mut self, frame: &LandmarkFrame, ctx: &mut StepCtx<'_>) {
        self.permitted = frame.policy.permitted();
        self.burst.step(frame.dt, self.permitted);
        if frame.drive.drop {
            self.burst.trigger(BREAK_DECAY_S, frame.time);
        }
        self.burst_level = self.burst.level();
        self.car_z = (frame.car.z - ctx.place.z) as f32;
        self.cycle = (self.cycle + f64::from(0.09 * (1.0 + frame.drive.intensity * 0.5) * frame.dt)).fract();
        // Un pied de chaque côté : la lumière change avec l'ambiance du morceau.
        let power = 0.9 + frame.beat.pulse * 1.4 + frame.drive.intensity;
        let colors = &frame.colors;
        for (i, (x, color)) in [(-HALF_WIDTH, SceneColor::Mint), (HALF_WIDTH, SceneColor::Rose)]
            .into_iter()
            .enumerate()
        {
            ctx.light(i, Vec3::new(x, 7.0, 0.0), 85.0, colors.get(color), power);
        }
        self.mood = frame.drive.mood;
    }

    fn paint(&self, frame: &LandmarkFrame, out: &mut [PartPaint]) {
        if let [voxels] = out {
            *voxels = PartPaint {
                transform: Transform::IDENTITY,
                mood: self.mood,
                a: Vec4::new(
                    self.burst_level,
                    if self.permitted { 1.0 } else { 0.0 },
                    self.car_z,
                    0.0,
                ),
                c: Vec4::new(self.cycle as f32, (frame.time % SPIN_WRAP_S) as f32, 0.0, 0.0),
                ..PartPaint::default()
            };
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn la_ligne_mediane_monte_passe_la_voute_puis_redescend() {
        let path = arch_path();
        assert_eq!(path.first().map(|p| (p.x, p.y)), Some((HALF_WIDTH, 0.0)));
        let top = path.iter().map(|p| p.y).fold(0.0, f32::max);
        assert!((top - (SPRING + RISE)).abs() < 1e-3, "sommet à 34 m ({top})");
        let last = path.last().copied().unwrap_or(PathPoint {
            x: 0.0,
            y: 1.0,
            nx: 0.0,
            ny: 0.0,
        });
        assert!((last.x + HALF_WIDTH).abs() < 1e-5 && last.y.abs() < 1e-5);
        assert!(path.iter().all(|p| (p.nx.hypot(p.ny) - 1.0).abs() < 1e-4));
    }

    #[test]
    fn mille_cinq_cents_voxels_dont_huit_pour_cent_d_orbiteurs() {
        let mesh = VoxelArch::voxel_mesh();
        assert_eq!(mesh.vertex_count(), VOXELS * 24);
        assert_eq!(mesh.colors.len(), mesh.vertex_count());
        assert_eq!(mesh.tangents.len(), mesh.vertex_count());
        assert_eq!(mesh.normals.len(), mesh.vertex_count());
        let orbiters = mesh.colors.iter().step_by(24).filter(|c| c[0] < 0.0).count();
        let share = orbiters as f32 / VOXELS as f32;
        assert!((share - ORBITERS).abs() < 0.03, "{share}");
        // Les voxels de l'arche sont dans la bande de l'arche, les orbiteurs ailleurs.
        for (c, t) in mesh.colors.iter().zip(&mesh.tangents).step_by(24) {
            if c[0] >= 0.0 {
                assert!(
                    t[0].abs() <= HALF_WIDTH + BAND
                        && t[1] >= -BAND
                        && t[1] <= SPRING + RISE + BAND
                        && t[2].abs() <= DEPTH / 2.0
                );
            }
            assert!((0.0..=1.0).contains(&t[3]));
        }
    }

    #[test]
    fn le_maillage_est_deterministe() {
        assert_eq!(VoxelArch::voxel_mesh(), VoxelArch::voxel_mesh());
    }

    #[test]
    fn l_arche_enjambe_la_route_et_publie_sa_bande_d_esquive() {
        let arch = VoxelArch::new();
        assert!(arch.spans_road());
        assert!(arch.body().is_none());
        let band = arch.road_band().expect("bande");
        assert!(band.lo > 28.0 && band.hi < 40.0 && band.lo < band.hi);
    }

    #[test]
    fn l_enveloppe_couvre_le_tourbillon_des_voxels() {
        let parts = VoxelArch::new().parts();
        let Some((lo, hi)) = parts[0].bounds else {
            panic!("enveloppe imposée");
        };
        let mesh = VoxelArch::voxel_mesh();
        let rest = mesh.bounds().unwrap_or_default();
        assert!(lo.x < rest.0.x && hi.x > rest.1.x);
    }
}
