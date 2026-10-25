//! La scène des immeubles-vitrines : la bibliothèque (`LandmarkLibrary` de `registry.ts`) et ce qu'il faut tenir d'une image
//! à l'autre. Pure : aucune entité, aucun asset ; la couche Bevy ([`super::render`]) en lit la pose des pièces.
//!
//! Sillage créait chaque immeuble à sa première apparition (shaders, textures, instances) puis le gardait : « un seul est
//! visible à la fois, les autres dorment ». Ici tout est créé **à l'ouverture** (règles 4 et 8 de `CLAUDE.md`) : la scène
//! construit les neuf logiques au démarrage, la couche de rendu en tire ses entités préallouées, et rien n'est créé ni
//! compilé pendant le show.
//!
//! La scène porte aussi ce que le directeur du trafic tenait de l'immeuble : l'historique des derniers tirages et leur
//! fraîcheur (`chooseLandmark`), la variation du spectacle (`landmarkVariation`), l'ordre de mise au point (`force`).

use bevy::math::Vec3;

use super::arcade::ArcadeBuilding;
use super::arch::VoxelArch;
use super::board::BoardTower;
use super::error::ErrorBuilding;
use super::field::{FieldState, MAX_LIGHTS};
use super::flash::PulseGate;
use super::frame::{Colors, LandmarkFrame};
use super::landmark::{KIND_COUNT, Landmark, LandmarkKind, Placement, StepCtx};
use super::lava::LavaBuilding;
use super::matrix::{MatrixBuilding, MatrixVariant};
use super::mecha::MechaBuilding;
use super::part::{PartPaint, PartSpec};
use crate::pilot::rand::Rand;

/// Combien de derniers tirages on se souvient (`landmarkHistory`) ; les deux derniers sont exclus du tirage suivant.
const HISTORY: usize = 4;
/// Nombre de derniers tirages exclus.
const EXCLUDED: usize = 2;

/// L'immeuble visible en ce moment.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Active {
    /// Lequel.
    pub kind: LandmarkKind,
    /// Où il est posé.
    pub place: Placement,
}

/// Ce que la scène lit en dehors du trafic : le temps musical continu et la palette, écrits chaque image par un système
/// Bevy (`beat.beat` d'`AudioFeatures` n'est pas dans `TrafficFrame`).
#[derive(Debug, Clone, Copy)]
pub struct StageInputs {
    /// Temps musical continu (`beat.beat`).
    pub beat: f32,
    /// La palette vivante.
    pub colors: Colors,
    /// Teinte ambiante de la section (`SceneMood`), lumière linéaire.
    pub mood: Vec3,
}

impl Default for StageInputs {
    fn default() -> Self {
        Self {
            beat: 0.0,
            colors: Colors::default(),
            mood: Vec3::new(0.4, 0.2, 0.9),
        }
    }
}

/// Ce que les shaders lisent de l'image, commun à toutes les pièces (`FrameUniforms` de `overlay.ts`).
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct FrameUniform {
    /// Horloge de la scène, s.
    pub time: f64,
    /// Pouls borné par le limiteur.
    pub pulse: f32,
    /// Basses.
    pub bass: f32,
    /// Médiums.
    pub mid: f32,
    /// Aigus.
    pub high: f32,
    /// Temps musical continu.
    pub beat: f32,
    /// Intensité du morceau.
    pub intensity: f32,
    /// Phase recalée sur le tempo.
    pub phase: f32,
}

/// Mise au point : un immeuble précis à une distance précise (`force` de `director.ts`).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Forced {
    /// Lequel.
    pub kind: LandmarkKind,
    /// Distance exacte du centre devant la voiture (m) et côté ; sinon le défaut.
    pub at: Option<(f32, i8)>,
}

/// La scène des immeubles-vitrines.
pub struct LandmarkStage {
    landmarks: Vec<Box<dyn Landmark>>,
    parts: Vec<Vec<PartSpec>>,
    paint: Vec<Vec<PartPaint>>,
    active: Option<Active>,
    seq: u32,
    /// Ce que les immeubles écrivent dans le champ de la ville.
    pub field: FieldState,
    light_levels: [f32; MAX_LIGHTS],
    pulse: PulseGate,
    /// Ce que la scène lit en dehors du trafic.
    pub inputs: StageInputs,
    uniform: FrameUniform,
    history: Vec<LandmarkKind>,
    fresh: [f64; KIND_COUNT],
    marks: Vec<(LandmarkKind, f64)>,
    last_side: i8,
    last_tint: Option<u8>,
    forced: Option<Forced>,
}

impl std::fmt::Debug for LandmarkStage {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("LandmarkStage")
            .field("active", &self.active)
            .field("history", &self.history)
            .finish_non_exhaustive()
    }
}

impl Default for LandmarkStage {
    fn default() -> Self {
        Self::new()
    }
}

impl LandmarkStage {
    /// Construit les neuf immeubles. À l'ouverture seulement : la construction calcule les voxels, les glyphes, les maillages.
    #[must_use]
    pub fn new() -> Self {
        let landmarks: Vec<Box<dyn Landmark>> = LandmarkKind::ALL
            .iter()
            .map(|kind| -> Box<dyn Landmark> {
                match kind {
                    LandmarkKind::Error => Box::new(ErrorBuilding::new()),
                    LandmarkKind::Matrix => Box::new(MatrixBuilding::new(MatrixVariant::Rain)),
                    LandmarkKind::Flux => Box::new(MatrixBuilding::new(MatrixVariant::Flux)),
                    LandmarkKind::Binary => Box::new(MatrixBuilding::new(MatrixVariant::Binary)),
                    LandmarkKind::Mecha => Box::new(MechaBuilding::new()),
                    LandmarkKind::Arch => Box::new(VoxelArch::new()),
                    LandmarkKind::Lava => Box::new(LavaBuilding::new()),
                    LandmarkKind::Arcade => Box::new(ArcadeBuilding::new()),
                    LandmarkKind::Board => Box::new(BoardTower::new()),
                }
            })
            .collect();
        let parts: Vec<Vec<PartSpec>> = landmarks.iter().map(|l| l.parts()).collect();
        let paint = parts.iter().map(|p| vec![PartPaint::default(); p.len()]).collect();
        Self {
            landmarks,
            parts,
            paint,
            active: None,
            seq: 0,
            field: FieldState::default(),
            light_levels: [0.0; MAX_LIGHTS],
            pulse: PulseGate::default(),
            inputs: StageInputs::default(),
            uniform: FrameUniform::default(),
            history: Vec::with_capacity(HISTORY + 1),
            fresh: [1.0; KIND_COUNT],
            marks: Vec::with_capacity(HISTORY),
            last_side: 1,
            last_tint: None,
            forced: None,
        }
    }

    /// Les pièces d'un immeuble, dans l'ordre de leur pose.
    #[must_use]
    pub fn parts(&self, kind: LandmarkKind) -> &[PartSpec] {
        &self.parts[kind.index()]
    }

    /// Ce que chaque pièce de `kind` montre à cette image.
    #[must_use]
    pub fn paint(&self, kind: LandmarkKind) -> &[PartPaint] {
        &self.paint[kind.index()]
    }

    /// L'immeuble de ce genre.
    #[must_use]
    pub fn landmark(&self, kind: LandmarkKind) -> &dyn Landmark {
        self.landmarks[kind.index()].as_ref()
    }

    /// L'immeuble de ce genre, en écriture.
    pub fn landmark_mut(&mut self, kind: LandmarkKind) -> &mut dyn Landmark {
        self.landmarks[kind.index()].as_mut()
    }

    /// L'immeuble visible, s'il y en a un.
    #[must_use]
    pub fn active(&self) -> Option<&Active> {
        self.active.as_ref()
    }

    /// Change à chaque pose et à chaque rangement : la couche de rendu s'en sert pour ne recopier la racine qu'au besoin.
    #[must_use]
    pub fn seq(&self) -> u32 {
        self.seq
    }

    /// Ce que les shaders lisent de l'image.
    #[must_use]
    pub fn uniform(&self) -> &FrameUniform {
        &self.uniform
    }

    /// Pose un immeuble. Un autre encore visible est rangé d'abord : jamais deux structures à la fois.
    pub fn show(&mut self, kind: LandmarkKind, place: Placement) {
        self.hide();
        self.landmarks[kind.index()].show(&place);
        self.active = Some(Active { kind, place });
        self.seq = self.seq.wrapping_add(1);
    }

    /// Range l'immeuble visible : éteint ses lumières, sa zone et son décalque (`hide` de `LandmarkBase`).
    pub fn hide(&mut self) {
        if let Some(active) = self.active.take() {
            self.landmarks[active.kind.index()].hide();
            self.seq = self.seq.wrapping_add(1);
        }
        self.field.clear();
        self.light_levels = [0.0; MAX_LIGHTS];
    }

    /// Avance l'immeuble visible d'une image et écrit la pose de ses pièces. `frame.beat.pulse` est le pouls **brut** de
    /// l'analyse : la scène le remplace par celui du limiteur avant de le donner à l'immeuble (règle 7 de `CLAUDE.md`).
    pub fn step(&mut self, frame: &mut LandmarkFrame) {
        let permitted = frame.policy.permitted();
        let pulse = self.pulse.update(frame.dt, frame.time, permitted, frame.beat.pulse);
        frame.beat.pulse = pulse;
        frame.beat.beat = self.inputs.beat;
        frame.colors = self.inputs.colors;
        let Some(active) = self.active else { return };
        let index = active.kind.index();
        let mut ctx = StepCtx::new(&mut self.field, &active.place, &mut self.light_levels, frame.dt);
        self.landmarks[index].step(frame, &mut ctx);
        self.landmarks[index].paint(frame, &mut self.paint[index]);
        self.uniform = FrameUniform {
            time: frame.time,
            pulse,
            bass: frame.beat.bass,
            mid: frame.beat.mid,
            high: frame.beat.high,
            beat: frame.beat.beat,
            intensity: frame.drive.intensity,
            phase: frame.phase,
        };
    }

    /// Les fraîcheurs des immeubles (`freshness.weight("landmark:<nom>")`), recopiées du chef du spectacle avant le tirage.
    pub fn set_freshness(&mut self, kind: LandmarkKind, weight: f64) {
        self.fresh[kind.index()] = if weight.is_finite() { weight.max(0.0) } else { 1.0 };
    }

    /// Les immeubles tirés depuis le dernier appel, à marquer utilisés chez le chef du spectacle (`freshness.mark`).
    pub fn take_marks(&mut self, mut mark: impl FnMut(LandmarkKind, f64)) {
        for (kind, now) in self.marks.drain(..) {
            mark(kind, now);
        }
    }

    /// Ordre de mise au point : le prochain tirage sera celui-ci (outil de développement).
    pub fn force(&mut self, forced: Option<Forced>) {
        self.forced = forced;
    }

    /// La mise au point en attente, consommée par le prochain événement.
    pub fn take_forced(&mut self) -> Option<Forced> {
        self.forced.take()
    }

    /// Un immeuble au hasard, jamais l'un des deux derniers ; les plus calmes seuls en mode « réduire les animations ».
    /// Au spectacle, le poids de chaque immeuble est multiplié par sa fraîcheur (`chooseLandmark`).
    pub fn choose(&mut self, rng: &mut Rand, calm: bool, now: f64) -> LandmarkKind {
        let recent = &self.history[self.history.len().saturating_sub(EXCLUDED)..];
        let allowed = |kind: &LandmarkKind| (!calm || kind.calm_safe()) && !recent.contains(kind);
        let any_allowed = LandmarkKind::ALL.iter().any(allowed);
        let pool = |kind: &&LandmarkKind| !any_allowed || allowed(kind);
        let weight = |kind: &LandmarkKind| kind.weight() * self.fresh[kind.index()];
        let total: f64 = LandmarkKind::ALL.iter().filter(pool).map(weight).sum();
        let mut roll = rng.unit() * total;
        let mut chosen = LandmarkKind::ALL
            .iter()
            .filter(pool)
            .last()
            .copied()
            .unwrap_or(LandmarkKind::Matrix);
        for kind in LandmarkKind::ALL.iter().filter(pool) {
            roll -= weight(kind);
            if roll <= 0.0 {
                chosen = *kind;
                break;
            }
        }
        self.marks.push((chosen, now));
        self.history.push(chosen);
        if self.history.len() > HISTORY {
            self.history.remove(0);
        }
        chosen
    }

    /// Allure du passage au spectacle (`landmarkVariation`) : un côté qui change le plus souvent, une teinte du cycle
    /// toujours différente de la précédente et, pour un immeuble qui n'enjambe pas la route, une échelle. Rend
    /// `(côté, rang de teinte, échelle)`.
    pub fn variation(&mut self, rng: &mut Rand, spans_road: bool, cycle_len: usize) -> (i8, u8, f32) {
        let side = if rng.unit() < 0.75 {
            -self.last_side
        } else {
            self.last_side
        };
        let size = cycle_len.max(1);
        let tint = match self.last_tint {
            None => (rng.unit() * size as f64) as usize % size,
            Some(last) => (usize::from(last) + 1 + (rng.unit() * (size - 1) as f64) as usize) % size,
        };
        self.last_side = side;
        self.last_tint = Some(tint as u8);
        let scale = if spans_road { 1.0 } else { rng.range(0.9, 1.18) as f32 };
        (side, tint as u8, scale)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn stage() -> LandmarkStage {
        LandmarkStage::new()
    }

    #[test]
    fn la_scene_a_les_pieces_de_chaque_immeuble_et_le_meme_nombre_de_poses() {
        let stage = stage();
        for kind in LandmarkKind::ALL {
            let parts = stage.parts(kind);
            assert!(!parts.is_empty(), "{}", kind.as_str());
            assert_eq!(parts.len(), stage.paint(kind).len(), "{}", kind.as_str());
        }
    }

    #[test]
    fn jamais_deux_structures_a_la_fois() {
        let mut stage = stage();
        let place = Placement {
            z: -1000.0,
            side: 1,
            tint: 0,
            scale: 1.0,
            spans_road: false,
        };
        stage.show(LandmarkKind::Lava, place);
        stage.show(LandmarkKind::Board, place);
        assert_eq!(stage.active().map(|a| a.kind), Some(LandmarkKind::Board));
        stage.hide();
        assert!(stage.active().is_none());
        assert!(stage.field.lights().iter().all(|l| l.color.length() == 0.0));
    }

    #[test]
    fn le_tirage_exclut_les_deux_derniers_et_le_mode_calme_les_bruyants() {
        let mut stage = stage();
        let mut rng = Rand::new(5);
        let mut last: Vec<LandmarkKind> = Vec::new();
        for i in 0..400 {
            let kind = stage.choose(&mut rng, false, f64::from(i));
            for earlier in last.iter().rev().take(2) {
                assert_ne!(*earlier, kind);
            }
            last.push(kind);
        }
        let mut calm = LandmarkStage::new();
        for i in 0..400 {
            assert!(calm.choose(&mut rng, true, f64::from(i)).calm_safe());
        }
    }

    #[test]
    fn le_mur_de_code_sort_deux_fois_plus_et_la_fraicheur_pese() {
        let mut stage = stage();
        let mut rng = Rand::new(11);
        let mut count = [0_u32; KIND_COUNT];
        for i in 0..9000 {
            let kind = stage.choose(&mut rng, false, f64::from(i));
            count[kind.index()] += 1;
        }
        let matrix_like = count[LandmarkKind::Matrix.index()]
            + count[LandmarkKind::Flux.index()]
            + count[LandmarkKind::Binary.index()];
        let others =
            count[LandmarkKind::Lava.index()] + count[LandmarkKind::Board.index()] + count[LandmarkKind::Error.index()];
        // Trois variantes à 2 contre trois autres à 1.
        assert!(matrix_like > others, "{matrix_like} / {others}");
        // Un immeuble « fatigué » (fraîcheur nulle) ne sort plus tant qu'il en reste d'autres.
        stage.set_freshness(LandmarkKind::Lava, 0.0);
        for i in 0..500 {
            assert_ne!(stage.choose(&mut rng, false, f64::from(i)), LandmarkKind::Lava);
        }
        let mut marked = 0;
        stage.take_marks(|_, _| marked += 1);
        assert_eq!(marked, 9500);
    }

    #[test]
    fn la_variation_change_de_teinte_a_chaque_passage_et_garde_l_echelle_des_arches() {
        let mut stage = stage();
        let mut rng = Rand::new(3);
        let mut last_tint = None;
        for _ in 0..200 {
            let (side, tint, scale) = stage.variation(&mut rng, false, 6);
            assert!(side == -1 || side == 1);
            assert!((0.9..=1.18).contains(&scale));
            assert_ne!(Some(tint), last_tint);
            assert!(tint < 6);
            last_tint = Some(tint);
        }
        assert!((stage.variation(&mut rng, true, 6).2 - 1.0).abs() < f32::EPSILON);
    }
}
