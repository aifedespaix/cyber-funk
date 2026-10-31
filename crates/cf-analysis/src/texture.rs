//! La « texture » du son, au-delà des kicks : transitoires (caisse claire, claps), charleston, brillance, bruit.
//!
//! Le détecteur de kicks de Sillage ne voit que les basses. Un set de techno ou de house se joue aussi sur les
//! transitoires des médiums et des aigus, sur la brillance d'un filtre qui s'ouvre et sur les bruits blancs qui
//! montent avant un drop. Cette analyse lit le même spectre en octets que le détecteur (aucune FFT de plus) et publie
//! cinq grandeurs, toutes dans 0..1, qui retombent seules : le monde les traite comme des impacts distincts du kick.
//!
//! Pas de verrou ni d'allocation : l'état tient dans des tableaux de taille fixe (thread d'analyse, règle 3).

use crate::analyser_emulation::{BIN_COUNT, FFT_SIZE};

/// Ce que l'analyse de texture publie à chaque pas.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct TextureState {
    /// Force des attaques en cours (part du spectre qui vient de monter), lissée, 0..1.
    pub flux: f32,
    /// Impulsion après un transitoire large bande (caisse claire, clap, percussion), 1 → 0.
    pub onset: f32,
    /// Impulsion après un transitoire dans les aigus (charleston, shaker), 1 → 0.
    pub hat: f32,
    /// Brillance : centre de gravité du spectre, 0 (sourd) → 1 (cristallin), lissé.
    pub brightness: f32,
    /// Bruit : platitude du spectre, 0 (note tenue, basse) → 1 (souffle, montée de bruit blanc), lissée.
    pub noisiness: f32,
}

/// Fréquence (Hz) au-dessus de laquelle un transitoire compte comme charleston.
const HAT_FROM_HZ: f64 = 6_000.0;
/// Bornes (Hz) de la brillance, sur une échelle logarithmique.
const CENTROID_LOW_HZ: f64 = 150.0;
const CENTROID_HIGH_HZ: f64 = 8_000.0;
/// Bornes (Hz) de la platitude : en dessous, les basses dominent toujours ; au-dessus, il n'y a plus de musique.
const FLAT_LOW_HZ: f64 = 250.0;
const FLAT_HIGH_HZ: f64 = 12_000.0;
/// Constantes de temps (s) : retombée des impulsions, moyenne glissante du seuil, lissage des grandeurs lentes.
const ONSET_DECAY_S: f32 = 0.20;
const HAT_DECAY_S: f32 = 0.12;
const THRESHOLD_AVG_S: f32 = 0.6;
const SLOW_SMOOTH_S: f32 = 0.18;
const FLUX_SMOOTH_S: f32 = 0.05;
/// Délai minimal (s) entre deux transitoires de la même famille : un coup ne compte qu'une fois.
const REFRACTORY_S: f32 = 0.09;
/// Un transitoire dépasse la moyenne de ce facteur, et d'au moins `FLOOR` (sinon le souffle d'un silence déclenche).
const THRESHOLD_FACTOR: f32 = 1.7;
const THRESHOLD_FLOOR: f32 = 0.035;

/// L'état de l'analyse : le spectre précédent et les moyennes glissantes.
#[derive(Debug, Clone)]
pub struct TextureAnalyzer {
    previous: [u8; BIN_COUNT],
    state: TextureState,
    flux_avg: f32,
    hat_flux_avg: f32,
    onset_cooldown_s: f32,
    hat_cooldown_s: f32,
    primed: bool,
}

impl Default for TextureAnalyzer {
    fn default() -> Self {
        Self::new()
    }
}

fn bin_of(hz: f64, sample_rate: u32) -> usize {
    ((hz * FFT_SIZE as f64 / f64::from(sample_rate.max(1))) as usize).min(BIN_COUNT)
}

/// Coefficient de lissage exponentiel pour un pas `dt_s` et une constante de temps `tau_s`.
fn alpha(dt_s: f32, tau_s: f32) -> f32 {
    1.0 - (-dt_s / tau_s).exp()
}

impl TextureAnalyzer {
    /// Analyse neuve : aucune mémoire du spectre précédent.
    #[must_use]
    pub fn new() -> Self {
        Self {
            previous: [0; BIN_COUNT],
            state: TextureState::default(),
            flux_avg: 0.0,
            hat_flux_avg: 0.0,
            onset_cooldown_s: 0.0,
            hat_cooldown_s: 0.0,
            primed: false,
        }
    }

    /// Oublie tout (nouvelle source ou nouvelle piste).
    pub fn reset(&mut self) {
        *self = Self::new();
    }

    /// Dernier état publié.
    #[must_use]
    pub fn state(&self) -> &TextureState {
        &self.state
    }

    /// Signal interrompu : les impulsions et les grandeurs retombent, la moyenne du seuil est gardée.
    pub fn rest(&mut self) {
        self.state = TextureState::default();
        self.previous = [0; BIN_COUNT];
        self.primed = false;
    }

    /// Avance d'un pas sur `spectrum` (octets de l'`AnalyserNode`, 0..255 par bande), `dt_s` secondes plus tard.
    pub fn update(&mut self, spectrum: &[u8; BIN_COUNT], sample_rate: u32, dt_s: f32) -> &TextureState {
        let dt_s = dt_s.clamp(0.0, 0.25);
        let hat_from = bin_of(HAT_FROM_HZ, sample_rate).min(BIN_COUNT - 1);
        let (mut rise, mut total, mut hat_rise, mut hat_total) = (0.0_f32, 0.0_f32, 0.0_f32, 0.0_f32);
        for (i, (&now, &before)) in spectrum.iter().zip(self.previous.iter()).enumerate() {
            let (now, before) = (f32::from(now), f32::from(before));
            let up = (now - before).max(0.0);
            rise += up;
            total += now;
            if i >= hat_from {
                hat_rise += up;
                hat_total += now;
            }
        }
        // Part de l'énergie qui vient de monter : indépendante du niveau, donc du gain de la source.
        let (flux, hat_flux) = if self.primed {
            (rise / total.max(255.0), hat_rise / hat_total.max(255.0))
        } else {
            (0.0, 0.0)
        };
        self.previous.copy_from_slice(spectrum);
        self.primed = true;

        let a_avg = alpha(dt_s, THRESHOLD_AVG_S);
        self.flux_avg += (flux - self.flux_avg) * a_avg;
        self.hat_flux_avg += (hat_flux - self.hat_flux_avg) * a_avg;
        self.onset_cooldown_s = (self.onset_cooldown_s - dt_s).max(0.0);
        self.hat_cooldown_s = (self.hat_cooldown_s - dt_s).max(0.0);

        let s = &mut self.state;
        s.onset *= (-dt_s / ONSET_DECAY_S).exp();
        s.hat *= (-dt_s / HAT_DECAY_S).exp();
        if self.onset_cooldown_s <= 0.0 && flux > self.flux_avg * THRESHOLD_FACTOR + THRESHOLD_FLOOR {
            s.onset = 1.0;
            self.onset_cooldown_s = REFRACTORY_S;
        }
        if self.hat_cooldown_s <= 0.0 && hat_flux > self.hat_flux_avg * THRESHOLD_FACTOR + THRESHOLD_FLOOR {
            s.hat = 1.0;
            self.hat_cooldown_s = REFRACTORY_S;
        }
        s.flux += (flux.clamp(0.0, 1.0) - s.flux) * alpha(dt_s, FLUX_SMOOTH_S);

        let (brightness, noisiness) = shape(spectrum, sample_rate);
        let a_slow = alpha(dt_s, SLOW_SMOOTH_S);
        s.brightness += (brightness - s.brightness) * a_slow;
        s.noisiness += (noisiness - s.noisiness) * a_slow;
        &self.state
    }
}

/// Brillance (centre de gravité, échelle log) et platitude (moyenne géométrique / arithmétique) du spectre.
fn shape(spectrum: &[u8; BIN_COUNT], sample_rate: u32) -> (f32, f32) {
    let bin_hz = f64::from(sample_rate.max(1)) / FFT_SIZE as f64;
    let first = bin_of(CENTROID_LOW_HZ, sample_rate).max(1);
    let (mut weighted, mut mass) = (0.0_f64, 0.0_f64);
    for (i, &m) in spectrum.iter().enumerate().skip(first) {
        let m = f64::from(m);
        weighted += m * (i as f64 * bin_hz).log2();
        mass += m;
    }
    let brightness = if mass > 255.0 {
        let (lo, hi) = (CENTROID_LOW_HZ.log2(), CENTROID_HIGH_HZ.log2());
        (((weighted / mass) - lo) / (hi - lo)).clamp(0.0, 1.0) as f32
    } else {
        0.0
    };
    let (from, to) = (
        bin_of(FLAT_LOW_HZ, sample_rate).max(1),
        bin_of(FLAT_HIGH_HZ, sample_rate),
    );
    let (mut log_sum, mut sum, mut n) = (0.0_f64, 0.0_f64, 0.0_f64);
    for &m in spectrum.iter().take(to).skip(from) {
        let m = f64::from(m) + 1.0;
        log_sum += m.ln();
        sum += m;
        n += 1.0;
    }
    let noisiness = if n > 0.0 && sum > n * 40.0 {
        ((log_sum / n).exp() / (sum / n)).clamp(0.0, 1.0) as f32
    } else {
        0.0
    };
    (brightness, noisiness)
}

#[cfg(test)]
mod tests {
    use super::*;

    const SR: u32 = 48_000;
    const DT: f32 = 512.0 / 48_000.0;

    fn silence() -> [u8; BIN_COUNT] {
        [0; BIN_COUNT]
    }

    /// Un son tenu : même spectre à chaque pas.
    fn steady(level: u8) -> [u8; BIN_COUNT] {
        let mut s = silence();
        for (i, v) in s.iter_mut().enumerate().take(200) {
            *v = level.saturating_sub((i / 2) as u8);
        }
        s
    }

    fn settle(a: &mut TextureAnalyzer, spectrum: &[u8; BIN_COUNT], steps: usize) {
        for _ in 0..steps {
            a.update(spectrum, SR, DT);
        }
    }

    #[test]
    fn a_steady_tone_triggers_nothing() {
        let mut a = TextureAnalyzer::new();
        settle(&mut a, &steady(180), 200);
        let s = a.state();
        assert!(s.onset < 0.01 && s.hat < 0.01, "{s:?}");
    }

    #[test]
    fn a_broadband_attack_triggers_an_onset_that_decays() {
        let mut a = TextureAnalyzer::new();
        settle(&mut a, &steady(100), 200);
        let mut hit = steady(100);
        for v in hit.iter_mut().take(600) {
            *v = v.saturating_add(120);
        }
        assert!(a.update(&hit, SR, DT).onset > 0.99);
        settle(&mut a, &hit, 100);
        assert!(a.state().onset < 0.05, "{:?}", a.state());
    }

    #[test]
    fn a_hat_only_fires_on_highs() {
        let mut a = TextureAnalyzer::new();
        settle(&mut a, &steady(100), 200);
        let mut hat = steady(100);
        for v in hat.iter_mut().skip(300).take(600) {
            *v = 200;
        }
        assert!(a.update(&hat, SR, DT).hat > 0.99);
        let mut bass = steady(100);
        for v in bass.iter_mut().take(20) {
            *v = 255;
        }
        let mut b = TextureAnalyzer::new();
        settle(&mut b, &steady(100), 200);
        assert!(b.update(&bass, SR, DT).hat < 0.5);
    }

    #[test]
    fn brightness_follows_the_spectrum_and_noise_follows_flatness() {
        let mut dull = TextureAnalyzer::new();
        settle(&mut dull, &steady(200), 100);
        let mut bright = TextureAnalyzer::new();
        let mut high = silence();
        for v in high.iter_mut().skip(200).take(400) {
            *v = 200;
        }
        settle(&mut bright, &high, 100);
        assert!(bright.state().brightness > dull.state().brightness + 0.2);

        let mut noise = TextureAnalyzer::new();
        settle(&mut noise, &[150; BIN_COUNT], 100);
        assert!(noise.state().noisiness > 0.9, "{:?}", noise.state());
        assert!(dull.state().noisiness < 0.5, "{:?}", dull.state());
    }

    #[test]
    fn silence_and_rest_are_neutral() {
        let mut a = TextureAnalyzer::new();
        settle(&mut a, &silence(), 50);
        assert_eq!(*a.state(), TextureState::default());
        settle(&mut a, &steady(200), 50);
        a.rest();
        assert_eq!(*a.state(), TextureState::default());
    }
}
