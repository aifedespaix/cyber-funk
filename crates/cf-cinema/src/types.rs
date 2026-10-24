//! Le vocabulaire du réalisateur (`cinema/types.ts`) : rangs, transitions, repères musicaux, et ce que la voiture et le
//! spectacle montrent à la caméra.
//!
//! Repère du monde (celui de Sillage) : la route file vers −z, la chaussée tient dans |x| < 9, les immeubles commencent
//! vers |x| > 11. La voiture roule à `speed` m/s (≈ 40 en croisière, jusqu'à ~70 en drop) et vole parfois (altitude
//! jusqu'à ~60 m).

use bevy::math::{DVec3, Vec3};
pub use cf_analysis::Section;
use cf_show::model::CutGrid;

/// Rang d'un plan dans le catalogue : plus il est rare, moins il revient.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ShotTier {
    /// Plan de base, l'essentiel du temps d'antenne.
    Common,
    /// Variante.
    Uncommon,
    /// Plan spécial.
    Rare,
    /// Plan événement.
    Epic,
}

/// Comment on entre dans un plan.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum TransitionKind {
    /// Coupe franche, sur un temps fort.
    Cut,
    /// Fondu de mouvement : position et orientation glissent d'un plan à l'autre (~1,5 à 3 s).
    Blend,
    /// Envolée : un fondu qui passe en arc par le haut (longs trajets, retour vers la voiture).
    Swoop,
    /// Panoramique éclair (~0,35 s) avec une bouffée de champ de vision.
    Whip,
}

/// Un repère musical.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum CueKind {
    /// Chaque kick.
    Beat,
    /// Premier temps d'une mesure (un kick sur 4).
    Bar,
    /// Début de phrase (un kick sur 16).
    Phrase,
    /// Le drop part.
    Drop,
    /// Changement de section (calme / groove / sommet).
    Section,
    /// Montée : l'énergie grimpe depuis plusieurs mesures.
    Build,
    /// Break : l'énergie retombe, place aux plans contemplatifs.
    Break,
}

/// Un repère musical de l'image, avec la section et l'énergie du moment.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Cue {
    /// Genre du repère.
    pub kind: CueKind,
    /// Section du morceau à cet instant.
    pub section: Section,
    /// Énergie lissée, 0..1.
    pub intensity: f32,
}

/// Phase du vol de la voiture (`FlightPhase` de Sillage).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum FlightPhase {
    /// Roues sur la chaussée.
    #[default]
    Ground,
    /// Les roues se déploient en répulseurs.
    Deploying,
    /// En vol.
    Flying,
    /// Les roues se replient, la voiture se pose.
    Retracting,
}

/// Ce que la voiture est en train de jouer : de quoi caler la caméra sur son animation, image par image.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct CarAnimation {
    /// Sol, roues qui se déploient, en vol, roues qui se replient.
    pub phase: FlightPhase,
    /// Avancement du déploiement des roues, 0 (repliées) → 1 (déployées).
    pub hover: f32,
    /// Vitesse verticale lissée (m/s, + = monte).
    pub climb: f32,
    /// Nez en l'air (+), en radians, sans les figures.
    pub pitch: f32,
    /// Roulis à droite (+), en radians, sans les figures.
    pub roll: f32,
    /// Rebond de la caisse sur ses répulseurs (m), poussé par les kicks.
    pub bounce: f32,
    /// Esquive d'une structure : −1 par dessous, 1 par dessus, 0 aucune.
    pub dodge: i8,
}

/// Une figure en vol.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TrickKind {
    /// Tonneau (gauche ou droite).
    Roll,
    /// Salto (avant ou arrière).
    Flip,
}

/// Figure en cours et sa progression.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Trick {
    /// Genre de figure.
    pub kind: TrickKind,
    /// Progression, 0 → 1.
    pub progress: f32,
}

/// Cascade scénarisée en cours (pilote automatique, sans points).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Stunt {
    /// Genre de cascade (`"ascension"`, …) : identifiant stable des cascades de Sillage.
    pub kind: &'static str,
    /// Progression, 0 → 1.
    pub progress: f32,
    /// Le point à regarder (absolu, m).
    pub focus: DVec3,
}

/// Les yeux du conducteur : la pose de la vue au volant.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Cabin {
    /// Position absolue (m).
    pub position: DVec3,
    /// Regard, unitaire.
    pub forward: Vec3,
    /// Haut, unitaire.
    pub up: Vec3,
}

impl Default for Cabin {
    fn default() -> Self {
        Self {
            position: DVec3::ZERO,
            forward: Vec3::NEG_Z,
            up: Vec3::Y,
        }
    }
}

/// La géante gazeuse à anneau du ciel. Le ciel suit la caméra : ses coordonnées sont locales au ciel (voir
/// `CameraPose::sky_drift`).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct RingPlanet {
    /// Centre (m, repère du ciel).
    pub center: Vec3,
    /// Rayon de la planète (m).
    pub radius: f32,
    /// Rayon intérieur de l'anneau (m).
    pub ring_inner: f32,
    /// Rayon extérieur de l'anneau (m).
    pub ring_outer: f32,
    /// Normale (unitaire) du plan de l'anneau, repère local du ciel.
    pub ring_normal: Vec3,
}

/// Dernier astre écrasé contre un immeuble.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct MeteorBlast {
    /// Point d'impact (absolu, m).
    pub at: DVec3,
    /// Rayon de l'explosion (m).
    pub radius: f32,
    /// `CinemaWorld::time` de l'impact.
    pub time: f64,
    /// Identifiant de l'astre.
    pub meteor_id: u64,
}

/// L'état du rythme vu par le réalisateur (le `BeatState` de Sillage, ce qu'il en lit).
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct BeatView {
    /// Nombre de kicks depuis le début de l'analyse : on compte les temps par différence.
    pub count: u32,
    /// Tempo détecté (battements par minute), s'il est établi.
    pub bpm: Option<f32>,
    /// Impulsion décroissante après chaque kick, 1 → 0.
    pub pulse: f32,
    /// Énergie des basses, lissée 0 → 1 (le `beat.bass` de Sillage).
    pub bass: f32,
}

/// Le spectacle vu par le réalisateur (ce qu'il lit du `ShowState`). Recopié à chaque image : `Copy`, sans allocation.
#[derive(Debug, Clone, Copy, PartialEq)]
#[allow(clippy::struct_excessive_bools)] // Copie des drapeaux du `ShowState` que lit le réalisateur.
pub struct ShowView {
    /// Un spectacle tourne (un show est chargé). Faux : rythme de clip historique, sans portes.
    pub active: bool,
    /// Mode respiration : plans lents, jamais de coupe franche.
    pub breathing: bool,
    /// Vrai pendant une seule image : le drop frappe maintenant.
    pub drop_now: bool,
    /// Un drop est préparé (`show.drop !== null` de Sillage) : le plan `dolly-zoom` s'y cale. Pas encore lu depuis le
    /// `ShowState` (voir la PR, « Écarts ») : faux tant que le monde ne le fournit pas.
    pub drop_planned: bool,
    /// Flashs permis (lu par J6-02 pour le plan `strobe`).
    pub flashes: bool,
    /// Grille de coupe du look courant, en temps musicaux par section.
    pub cut_grid: CutGrid,
}

impl Default for ShowView {
    fn default() -> Self {
        Self {
            active: false,
            breathing: false,
            drop_now: false,
            drop_planned: false,
            flashes: true,
            cut_grid: CutGrid {
                calm: 16,
                groove: 8,
                peak: 4,
            },
        }
    }
}

impl ShowView {
    /// Recopie ce que le réalisateur lit du `ShowState` de l'image.
    #[must_use]
    pub fn of(state: &cf_show::director::ShowState) -> Self {
        Self {
            active: true,
            breathing: state.breathing,
            drop_now: state.drop_now,
            // Le drop préparé vit dans le `ShowDirector` (`drop_plan()`), pas dans le `ShowState` : non lu ici.
            drop_planned: false,
            flashes: state.flashes,
            cut_grid: state.gates().cut_grid,
        }
    }

    /// Temps musicaux de la grille pour `section`.
    #[must_use]
    pub fn grid_for(&self, section: Section) -> u8 {
        match section {
            Section::Calm => self.cut_grid.calm,
            Section::Groove => self.cut_grid.groove,
            Section::Peak => self.cut_grid.peak,
        }
    }
}
