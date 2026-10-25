//! La musique telle que la voit une enseigne (`SignAudio` de `types.ts`, `listen` de `sign-manager.ts`), recalculée une
//! fois par image.

use cf_analysis::AudioFeatures;

/// Front montant des aigus qui compte pour un coup de charleston ou de caisse claire.
const SNARE_RISE: f32 = 0.1;
/// Période réfractaire entre deux coups de charleston, en s.
const SNARE_REFRACTORY_S: f64 = 0.11;

/// Ce qu'une enseigne lit de la musique et de la voiture.
#[derive(Debug, Clone, Copy, PartialEq)]
#[allow(
    clippy::struct_excessive_bools,
    reason = "un sac de faits de l'image : kick, charleston, drop, calme"
)]
pub struct SignAudio {
    /// Horloge de la scène (s).
    pub time: f64,
    /// Pas de l'image (s).
    pub dt: f32,
    /// Un kick vient de frapper (une image).
    pub kick: bool,
    /// Énergie des basses, 0..1.
    pub bass: f32,
    /// Énergie des médiums, 0..1.
    pub mid: f32,
    /// Énergie des aigus, 0..1.
    pub high: f32,
    /// Pouls du dernier kick, 1 → 0. Brut : toute lumière qu'on en tire passe par le limiteur.
    pub pulse: f32,
    /// Vrai une image sur un coup de charleston ou de caisse claire (front montant des aigus).
    pub snare: bool,
    /// Un drop frappe maintenant.
    pub drop: bool,
    /// Mode « réduire les animations » : pas de stroboscope (respiration du spectacle).
    pub calm: bool,
    /// Vitesse de la voiture au compteur, en km/h.
    pub kmh: f32,
}

impl Default for SignAudio {
    fn default() -> Self {
        Self {
            time: 0.0,
            dt: 0.0,
            kick: false,
            bass: 0.0,
            mid: 0.0,
            high: 0.0,
            pulse: 0.0,
            snare: false,
            drop: false,
            calm: false,
            kmh: 0.0,
        }
    }
}

/// La mémoire qui sert à repérer les fronts (`previousHigh`, `lastSnare`).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SignListener {
    previous_high: f32,
    last_snare: f64,
}

impl Default for SignListener {
    fn default() -> Self {
        Self {
            previous_high: 0.0,
            last_snare: f64::NEG_INFINITY,
        }
    }
}

impl SignListener {
    /// Recompose l'audio de l'image (`listen`).
    pub fn listen(
        &mut self,
        time: f64,
        dt: f32,
        features: &AudioFeatures,
        drop: bool,
        calm: bool,
        kmh: f32,
    ) -> SignAudio {
        let snare = features.high - self.previous_high > SNARE_RISE && time - self.last_snare > SNARE_REFRACTORY_S;
        if snare {
            self.last_snare = time;
        }
        self.previous_high = features.high;
        SignAudio {
            time,
            dt,
            kick: features.kick,
            bass: features.bass,
            mid: features.mid,
            high: features.high,
            pulse: features.pulse,
            snare,
            drop,
            calm,
            kmh,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn le_charleston_est_un_front_montant_avec_une_periode_refractaire() {
        let mut l = SignListener::default();
        let mut f = AudioFeatures::NEUTRAL;
        let at = |l: &mut SignListener, f: &AudioFeatures, t: f64| l.listen(t, 0.016, f, false, false, 0.0).snare;
        f.high = 0.05;
        assert!(!at(&mut l, &f, 1.0));
        f.high = 0.4;
        assert!(at(&mut l, &f, 1.016), "front montant");
        f.high = 0.9;
        assert!(!at(&mut l, &f, 1.05), "période réfractaire");
        f.high = 0.1;
        let _ = at(&mut l, &f, 1.2);
        f.high = 0.5;
        assert!(at(&mut l, &f, 1.3));
    }
}
