//! L'étalonnage de quartier : une modulation **bornée** de la palette à l'écran, posée après le thème du show et la
//! rotation de teinte du look. Ce n'est jamais un remplacement : le changement de DJ reste le grand changement de
//! couleur, le quartier ne fait que le nuancer (teinte, saturation, luminosité, poids de quatre accents).
//!
//! ```text
//! district_grades.ron ─▶ GradeLibrary ─▶ DistrictGradeState (cible, attente d'un temps fort, fondu linéaire)
//! thème du show ─▶ fondu ─▶ teinte du look ─▶ ScenePalette::graded(état.update(dt)) ─▶ Res<ScenePalette>
//! ```

use bevy::prelude::Resource;
use cf_show::district::DistrictId;

use super::color::{Hsl, LinearRgb};
use super::fader::{DOWNBEAT_EVERY, MAX_FADE, MAX_WAIT};
use super::grade_library::GradeLibrary;
use super::scene::{SceneColor, ScenePalette};

/// Les rôles d'accent, dans l'ordre fixe de [`DistrictGrade::accents`] (même ordre pour tous les étalonnages : c'est
/// ce qui permet d'interpoler sans à-coup d'un quartier à l'autre).
pub const ACCENT_ROLES: [SceneColor; 4] = [SceneColor::Mint, SceneColor::Rose, SceneColor::Amber, SceneColor::Ice];

/// Borne de la rotation de teinte, en degrés.
pub const MAX_HUE_DEG: f32 = 30.0;
/// Bornes du facteur de saturation.
pub const SATURATION_RANGE: (f32, f32) = (0.8, 1.2);
/// Bornes du facteur de luminosité. Ces bornes seules ne garantissent pas les 10 % de luminance (règle 7) : c'est
/// vérifié sur les valeurs livrées de `district_grades.ron` (test `luminance_voisine`).
pub const LIGHTNESS_RANGE: (f32, f32) = (0.9, 1.1);
/// Bornes du poids d'un accent (même remarque que [`LIGHTNESS_RANGE`] pour la luminance).
pub const ACCENT_RANGE: (f32, f32) = (0.8, 1.25);
/// Nombre de variantes par type, au minimum et au maximum.
pub const VARIANTS_RANGE: (usize, usize) = (6, 8);

/// Un étalonnage : la modulation d'un quartier. Les valeurs livrées respectent les bornes ci-dessus (testé).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct DistrictGrade {
    /// Rotation de teinte, en degrés (|x| ≤ 30).
    pub hue_deg: f32,
    /// Facteur de saturation, dans [0,8 ; 1,2].
    pub saturation: f32,
    /// Facteur de luminosité (gain de lumière, exposant 2,2), dans [0,9 ; 1,1].
    pub lightness: f32,
    /// Poids des accents (facteur sur la lumière linéaire), dans [0,8 ; 1,25], dans l'ordre de [`ACCENT_ROLES`].
    ///
    /// Écart à la fiche : quatre accents et non trois, pour couvrir menthe, corail, ambre et glace avec le même ordre
    /// partout (une interpolation entre deux étalonnages aux accents différents n'a pas de sens sinon).
    pub accents: [(SceneColor, f32); 4],
}

impl DistrictGrade {
    /// L'étalonnage neutre : la palette passe inchangée, à l'octet près.
    #[must_use]
    pub const fn neutral() -> Self {
        Self {
            hue_deg: 0.0,
            saturation: 1.0,
            lightness: 1.0,
            accents: [
                (ACCENT_ROLES[0], 1.0),
                (ACCENT_ROLES[1], 1.0),
                (ACCENT_ROLES[2], 1.0),
                (ACCENT_ROLES[3], 1.0),
            ],
        }
    }

    /// Vrai pour l'identité.
    #[must_use]
    pub fn is_neutral(&self) -> bool {
        *self == Self::neutral()
    }

    /// Le mélange linéaire de deux étalonnages (0 : `self`, 1 : `other`). Reste dans les bornes si les deux y sont.
    #[must_use]
    pub fn lerp(&self, other: &Self, t: f32) -> Self {
        let mix = |a: f32, b: f32| a + (b - a) * t;
        let mut accents = self.accents;
        for (slot, (_, b)) in accents.iter_mut().zip(other.accents) {
            slot.1 = mix(slot.1, b);
        }
        Self {
            hue_deg: mix(self.hue_deg, other.hue_deg),
            saturation: mix(self.saturation, other.saturation),
            lightness: mix(self.lightness, other.lightness),
            accents,
        }
    }

    /// Le poids appliqué à un rôle (1 hors accents).
    fn weight(&self, role: SceneColor) -> f32 {
        self.accents.iter().find(|(r, _)| *r == role).map_or(1.0, |(_, w)| *w)
    }

    /// Vrai si toutes les valeurs sont dans les bornes.
    #[must_use]
    pub fn within_bounds(&self) -> bool {
        self.hue_deg.abs() <= MAX_HUE_DEG
            && (SATURATION_RANGE.0..=SATURATION_RANGE.1).contains(&self.saturation)
            && (LIGHTNESS_RANGE.0..=LIGHTNESS_RANGE.1).contains(&self.lightness)
            && self
                .accents
                .iter()
                .all(|(_, w)| (ACCENT_RANGE.0..=ACCENT_RANGE.1).contains(w))
    }
}

impl ScenePalette {
    /// La palette modulée par un étalonnage de quartier. Le rouge d'alarme n'est pas touché (c'est le seul rouge franc
    /// de la scène). L'étalonnage neutre rend la palette telle quelle, à l'octet près.
    #[must_use]
    pub fn graded(&self, grade: &DistrictGrade) -> Self {
        if grade.is_neutral() {
            return *self;
        }
        let turn = grade.hue_deg / 360.0;
        // La luminosité est un gain sur la lumière linéaire (HSL en sRGB ~ puissance 2,2) : c'est le seul levier de
        // luminance de l'étalonnage, avec le poids des accents. La teinte et la saturation, elles, gardent la luminance
        // de chaque rôle (voir `keep_luminance`) : sans cela, tourner le cyan vers le vert éclaire la scène de 20 %.
        let gain = grade.lightness.powf(2.2);
        let mut out = *self;
        for role in SceneColor::ALL {
            if role == SceneColor::Alarm {
                continue;
            }
            let before = self.get(role);
            let Hsl { h, s, l } = before.to_hsl();
            let moved = keep_luminance(
                before,
                LinearRgb::from_hsl(Hsl {
                    h: h + turn,
                    s: (s * grade.saturation).clamp(0.0, 1.0),
                    l,
                }),
            );
            let w = gain * grade.weight(role);
            out.set(
                role,
                LinearRgb {
                    r: moved.r * w,
                    g: moved.g * w,
                    b: moved.b * w,
                },
            );
        }
        out
    }
}

/// Luminance relative (Rec. 709) d'une couleur linéaire.
fn luminance(c: LinearRgb) -> f32 {
    0.2126 * c.r + 0.7152 * c.g + 0.0722 * c.b
}

/// Remet `moved` à la luminance de `before`, sans jamais pousser un canal au-delà de 1 (on ne fait pas déborder la
/// gamme : mieux vaut un peu plus sombre qu'écrêté).
fn keep_luminance(before: LinearRgb, moved: LinearRgb) -> LinearRgb {
    let (y_before, y_moved) = (luminance(before), luminance(moved));
    if y_moved < 1e-6 || y_before < 1e-6 {
        return moved;
    }
    let peak = moved.r.max(moved.g).max(moved.b);
    let scale = (y_before / y_moved).min(1.0 / peak.max(1e-6)).max(0.0);
    LinearRgb {
        r: moved.r * scale,
        g: moved.g * scale,
        b: moved.b * scale,
    }
}

/// Une cible d'étalonnage qui attend son temps fort.
#[derive(Debug, Clone, Copy, PartialEq)]
struct PendingGrade {
    grade: DistrictGrade,
    fade_s: f32,
    waited_s: f32,
}

/// L'état de l'étalonnage de quartier : d'où l'on vient, où l'on va, l'avancement du fondu linéaire.
///
/// Le monde (tâche 4) appelle [`DistrictGradeState::set_target`] ; le seul `drive_palette` appelle
/// [`DistrictGradeState::tick_pending`] puis [`DistrictGradeState::update`]. Changer de thème de show n'y touche pas :
/// l'étalonnage se pose sur le nouveau thème tel quel.
#[derive(Resource, Debug, Clone)]
pub struct DistrictGradeState {
    library: GradeLibrary,
    from: DistrictGrade,
    to: DistrictGrade,
    fade_s: f32,
    elapsed_s: f32,
    pending: Option<PendingGrade>,
}

impl Default for DistrictGradeState {
    fn default() -> Self {
        let library = GradeLibrary::builtin().unwrap_or_else(|error| {
            // Compilé dans le binaire et testé : ce chemin n'existe qu'en cas de bug. Sans étalonnages, tout reste neutre.
            tracing::error!(%error, "palettes : étalonnages de quartier livrés illisibles");
            GradeLibrary::default()
        });
        Self::new(library)
    }
}

impl DistrictGradeState {
    /// Un état neutre, sans cible, avec ces étalonnages.
    #[must_use]
    pub fn new(library: GradeLibrary) -> Self {
        Self {
            library,
            from: DistrictGrade::neutral(),
            to: DistrictGrade::neutral(),
            fade_s: 0.0,
            elapsed_s: 0.0,
            pending: None,
        }
    }

    /// Les étalonnages connus.
    #[must_use]
    pub fn library(&self) -> &GradeLibrary {
        &self.library
    }

    /// Demande l'étalonnage du quartier `id`, en `fade_s` secondes (borné à [`MAX_FADE`]). Le fondu démarre au
    /// prochain temps fort (au plus [`MAX_WAIT`] s d'attente) ; une nouvelle demande remplace celle qui attend.
    pub fn set_target(&mut self, id: DistrictId, fade_s: f32) {
        let grade = self.library.grade(id);
        let fade_s = if fade_s.is_finite() {
            fade_s.clamp(0.0, MAX_FADE)
        } else {
            MAX_FADE
        };
        // Déjà sur cet étalonnage et rien ne glisse : pas de fondu inutile entre deux étalonnages identiques.
        if !self.is_fading() && grade == self.to {
            self.pending = None;
            return;
        }
        // Même cible déjà en attente (l'appelant peut ré-émettre à chaque image) : on garde l'attente écoulée, sinon
        // le repli de MAX_WAIT secondes ne partirait jamais.
        if let Some(pending) = self.pending.as_mut()
            && pending.grade == grade
        {
            pending.fade_s = fade_s;
            return;
        }
        self.pending = Some(PendingGrade {
            grade,
            fade_s,
            waited_s: 0.0,
        });
    }

    /// Une cible attend son temps fort.
    #[must_use]
    pub fn is_pending(&self) -> bool {
        self.pending.is_some()
    }

    /// Un fondu d'étalonnage est en cours.
    #[must_use]
    pub fn is_fading(&self) -> bool {
        self.elapsed_s < self.fade_s
    }

    /// Libère la cible en attente sur un temps fort (un kick sur [`DOWNBEAT_EVERY`]) ou après [`MAX_WAIT`] s. Le
    /// fondu repart de l'étalonnage courant (jamais d'à-coup, même si on change de cible en plein fondu).
    pub fn tick_pending(&mut self, dt_s: f32, kick: bool, count: u32) {
        let Some(pending) = self.pending.as_mut() else {
            return;
        };
        pending.waited_s += dt_s.max(0.0);
        if (kick && count.is_multiple_of(DOWNBEAT_EVERY)) || pending.waited_s >= MAX_WAIT {
            let pending = *pending;
            self.pending = None;
            self.from = self.current();
            self.to = pending.grade;
            self.fade_s = pending.fade_s;
            self.elapsed_s = 0.0;
        }
    }

    /// L'étalonnage à cet instant.
    fn current(&self) -> DistrictGrade {
        if self.elapsed_s >= self.fade_s {
            return self.to;
        }
        self.from.lerp(&self.to, self.elapsed_s / self.fade_s)
    }

    /// L'étalonnage courant, sans rien avancer (lecture seule).
    #[must_use]
    pub fn settled(&self) -> DistrictGrade {
        self.current()
    }

    /// Avance le fondu de `dt_s` et rend l'étalonnage courant (interpolation linéaire, jamais d'à-coup).
    pub fn update(&mut self, dt_s: f32) -> DistrictGrade {
        if self.elapsed_s < self.fade_s {
            self.elapsed_s = (self.elapsed_s + dt_s.max(0.0)).min(self.fade_s);
        }
        self.current()
    }
}
