//! Ce que reçoit un immeuble-vitrine à chaque image (`LandmarkFrame`, `Drive` de `landmarks/types.ts`) et la palette
//! qu'il lit. Des copies légères : toute la logique des immeubles reste testable sans `App`.

use bevy::math::{DVec3, Vec3};
use cf_analysis::Section;
use cf_render::{SceneColor, ScenePalette};

/// L'audio de l'image (`BeatState`). `pulse` est **déjà borné** par le limiteur de flashs (règle 7 de `CLAUDE.md`) : tout
/// ce qui brille sur un kick lit celui-là, jamais le pouls brut de l'analyse.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct LandmarkBeat {
    /// Un kick vient d'avoir lieu (vrai une seule image).
    pub kick: bool,
    /// Basses, médiums, aigus : énergie 0..1.
    pub bass: f32,
    /// Médiums, 0..1.
    pub mid: f32,
    /// Aigus, 0..1.
    pub high: f32,
    /// Volume global, 0..1.
    pub loud: f32,
    /// Pouls du kick, 0..1, rendu par le limiteur (voir [`crate::landmarks::flash::PulseGate`]).
    pub pulse: f32,
    /// Nombre de kicks depuis le début de l'analyse.
    pub count: u32,
    /// Tempo détecté, battements par minute.
    pub bpm: Option<f32>,
    /// Temps musical continu (`beat.beat` : le numéro du temps plus une montée douce de 0 à 1).
    pub beat: f32,
}

impl LandmarkBeat {
    /// Le silence : ni kick ni énergie.
    pub const SILENT: Self = Self {
        kick: false,
        bass: 0.0,
        mid: 0.0,
        high: 0.0,
        loud: 0.0,
        pulse: 0.0,
        count: 0,
        bpm: None,
        beat: 0.0,
    };
}

/// La forme du morceau, au-delà du kick (`Drive`).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Drive {
    /// Vrai pendant une seule image, au moment d'un drop ou de l'entrée dans le sommet.
    pub drop: bool,
    /// 0 (calme) → 1 (sommet), lissé sur quelques secondes.
    pub intensity: f32,
    /// Section du morceau.
    pub section: Section,
    /// Teinte ambiante : cyan au calme, violet en groove, corail au sommet (lumière linéaire).
    pub mood: Vec3,
    /// Vrai dans les dernières secondes du titre.
    pub outro: bool,
}

/// Les couleurs de la scène, en lumière linéaire, par rôle (`ScenePalette` de Sillage). Ce que les immeubles lisent pour
/// teinter leurs lumières ; les shaders reçoivent les mêmes onze couleurs.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Colors {
    roles: [Vec3; 11],
    cycle: [Vec3; 6],
}

impl Default for Colors {
    fn default() -> Self {
        Self {
            roles: [Vec3::ONE; 11],
            cycle: [Vec3::ONE; 6],
        }
    }
}

fn vec3(c: cf_render::palette::LinearRgb) -> Vec3 {
    Vec3::new(c.r, c.g, c.b)
}

impl Colors {
    /// Les couleurs de la palette vivante de l'image.
    #[must_use]
    pub fn of(palette: &ScenePalette) -> Self {
        let mut roles = [Vec3::ZERO; 11];
        for role in SceneColor::ALL {
            roles[role as usize] = vec3(palette.get(role));
        }
        Self {
            roles,
            cycle: palette.prismatic_cycle().map(vec3),
        }
    }

    /// La couleur d'un rôle.
    #[must_use]
    pub fn get(&self, role: SceneColor) -> Vec3 {
        self.roles[role as usize]
    }

    /// Les onze rôles, dans l'ordre de [`SceneColor::ALL`].
    #[must_use]
    pub fn roles(&self) -> &[Vec3; 11] {
        &self.roles
    }

    /// Le cycle prismatique (les six teintes des tours).
    #[must_use]
    pub fn cycle(&self) -> &[Vec3; 6] {
        &self.cycle
    }
}

/// La politique de flashs du spectacle : `(flashes, breathing)` ; sans spectacle, rien ne flashe (comme le trafic).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FlashPolicy {
    /// Le look permet les flashs (le limiteur s'applique quoi qu'il arrive).
    pub flashes: bool,
    /// Mode respiration : seulement du lent et du doux.
    pub breathing: bool,
}

impl FlashPolicy {
    /// Pas de spectacle : rien ne flashe.
    pub const NONE: Self = Self {
        flashes: false,
        breathing: true,
    };

    /// Les flashs sont-ils permis ?
    #[must_use]
    pub const fn permitted(self) -> bool {
        self.flashes && !self.breathing
    }
}

/// Tout ce qu'un immeuble-vitrine reçoit à chaque image pour réagir à la musique (`LandmarkFrame`).
#[derive(Debug, Clone, Copy)]
pub struct LandmarkFrame {
    /// Horloge de la scène, s.
    pub time: f64,
    /// Durée de l'image, s.
    pub dt: f32,
    /// L'audio (pouls borné).
    pub beat: LandmarkBeat,
    /// La forme du morceau.
    pub drive: Drive,
    /// La voiture, absolue, m.
    pub car: DVec3,
    /// Mode « réduire les animations » : pas de stroboscope, mouvements lents.
    pub calm: bool,
    /// 0 → 1 entre deux kicks, recalé sur le BPM détecté. Contrairement à `beat.pulse` (qui retombe en 0,22 s), il avance
    /// au rythme du morceau : les mouvements continus (balancier, torsion) restent calés sur le tempo.
    pub phase: f32,
    /// La politique de flashs du spectacle.
    pub policy: FlashPolicy,
    /// La palette vivante.
    pub colors: Colors,
}

impl LandmarkFrame {
    /// Une image neutre (tests) : au repos, au sol, sans spectacle.
    #[must_use]
    pub fn rest() -> Self {
        Self {
            time: 0.0,
            dt: 1.0 / 60.0,
            beat: LandmarkBeat::SILENT,
            drive: Drive {
                drop: false,
                intensity: 0.4,
                section: Section::Groove,
                mood: Vec3::new(0.4, 0.2, 0.9),
                outro: false,
            },
            car: DVec3::ZERO,
            calm: false,
            phase: 1.0,
            policy: FlashPolicy::NONE,
            colors: Colors::default(),
        }
    }
}
