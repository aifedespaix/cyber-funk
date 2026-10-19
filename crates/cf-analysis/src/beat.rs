//! Détection de rythme en temps réel : portage à l'identique de `BeatDetector` (`engine/beat.ts` de Sillage).
//!
//! Principe : on suit le flux d'énergie positif dans la bande des basses (kick). Un coup est retenu quand le flux
//! dépasse sa moyenne glissante d'un facteur, avec une période réfractaire. Le tempo vient de la médiane des
//! intervalles entre kicks, repliée dans 70–180 BPM.
//!
//! Le calcul est en `f64` partout, comme les nombres de JavaScript : les comparaisons à seuil tombent du même côté que
//! dans Sillage. L'algorithme n'est **pas** corrigé : là où `beat.ts` se trompe (voir la PR de J1-04), le portage se
//! trompe de la même façon, c'est le but du golden.

/// Mémoire (s) de la moyenne glissante du flux : indépendante de la fréquence d'images.
const FLUX_MEMORY: f64 = 0.7;
/// Seuil absolu du flux, en unités par seconde (0,05 par image à 60 i/s).
const MIN_FLUX_RATE: f64 = 3.0;
/// Rapport du flux à sa moyenne glissante au-delà duquel un kick est retenu.
const FLUX_RATIO: f64 = 1.55;
/// Plus petit pas de temps pris en compte (s) : évite une division par un dt nul.
const MIN_DT: f64 = 1.0 / 240.0;
/// Délai (s) pendant lequel un kick ne peut pas être suivi d'un autre.
const REFRACTORY: f64 = 0.24;
/// Intervalle minimal entre deux kicks pour compter dans le tempo (s) : ~214 BPM.
const MIN_INTERVAL: f64 = 0.28;
/// Intervalle maximal entre deux kicks pour compter dans le tempo (s) : 50 BPM.
const MAX_INTERVAL: f64 = 1.2;
/// Nombre d'intervalles gardés pour la médiane.
const TEMPO_WINDOW: usize = 12;
/// Nombre minimal d'intervalles avant de publier un tempo.
const MIN_INTERVALS: usize = 4;
/// Silence (s) entre deux kicks au-delà duquel l'historique de tempo est oublié.
const STALE_AFTER: f64 = 8.0;
/// Plage de repli du tempo (BPM).
const MIN_BPM: f64 = 70.0;
const MAX_BPM: f64 = 180.0;
// Le spectre s'affaiblit avec la fréquence : on rééquilibre pour viser 0–1.
const MID_GAIN: f64 = 1.7;
const HIGH_GAIN: f64 = 4.5;
/// Bornes (Hz) des trois bandes : basses (qui portent aussi la détection des kicks), médiums, aigus.
const BASS_HZ: (f64, f64) = (40.0, 150.0);
const MID_HZ: (f64, f64) = (300.0, 2500.0);
const HIGH_HZ: (f64, f64) = (5000.0, 12000.0);
/// Décroissance de `pulse` par seconde après un kick.
const PULSE_DECAY: f64 = 4.5;

/// Ce que `BeatDetector` publie à chaque pas (`BeatState` de Sillage).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct BeatState {
    /// Vrai pendant un seul pas, au moment d'un kick.
    pub kick: bool,
    /// Énergie des basses, lissée, 0..1.
    pub bass: f64,
    /// Énergie des médiums (voix, synthés), lissée, 0..1.
    pub mid: f64,
    /// Énergie des aigus (charleston, brillance), lissée, 0..1.
    pub high: f64,
    /// Volume global pondéré (basses 50 %, médiums 35 %, aigus 15 %), 0..1 : la mesure commune des sections et de
    /// l'allure.
    pub loud: f64,
    /// Impulsion décroissante après chaque kick, 1 → 0.
    pub pulse: f64,
    /// Nombre de kicks depuis le début (Sillage ne le remet jamais à zéro, pas même à `reset`).
    pub count: u32,
    /// Compteur continu : `count` plus une montée douce 0 → 1 pendant la décroissance de `pulse`. Les effets qui
    /// avancent « d'un cran par temps » s'y accrochent sans à-coup visible.
    pub beat: f64,
    /// Tempo (BPM, arrondi à l'entier comme Sillage), `None` tant qu'il n'est pas établi.
    pub bpm: Option<f32>,
}

impl BeatState {
    /// État de départ : tout à zéro, pas de tempo.
    pub const NEW: Self = Self {
        kick: false,
        bass: 0.0,
        mid: 0.0,
        high: 0.0,
        loud: 0.0,
        pulse: 0.0,
        count: 0,
        beat: 0.0,
        bpm: None,
    };
}

impl Default for BeatState {
    fn default() -> Self {
        Self::NEW
    }
}

/// Bande de fréquences suivie pour le flux.
#[derive(Debug, Clone, Copy)]
struct Band {
    previous: f64,
    /// Moyenne glissante du flux positif, en unités par seconde.
    mean: f64,
    last_hit: f64,
}

/// Intervalles entre kicks : fenêtre glissante de [`TEMPO_WINDOW`] valeurs, sur la pile (aucune allocation).
#[derive(Debug, Clone, Copy)]
struct Intervals {
    values: [f64; TEMPO_WINDOW],
    len: usize,
}

impl Intervals {
    const fn new() -> Self {
        Self {
            values: [0.0; TEMPO_WINDOW],
            len: 0,
        }
    }

    /// `push` puis `shift` si la fenêtre déborde.
    fn push(&mut self, value: f64) {
        if self.len == TEMPO_WINDOW {
            self.values.copy_within(1.., 0);
            self.len -= 1;
        }
        self.values[self.len] = value;
        self.len += 1;
    }

    /// Médiane haute (`sorted[floor(len / 2)]`), calculée sur une copie triée sur la pile.
    fn median(&self) -> f64 {
        let mut sorted = self.values;
        let live = &mut sorted[..self.len];
        live.sort_unstable_by(f64::total_cmp);
        live[self.len / 2]
    }
}

/// Plages de bins, calculées une seule fois à partir de la fréquence d'échantillonnage et de `fftSize`.
#[derive(Debug, Clone, Copy)]
struct Ranges {
    bass: (usize, usize),
    mid: (usize, usize),
    high: (usize, usize),
}

/// Détecteur de kicks et de tempo (`BeatDetector` de `beat.ts`).
#[derive(Debug, Clone)]
pub struct BeatDetector {
    state: BeatState,
    bass: Band,
    intervals: Intervals,
    last_kick_time: Option<f64>,
    ranges: Option<Ranges>,
}

impl Default for BeatDetector {
    fn default() -> Self {
        Self::new()
    }
}

impl BeatDetector {
    /// Détecteur neuf.
    #[must_use]
    pub fn new() -> Self {
        Self {
            state: BeatState::NEW,
            bass: Band {
                previous: 0.0,
                mean: 0.0,
                last_hit: f64::NEG_INFINITY,
            },
            intervals: Intervals::new(),
            last_kick_time: None,
            ranges: None,
        }
    }

    /// État courant (celui que renvoie le dernier [`BeatDetector::update`]).
    #[must_use]
    pub fn state(&self) -> &BeatState {
        &self.state
    }

    /// Un pas. `spectrum` : spectre 0..255 de l'analyseur (`None` : pas de signal, les énergies retombent) ;
    /// `now` et `dt` en secondes. Aucune allocation.
    pub fn update(&mut self, spectrum: Option<&[u8]>, sample_rate: u32, fft_size: usize, now: f64, dt: f64) -> &BeatState {
        self.state.kick = false;
        self.state.pulse = (self.state.pulse - dt * PULSE_DECAY).max(0.0);

        let Some(spectrum) = spectrum else {
            let decay = (-dt * 3.0).exp();
            self.state.bass *= decay;
            self.state.mid *= decay;
            self.state.high *= decay;
            self.state.loud = loudness(&self.state);
            self.state.beat = beat_position(&self.state);
            return &self.state;
        };

        let ranges = *self.ranges.get_or_insert_with(|| {
            compute_ranges(f64::from(sample_rate), fft_size, spectrum.len())
        });

        let bass_now = band_average(spectrum, ranges.bass);
        let s = &mut self.state;
        s.bass = smooth(s.bass, bass_now, dt, 0.04, 0.25);
        s.mid = smooth(s.mid, (band_average(spectrum, ranges.mid) * MID_GAIN).min(1.0), dt, 0.05, 0.2);
        s.high = smooth(s.high, (band_average(spectrum, ranges.high) * HIGH_GAIN).min(1.0), dt, 0.03, 0.12);
        s.loud = loudness(s);

        if self.detect(bass_now, now, dt) {
            self.state.kick = true;
            self.state.pulse = 1.0;
            self.state.count += 1;
            self.register_kick(now);
        }
        self.state.beat = beat_position(&self.state);
        &self.state
    }

    /// Signal interrompu (perte de périphérique, pause) : les énergies instantanées tombent à zéro d'un coup. Le
    /// tempo, le compteur et la mémoire des kicks restent, comme les sections ; seul `update(None, …)` de Sillage
    /// fait décroître les énergies image par image, ce qu'on n'a pas ici (un seul appel à l'arrêt du signal).
    pub fn rest(&mut self) {
        let s = &mut self.state;
        s.kick = false;
        s.bass = 0.0;
        s.mid = 0.0;
        s.high = 0.0;
        s.loud = 0.0;
        s.pulse = 0.0;
        s.beat = beat_position(s);
    }

    /// Vrai si le flux positif des basses dépasse sa moyenne glissante. Le flux est ramené à une vitesse (unités par
    /// seconde) : à 30 comme à 144 images/s, un même kick franchit le même seuil.
    fn detect(&mut self, level: f64, now: f64, dt: f64) -> bool {
        let band = &mut self.bass;
        let step = dt.max(MIN_DT);
        let rate = (level - band.previous).max(0.0) / step;
        band.previous = level;

        let mean = band.mean;
        band.mean += (rate - mean) * (1.0 - (-step / FLUX_MEMORY).exp());

        let hit = rate > mean * FLUX_RATIO && rate > MIN_FLUX_RATE && now - band.last_hit > REFRACTORY;
        if hit {
            band.last_hit = now;
        }
        hit
    }

    fn register_kick(&mut self, now: f64) {
        let Some(last) = self.last_kick_time.replace(now) else {
            return;
        };
        let interval = now - last;
        // Longue coupure (changement de DJ) : les intervalles de l'ancien tempo ne pèsent plus sur la médiane du nouveau.
        if interval > STALE_AFTER {
            self.intervals.len = 0;
        }
        if !(MIN_INTERVAL..=MAX_INTERVAL).contains(&interval) {
            return;
        }
        self.intervals.push(interval);
        if self.intervals.len < MIN_INTERVALS {
            return;
        }

        let mut bpm = 60.0 / self.intervals.median();
        while bpm < MIN_BPM {
            bpm *= 2.0;
        }
        while bpm > MAX_BPM {
            bpm /= 2.0;
        }
        self.state.bpm = Some(bpm.round() as f32);
    }

    /// Repart de zéro quand on change de piste : évite un faux tempo. Comme dans Sillage, seuls le tempo et la mémoire
    /// des intervalles sont oubliés (ni `count`, ni les énergies, ni le flux).
    pub fn reset(&mut self) {
        self.intervals.len = 0;
        self.last_kick_time = None;
        self.state.bpm = None;
    }
}

/// Convertit les bornes de fréquence en indices de bins.
fn compute_ranges(sample_rate: f64, fft_size: usize, bin_count: usize) -> Ranges {
    let hz = sample_rate / fft_size as f64;
    let to_bin = |f: f64| ((f / hz).round() as usize).min(bin_count.saturating_sub(1));
    let range = |(from, to): (f64, f64)| (to_bin(from), to_bin(to));
    Ranges {
        bass: range(BASS_HZ),
        mid: range(MID_HZ),
        high: range(HIGH_HZ),
    }
}

fn band_average(spectrum: &[u8], (lo, hi): (usize, usize)) -> f64 {
    let sum: f64 = (lo..=hi).map(|i| f64::from(spectrum.get(i).copied().unwrap_or(0))).sum();
    sum / (hi + 1).saturating_sub(lo).max(1) as f64 / 255.0
}

fn smooth(current: f64, target: f64, dt: f64, attack: f64, release: f64) -> f64 {
    let tau = if target > current { attack } else { release };
    current + (target - current) * (1.0 - (-dt / tau).exp())
}

/// Volume global : la même pondération pour les sections du morceau et pour l'allure.
fn loudness(s: &BeatState) -> f64 {
    s.bass * 0.5 + s.mid * 0.35 + s.high * 0.15
}

fn beat_position(s: &BeatState) -> f64 {
    let t = 1.0 - s.pulse;
    f64::from(s.count) + t * t * (3.0 - 2.0 * t)
}

#[cfg(test)]
mod tests {
    use super::*;

    const SR: u32 = 48_000;
    const FFT: usize = 2048;

    /// Spectre plat de `level` octets partout.
    fn flat(level: u8) -> Vec<u8> {
        vec![level; 1024]
    }

    /// Fait battre le détecteur à `bpm` pendant `seconds`, au pas `dt` : un spectre fort pendant `hit_s` à chaque temps.
    fn drive(det: &mut BeatDetector, bpm: f64, seconds: f64, dt: f64, start: f64) -> Vec<f64> {
        let period = 60.0 / bpm;
        let mut kicks = Vec::new();
        let steps = (seconds / dt) as usize;
        for i in 1..=steps {
            let now = start + i as f64 * dt;
            let phase = (now - start) % period;
            let spectrum = if phase < 0.05 { flat(200) } else { flat(20) };
            if det.update(Some(&spectrum), SR, FFT, now, dt).kick {
                kicks.push(now);
            }
        }
        kicks
    }

    #[test]
    fn bands_cover_the_sillage_bins() {
        let r = compute_ranges(48_000.0, 2048, 1024);
        assert_eq!(r.bass, (2, 6));
        assert_eq!(r.mid, (13, 107));
        assert_eq!(r.high, (213, 512));
    }

    #[test]
    fn steady_pulse_gives_tempo_and_kicks() {
        let mut det = BeatDetector::new();
        let kicks = drive(&mut det, 128.0, 20.0, 1.0 / 60.0, 0.0);
        assert!(kicks.len() >= 30, "{} kicks", kicks.len());
        let bpm = det.state().bpm.unwrap_or_default();
        assert!((bpm - 128.0).abs() <= 3.0, "bpm {bpm}");
        assert_eq!(det.state().count as usize, kicks.len());
    }

    #[test]
    fn tempo_does_not_depend_on_step_size() {
        for dt in [1.0 / 30.0, 1.0 / 60.0, 512.0 / 48_000.0, 1.0 / 144.0] {
            let mut det = BeatDetector::new();
            drive(&mut det, 120.0, 20.0, dt, 0.0);
            let bpm = det.state().bpm.unwrap_or_default();
            assert!((bpm - 120.0).abs() <= 4.0, "dt {dt} : bpm {bpm}");
        }
    }

    #[test]
    fn refractory_period_blocks_double_hits() {
        let mut det = BeatDetector::new();
        let kicks = drive(&mut det, 200.0, 10.0, 1.0 / 60.0, 0.0);
        assert!(kicks.windows(2).all(|w| w[1] - w[0] > REFRACTORY));
    }

    #[test]
    fn long_silence_forgets_the_tempo_history() {
        let mut det = BeatDetector::new();
        drive(&mut det, 150.0, 10.0, 1.0 / 60.0, 0.0);
        assert!(det.state().bpm.is_some());
        // Silence de 12 s, puis un autre tempo : la médiane ne mélange pas les deux.
        for i in 0..720 {
            det.update(Some(&flat(0)), SR, FFT, 10.0 + f64::from(i) / 60.0, 1.0 / 60.0);
        }
        drive(&mut det, 90.0, 20.0, 1.0 / 60.0, 22.0);
        let bpm = det.state().bpm.unwrap_or_default();
        assert!((bpm - 90.0).abs() <= 4.0, "bpm {bpm}");
    }

    #[test]
    fn reset_clears_tempo_only() {
        let mut det = BeatDetector::new();
        drive(&mut det, 128.0, 10.0, 1.0 / 60.0, 0.0);
        let count = det.state().count;
        assert!(count > 0 && det.state().bpm.is_some());
        det.reset();
        assert_eq!(det.state().bpm, None);
        assert_eq!(det.state().count, count);
    }

    #[test]
    fn missing_spectrum_decays_energies() {
        let mut det = BeatDetector::new();
        det.update(Some(&flat(200)), SR, FFT, 0.1, 0.1);
        let before = det.state().bass;
        assert!(before > 0.0);
        det.update(None, SR, FFT, 0.2, 0.1);
        assert!(det.state().bass < before && !det.state().kick);
        det.rest();
        assert!(det.state().bass.abs() < f64::EPSILON && det.state().loud.abs() < f64::EPSILON);
    }

    #[test]
    fn intervals_window_keeps_the_last_twelve() {
        let mut w = Intervals::new();
        for i in 0..20 {
            w.push(f64::from(i));
        }
        assert_eq!(w.len, TEMPO_WINDOW);
        assert!((w.values[0] - 8.0).abs() < f64::EPSILON && (w.values[11] - 19.0).abs() < f64::EPSILON);
        assert!((w.median() - 14.0).abs() < f64::EPSILON);
    }

    #[test]
    fn beat_position_is_continuous_between_kicks() {
        let s = BeatState { count: 5, pulse: 1.0, ..BeatState::NEW };
        assert!((beat_position(&s) - 5.0).abs() < 1e-12);
        let s = BeatState { count: 5, pulse: 0.0, ..BeatState::NEW };
        assert!((beat_position(&s) - 6.0).abs() < 1e-12);
    }
}
