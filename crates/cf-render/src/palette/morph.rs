//! Le fondu de palette (`LivePalette` de `engine/live-palette.ts`) : la palette glisse sur place vers une autre, sans
//! reconstruire la scène. Logique pure, sans Bevy : le système du module l'avance de `dt` à chaque image.

use super::color::{Hsl, LinearRgb};
use super::scene::{SceneColor, ScenePalette};

/// Cadence maximale de l'événement `Changed` pendant un fondu, en secondes.
pub const CHANGE_INTERVAL: f32 = 0.25;
/// En dessous de cette saturation (min des deux bouts), le fondu passe par le mélange linéaire et non par la teinte.
const HUE_FROM: f32 = 0.15;
const HUE_TO: f32 = 0.5;

/// Ce qu'une image de fondu signale aux abonnés (rafraîchissements coûteux : textures peintes, couleurs cuites).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MorphEvent {
    /// Rien de nouveau pour les abonnés.
    None,
    /// Le fondu avance (au plus tous les [`CHANGE_INTERVAL`] s).
    Changed,
    /// Le fondu est fini : la palette est exactement la cible.
    Settled,
}

fn smooth(t: f32) -> f32 {
    t * t * (3.0 - 2.0 * t)
}

/// La palette vivante : la couleur courante et le fondu en cours vers une cible.
#[derive(Debug, Clone)]
pub struct PaletteMorph {
    current: ScenePalette,
    from: ScenePalette,
    to: ScenePalette,
    from_hsl: [Hsl; 11],
    to_hsl: [Hsl; 11],
    duration_s: f32,
    elapsed_s: f32,
    active: bool,
    since_change_s: f32,
}

impl PaletteMorph {
    /// Une palette au repos sur `palette`.
    #[must_use]
    pub fn new(palette: ScenePalette) -> Self {
        Self {
            current: palette,
            from: palette,
            to: palette,
            from_hsl: hsl_of(&palette),
            to_hsl: hsl_of(&palette),
            duration_s: 0.0,
            elapsed_s: 0.0,
            active: false,
            since_change_s: 0.0,
        }
    }

    /// La palette courante (celle du fondu, jamais remplacée par un saut).
    #[must_use]
    pub fn palette(&self) -> &ScenePalette {
        &self.current
    }

    /// Un fondu est en cours.
    #[must_use]
    pub fn morphing(&self) -> bool {
        self.active
    }

    /// Avancement du fondu, 0 → 1 (1 au repos).
    #[must_use]
    pub fn progress(&self) -> f32 {
        if self.active {
            (self.elapsed_s / self.duration_s).clamp(0.0, 1.0)
        } else {
            1.0
        }
    }

    /// Fait glisser la palette vers `target` en `seconds`. Part de la couleur actuelle : un fondu en cours se reprend
    /// sans saut. Une durée nulle pose la cible d'un coup (renvoie alors [`MorphEvent::Settled`]).
    pub fn morph_to(&mut self, target: &ScenePalette, seconds: f32) -> MorphEvent {
        self.from = self.current;
        self.to = *target;
        self.from_hsl = hsl_of(&self.from);
        self.to_hsl = hsl_of(&self.to);
        self.elapsed_s = 0.0;
        self.duration_s = if seconds.is_finite() { seconds.max(0.0) } else { 0.0 };
        self.since_change_s = 0.0;
        if self.duration_s <= 0.0 {
            self.finish();
            return MorphEvent::Settled;
        }
        self.active = true;
        MorphEvent::None
    }

    /// Avance le fondu de `dt_s` secondes.
    pub fn update(&mut self, dt_s: f32) -> MorphEvent {
        if !self.active {
            return MorphEvent::None;
        }
        self.elapsed_s += dt_s;
        self.since_change_s += dt_s;
        if self.elapsed_s >= self.duration_s {
            self.finish();
            return MorphEvent::Settled;
        }
        let t = smooth(self.elapsed_s / self.duration_s);
        for color in SceneColor::ALL {
            self.blend(color, t);
        }
        if self.since_change_s >= CHANGE_INTERVAL {
            self.since_change_s = 0.0;
            return MorphEvent::Changed;
        }
        MorphEvent::None
    }

    /// Mélange `from` → `to` : par la teinte quand les deux bouts sont saturés (pas de gris boueux entre deux couleurs
    /// vives), sinon en linéaire.
    fn blend(&mut self, color: SceneColor, t: f32) {
        let i = color as usize;
        let linear = self.from.get(color).lerp(self.to.get(color), t);
        let (a, b) = (self.from_hsl[i], self.to_hsl[i]);
        let weight = smooth(((a.s.min(b.s) - HUE_FROM) / (HUE_TO - HUE_FROM)).clamp(0.0, 1.0));
        if weight <= 0.0 {
            self.current.set(color, linear);
            return;
        }
        let mut dh = b.h - a.h;
        if dh > 0.5 {
            dh -= 1.0;
        } else if dh < -0.5 {
            dh += 1.0;
        }
        let mixed = LinearRgb::from_hsl(Hsl {
            h: (a.h + dh * t + 1.0).rem_euclid(1.0),
            s: a.s + (b.s - a.s) * t,
            l: a.l + (b.l - a.l) * t,
        });
        self.current.set(color, linear.lerp(mixed, weight));
    }

    fn finish(&mut self) {
        self.current = self.to;
        self.active = false;
        self.elapsed_s = self.duration_s;
    }
}

fn hsl_of(palette: &ScenePalette) -> [Hsl; 11] {
    SceneColor::ALL.map(|color| palette.get(color).to_hsl())
}

#[cfg(test)]
mod tests {
    use cf_show::model::PresetTheme;

    use super::*;
    use crate::palette::ThemeLibrary;

    fn themes() -> (ScenePalette, ScenePalette) {
        let library = ThemeLibrary::builtin().unwrap();
        (
            *library.preset(PresetTheme::Velvet),
            *library.preset(PresetTheme::Synthwave),
        )
    }

    /// Plus grand écart (octets sRGB) entre deux palettes.
    fn distance(a: &ScenePalette, b: &ScenePalette) -> u8 {
        SceneColor::ALL
            .iter()
            .flat_map(|c| {
                let (x, y) = (a.get(*c).to_srgb8(), b.get(*c).to_srgb8());
                [x[0].abs_diff(y[0]), x[1].abs_diff(y[1]), x[2].abs_diff(y[2])]
            })
            .max()
            .unwrap_or(0)
    }

    #[test]
    fn le_fondu_arrive_exactement_sur_la_cible() {
        let (velvet, synthwave) = themes();
        let mut morph = PaletteMorph::new(velvet);
        assert_eq!(morph.morph_to(&synthwave, 2.0), MorphEvent::None);
        let mut settled = 0;
        for _ in 0..200 {
            if morph.update(1.0 / 60.0) == MorphEvent::Settled {
                settled += 1;
            }
        }
        assert_eq!(settled, 1);
        assert!(!morph.morphing());
        assert_eq!(morph.palette(), &synthwave);
    }

    #[test]
    fn duree_nulle_pose_la_cible_d_un_coup() {
        let (velvet, synthwave) = themes();
        let mut morph = PaletteMorph::new(velvet);
        assert_eq!(morph.morph_to(&synthwave, 0.0), MorphEvent::Settled);
        assert_eq!(morph.palette(), &synthwave);
    }

    #[test]
    fn jamais_de_saut_d_une_image_a_l_autre() {
        // Fondu d'une mesure à 128 bpm (1,875 s) à 60 i/s : aucune couleur ne saute de plus de 12 octets.
        let (velvet, synthwave) = themes();
        let mut morph = PaletteMorph::new(velvet);
        morph.morph_to(&synthwave, 60.0 / 128.0 * 4.0);
        let mut previous = *morph.palette();
        let mut worst = 0;
        while morph.morphing() {
            morph.update(1.0 / 60.0);
            worst = worst.max(distance(&previous, morph.palette()));
            previous = *morph.palette();
        }
        assert!(worst <= 12, "saut maximal : {worst} octets");
    }

    #[test]
    fn un_fondu_en_cours_se_reprend_sans_saut() {
        let (velvet, synthwave) = themes();
        let library = ThemeLibrary::builtin().unwrap();
        let gold = *library.preset(PresetTheme::Gold);
        let mut morph = PaletteMorph::new(velvet);
        morph.morph_to(&synthwave, 2.0);
        for _ in 0..60 {
            morph.update(1.0 / 60.0);
        }
        let before = *morph.palette();
        morph.morph_to(&gold, 2.0);
        assert_eq!(morph.palette(), &before);
        morph.update(1.0 / 60.0);
        assert!(distance(&before, morph.palette()) <= 6);
    }

    #[test]
    fn changed_au_plus_tous_les_quarts_de_seconde() {
        let (velvet, synthwave) = themes();
        let mut morph = PaletteMorph::new(velvet);
        morph.morph_to(&synthwave, 2.0);
        let changed = (0..119)
            .filter(|_| morph.update(1.0 / 60.0) == MorphEvent::Changed)
            .count();
        // 2 s / 0,25 s = 8 au plus.
        assert!((6..=8).contains(&changed), "{changed}");
    }

    #[test]
    fn au_repos_rien_ne_bouge() {
        let (velvet, _) = themes();
        let mut morph = PaletteMorph::new(velvet);
        assert_eq!(morph.update(1.0), MorphEvent::None);
        assert!((morph.progress() - 1.0).abs() < f32::EPSILON);
    }
}
