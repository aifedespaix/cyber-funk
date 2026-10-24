//! La lumière de la voiture : teinte du moment, néons, optiques, halos, lueurs sur la route. Logique pure : ce module
//! décide des couleurs et des niveaux à chaque image, `lights.rs` et `scene.rs` les appliquent aux matériaux.
//!
//! Portage de `CarLights.update` (`car-lights.ts`), `CyberGlow.update` (`cyber/glow.ts`), `Blinker` (`cyber/blinker.ts`)
//! et du logo arrière de `CyberCar.ts` (`updateLogo`).
//!
//! **Les kicks** : le pouls (`AudioFeatures::pulse`, 1 au kick puis 0 en ~0,12 s) pousse les halos des phares, le volume
//! lumineux des faisceaux, les flaques devant la voiture, la lueur sous la caisse, le logo arrière et le liseré de la
//! peinture ; les basses règlent le logo.

use bevy::color::{LinearRgba, Mix};

/// Durée de la migration de teinte quand la voiture roule sur un faisceau, s (`GLOW_SECONDS`).
pub const GLOW_SECONDS: f32 = 0.5;
/// Intensité des phares en croisière (`lights.update(…, highBeams ? 1.6 : 1.1, …)`).
pub const HEADLIGHT_LEVEL: f32 = 1.1;
/// Intensité des feux de route.
pub const HIGH_BEAMS_LEVEL: f32 = 1.6;
/// Vitesse de croisière rapportée au-delà de laquelle les feux de route s'allument (un boost).
pub const HIGH_BEAMS_RATIO: f32 = 1.3;
/// Multiplicateur d'intensité des néons exportés (`neonIntensity`, 0,45 par défaut dans le modèle).
pub const NEON_INTENSITY: f32 = 0.45;

/// Les couleurs de la scène que lit la voiture, en lumière linéaire.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct CarPalette {
    /// `--scene-violet` : la lueur de départ et la peinture.
    pub violet: LinearRgba,
    /// `--scene-rose` : le corail qui se mêle au second néon.
    pub rose: LinearRgba,
    /// `--scene-ice` : le blanc des phares.
    pub ice: LinearRgba,
    /// `--scene-alarm` : les feux arrière.
    pub alarm: LinearRgba,
    /// `--scene-amber` : les clignotants.
    pub amber: LinearRgba,
}

/// Migration de la teinte de la voiture (`setGlow`, `glow`) : `GLOW_SECONDS` de fondu, adouci par un `smoothstep`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct GlowFade {
    current: LinearRgba,
    from: LinearRgba,
    target: LinearRgba,
    progress: f32,
}

impl GlowFade {
    /// Démarre à `color`, sans fondu.
    #[must_use]
    pub fn new(color: LinearRgba) -> Self {
        Self {
            current: color,
            from: color,
            target: color,
            progress: 1.0,
        }
    }

    /// Demande à migrer vers `color` ; sans effet si c'est déjà la cible.
    pub fn set_target(&mut self, color: LinearRgba) {
        if self.target == color {
            return;
        }
        self.target = color;
        self.from = self.current;
        self.progress = 0.0;
    }

    /// Avance le fondu et rend la teinte actuelle.
    pub fn update(&mut self, dt_s: f32) -> LinearRgba {
        if self.progress < 1.0 {
            self.progress = (self.progress + dt_s / GLOW_SECONDS).min(1.0);
            let eased = self.progress * self.progress * (3.0 - 2.0 * self.progress);
            self.current = self.from.mix(&self.target, eased);
        }
        self.current
    }

    /// La teinte actuelle.
    #[must_use]
    pub fn current(&self) -> LinearRgba {
        self.current
    }
}

/// Clignotant à commande par le volant : un seul côté à la fois, celui vers lequel le volant est tourné, jamais les
/// deux (`Blinker` de `cyber/blinker.ts`).
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct Blinker {
    /// −1 gauche, +1 droite, 0 éteint.
    pub side: i8,
    clock: f32,
    hold: f32,
}

/// Le volant tourné d'au moins ceci allume le clignotant de ce côté ; sous ce seuil il retombe (hystérésis).
const BLINK_ON: f32 = 0.2;
const BLINK_OFF: f32 = 0.08;
/// Il reste allumé au moins ce temps après que le volant se redresse, comme un vrai, s.
const BLINK_HOLD_S: f32 = 0.8;
/// Cycle du clignotant, s : 1,5 Hz, sous les 3 Hz du seuil photosensible (WCAG 2.3.1), allumé la première moitié.
pub const BLINK_PERIOD_S: f32 = 0.66;

impl Blinker {
    /// Avance d'une image avec le volant (`steer` : +1 à droite de la route).
    pub fn update(&mut self, dt_s: f32, steer: f32) {
        let dir: i8 = if steer < 0.0 { -1 } else { 1 };
        let amount = steer.abs();
        if amount > BLINK_ON && self.side != dir {
            self.side = dir;
            self.clock = 0.0;
        }
        if self.side != 0 {
            if amount > BLINK_OFF && dir == self.side {
                self.hold = BLINK_HOLD_S;
            } else {
                self.hold -= dt_s;
            }
            if self.hold <= 0.0 {
                self.side = 0;
            }
        }
        self.clock += dt_s;
    }

    /// Le clignotant est-il allumé à cet instant ? Premier tiers du cycle de 0,66 s du modèle (`time % 0.66 < 0.33`),
    /// remis à zéro quand le côté change : le premier battement est immédiat.
    #[must_use]
    pub fn lit(&self) -> bool {
        self.side != 0 && self.clock % BLINK_PERIOD_S < BLINK_PERIOD_S / 2.0
    }
}

/// Ce qui change d'une image à l'autre et règle la lumière.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct LightInputs {
    /// Teinte du moment (néons, lueur sous la caisse).
    pub glow: LinearRgba,
    /// Pouls du kick, 0 à 1.
    pub pulse: f32,
    /// Freinage, 0 à 1.
    pub brake: f32,
    /// Vitesse rapportée à la croisière.
    pub speed_ratio: f32,
    /// Niveau des phares : 0 éteints, [`HEADLIGHT_LEVEL`] normal, [`HIGH_BEAMS_LEVEL`] en feux de route.
    pub level: f32,
}

/// Une couleur et le coefficient d'opacité d'un volume ou d'une flaque additifs.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Tinted {
    /// Teinte (lumière linéaire, éventuellement au-delà de 1 : le bloom s'en empare).
    pub color: LinearRgba,
    /// Opacité (`uK`).
    pub k: f32,
}

/// Les valeurs de l'image pour les lumières de la voiture.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct LightLevels {
    /// Blanc des phares, tiré de 28 % vers la teinte du moment : « pour rester dans la direction artistique ».
    pub tint: LinearRgba,
    /// Les deux volumes de lumière des phares.
    pub beam: Tinted,
    /// La flaque de lumière devant la voiture.
    pub front_pool: Tinted,
    /// La flaque rouge derrière (au freinage).
    pub rear_pool: Tinted,
    /// Halo autour des phares (lumière, 0 = invisible).
    pub head_halo: LinearRgba,
    /// Halo autour des feux stop.
    pub tail_halo: LinearRgba,
    /// Lueur sous la caisse.
    pub under: LinearRgba,
    /// Longueur de la lueur sous la caisse, m : elle s'étire avec la vitesse.
    pub under_length_m: f32,
}

fn scaled(color: LinearRgba, k: f32) -> LinearRgba {
    LinearRgba::new(color.red * k, color.green * k, color.blue * k, 1.0)
}

impl LightLevels {
    /// Calcule les niveaux (`CarLights.update` + `CyberGlow.update`).
    #[must_use]
    pub fn compute(palette: &CarPalette, inputs: &LightInputs) -> Self {
        let LightInputs {
            glow,
            pulse,
            brake,
            speed_ratio,
            level,
        } = *inputs;
        let tint = palette.ice.mix(&glow, 0.28);
        let drive = 0.65 + speed_ratio.min(1.0) * 0.35;
        let braking = (brake * 1.4).min(1.0);
        Self {
            tint,
            beam: Tinted {
                color: tint,
                k: (0.17 + pulse * 0.04) * drive * level,
            },
            front_pool: Tinted {
                color: tint,
                k: (0.34 + pulse * 0.08) * drive * level,
            },
            rear_pool: Tinted {
                color: palette.alarm,
                k: (0.1 + brake * 0.4) * level,
            },
            head_halo: scaled(palette.ice, 0.45 + pulse * 0.15),
            // Veilleuse rouge en roulant, pleins feux au freinage.
            tail_halo: scaled(palette.alarm, 0.2 + braking * 1.3),
            // Lueur néon sous la caisse : elle bat avec la musique et s'étire avec la vitesse.
            under: scaled(glow, 0.4 + pulse * 0.3),
            under_length_m: 5.0 + speed_ratio.min(1.5) * 0.8,
        }
    }

    /// Facteur d'émissif du feu arrière (matériau `light_brake`) : de la veilleuse (0,18 phares allumés, 0,03 sinon) aux
    /// pleins feux (1) avec le freinage, **continu** : le freinage est lissé en amont (`Slew`), le feu ne saute pas.
    #[must_use]
    pub fn brake_light_factor(brake: f32, headlights_on: bool) -> f32 {
        let rest = if headlights_on { 0.18 } else { 0.03 };
        rest + (1.0 - rest) * brake.clamp(0.0, 1.0)
    }
}

/// Le logo arrière : un néon plus lumineux que les autres, qui réagit à la musique (`updateLogo` de `CyberCar.ts`).
///
/// Il ne suit plus directement les basses ni le pouls : le kick qu'il reçoit est le niveau **déjà borné par le limiteur**
/// (`CarFlash::kick`, règle 7), si bien qu'il ne monte jamais plus vite que lui. Relâchement ~0,25 s ; au repos, il
/// respire lentement (0,12 ± 0,1), sauf quand les flashs sont coupés : niveau fixe.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct RearLogo {
    level: f32,
    clock: f32,
}

/// Émissif de base du logo dans le GLB (`emissiveStrength`).
const LOGO_BASE_EMISSIVE: f32 = 10.0;
/// De combien la musique s'ajoute à la lueur de repos (`logoBoost`).
const LOGO_BOOST: f32 = 1.6;

impl RearLogo {
    /// Avance d'une image. `kick` : 0 à 1, borné par le limiteur ; `animated` : les flashs sont permis (la respiration
    /// de repos ne bat que dans ce cas).
    pub fn update(&mut self, dt_s: f32, kick: f32, animated: bool) {
        self.clock += dt_s;
        let idle = if animated {
            0.12 + 0.1 * (self.clock * 2.0).sin()
        } else {
            0.12
        };
        let target = idle.max(kick.clamp(0.0, 1.0) * 0.9);
        // Montée : celle du kick limité. Relâchement de 4 par seconde (~0,25 s).
        self.level = if target > self.level {
            target
        } else {
            target.max(self.level - dt_s * 4.0)
        };
    }

    /// Niveau lissé, 0 à 1.
    #[must_use]
    pub fn level(&self) -> f32 {
        self.level
    }

    /// Force de l'émissif du matériau `logo_rear_neon`.
    #[must_use]
    pub fn emissive_strength(&self) -> f32 {
        LOGO_BASE_EMISSIVE * (0.6 + LOGO_BOOST * self.level)
    }

    /// Échelle du logo : jusqu'à +5 % sur le niveau.
    #[must_use]
    pub fn scale(&self) -> f32 {
        1.0 + 0.05 * self.level
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn palette() -> CarPalette {
        CarPalette {
            violet: LinearRgba::rgb(0.14, 0.07, 0.54),
            rose: LinearRgba::rgb(1.0, 0.095, 0.033),
            ice: LinearRgba::rgb(0.79, 0.93, 1.0),
            alarm: LinearRgba::rgb(1.0, 0.01, 0.01),
            amber: LinearRgba::rgb(1.0, 0.5, 0.0),
        }
    }

    fn inputs(pulse: f32) -> LightInputs {
        LightInputs {
            glow: palette().violet,
            pulse,
            brake: 0.0,
            speed_ratio: 1.0,
            level: HEADLIGHT_LEVEL,
        }
    }

    /// Critère de la tâche : la lueur de la voiture pulse sur les kicks. Chaque lumière qui doit battre est plus forte
    /// au pic du kick qu'au creux.
    #[test]
    fn les_lumieres_pulsent_sur_les_kicks() {
        let calm = LightLevels::compute(&palette(), &inputs(0.0));
        let kick = LightLevels::compute(&palette(), &inputs(1.0));
        let luma = |c: LinearRgba| c.red + c.green + c.blue;
        assert!(kick.beam.k > calm.beam.k * 1.2, "faisceaux");
        assert!(kick.front_pool.k > calm.front_pool.k * 1.2, "flaque devant");
        assert!(luma(kick.head_halo) > luma(calm.head_halo) * 1.3, "halos des phares");
        assert!(luma(kick.under) > luma(calm.under) * 1.7, "lueur sous la caisse");
        // Les feux arrière, eux, ne suivent pas la musique : seul le freinage les allume.
        assert!((kick.tail_halo.red - calm.tail_halo.red).abs() < 1e-6);
    }

    #[test]
    fn le_freinage_allume_les_feux_arriere_et_la_flaque_rouge() {
        let rolling = LightLevels::compute(&palette(), &inputs(0.0));
        let braking = LightLevels::compute(
            &palette(),
            &LightInputs {
                brake: 1.0,
                ..inputs(0.0)
            },
        );
        assert!(braking.tail_halo.red > rolling.tail_halo.red * 5.0);
        assert!(braking.rear_pool.k > rolling.rear_pool.k * 3.0);
        assert!((LightLevels::brake_light_factor(0.0, true) - 0.18).abs() < 1e-6);
        assert!((LightLevels::brake_light_factor(1.0, true) - 1.0).abs() < 1e-6);
        let (a, b) = (
            LightLevels::brake_light_factor(0.04, true),
            LightLevels::brake_light_factor(0.06, true),
        );
        assert!((b - a).abs() < 0.03, "continu : pas de saut à 0,05");
        assert!((LightLevels::brake_light_factor(0.0, false) - 0.03).abs() < 1e-6);
    }

    #[test]
    fn la_lueur_sous_la_caisse_s_etire_avec_la_vitesse() {
        let slow = LightLevels::compute(
            &palette(),
            &LightInputs {
                speed_ratio: 0.0,
                ..inputs(0.0)
            },
        );
        let fast = LightLevels::compute(
            &palette(),
            &LightInputs {
                speed_ratio: 1.5,
                ..inputs(0.0)
            },
        );
        assert!((slow.under_length_m - 5.0).abs() < 1e-6 && (fast.under_length_m - 6.2).abs() < 1e-6);
        assert!(fast.beam.k > slow.beam.k, "les phares montent avec la vitesse");
    }

    #[test]
    fn la_teinte_migre_vers_la_cible_en_une_demi_seconde() {
        let mut fade = GlowFade::new(palette().violet);
        fade.set_target(palette().rose);
        let half = fade.update(GLOW_SECONDS / 2.0);
        assert!(
            half.red > palette().violet.red && half.red < palette().rose.red,
            "à mi-chemin"
        );
        let done = fade.update(GLOW_SECONDS);
        assert_eq!(done, palette().rose);
        // Redemander la même cible ne relance pas le fondu.
        fade.set_target(palette().rose);
        assert_eq!(fade.update(0.1), palette().rose);
    }

    #[test]
    fn le_clignotant_suit_le_volant_et_ne_s_allume_que_d_un_cote() {
        let mut blinker = Blinker::default();
        blinker.update(0.016, 0.0);
        assert_eq!(blinker.side, 0);
        assert!(!blinker.lit());
        blinker.update(0.016, 0.5);
        assert_eq!(blinker.side, 1, "droite");
        assert!(blinker.lit(), "premier battement immédiat");
        // Le volant se redresse : il reste allumé 0,8 s puis s'éteint.
        blinker.update(0.5, 0.0);
        assert_eq!(blinker.side, 1);
        blinker.update(0.5, 0.0);
        assert_eq!(blinker.side, 0);
        blinker.update(0.016, -0.5);
        assert_eq!(blinker.side, -1, "gauche");
    }

    #[test]
    fn le_clignotant_bat_sous_les_3_hz_photosensibles() {
        const { assert!(1.0 / BLINK_PERIOD_S < 3.0) };
        let mut blinker = Blinker::default();
        blinker.update(0.0, 1.0);
        let mut changes = 0;
        let mut last = blinker.lit();
        for _ in 0..600 {
            blinker.update(1.0 / 60.0, 1.0);
            if blinker.lit() != last {
                changes += 1;
                last = blinker.lit();
            }
        }
        // 10 s à 1,5 Hz : 15 allumages, soit ~30 changements d'état.
        assert!((28..=32).contains(&changes), "{changes} changements en 10 s");
    }

    #[test]
    fn le_logo_arriere_suit_le_kick_limite_et_relache_en_un_quart_de_seconde() {
        let mut logo = RearLogo::default();
        logo.update(1.0 / 60.0, 1.0, true);
        assert!(
            logo.level() >= 0.89,
            "un kick plein pousse le logo à 0,9 : {}",
            logo.level()
        );
        let peak = logo.emissive_strength();
        for _ in 0..30 {
            logo.update(1.0 / 60.0, 0.0, true);
        }
        assert!(logo.level() < 0.25, "relâché en 0,5 s : {}", logo.level());
        assert!(logo.emissive_strength() < peak * 0.6);
        assert!(logo.scale() < 1.02);
    }

    #[test]
    fn le_logo_respire_quand_les_flashs_sont_permis_et_se_fige_sinon() {
        let range = |animated: bool| {
            let mut logo = RearLogo::default();
            let mut levels = Vec::new();
            for _ in 0..240 {
                logo.update(1.0 / 60.0, 0.0, animated);
                levels.push(logo.level());
            }
            let (min, max) = levels
                .iter()
                .fold((1.0_f32, 0.0_f32), |(lo, hi), &l| (lo.min(l), hi.max(l)));
            (min, max)
        };
        let (min, max) = range(true);
        assert!(
            max > 0.18 && max <= 0.23 && min < max - 0.05,
            "il respire : {min}..{max}"
        );
        let (min, max) = range(false);
        assert!((max - min).abs() < 1e-6, "flashs coupés : niveau fixe ({min}..{max})");
    }
}
