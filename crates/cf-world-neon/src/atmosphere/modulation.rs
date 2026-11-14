//! La brume réagit au son (P2 §1.3, table « modulation audio ») : densité de fond, anisotropie, bouffée de drop, apnée,
//! vent. Logique pure, sans Bevy : un état de taille fixe, avancé une fois par image.
//!
//! La bouffée de drop **[L]** épaissit la brume, donc l'éclaircit : c'est une variation de luminance. Elle passe par un
//! [`FlashLimiter`] (voie lente `Sweep`) qui lui appartient : cadence, écart, plafond et pentes sont les siens (règle 7).

use cf_analysis::filters::{AttackRelease, alpha};
use cf_show::limiter::{Channel, FlashLimiter, FlashRequest, MAX_SWEEP_PEAK};

use super::formula::fog_reach;

/// Réglages de la brume (valeurs de départ de P2 §1.3 et de DA §1.2).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct FogTuning {
    /// `σ_t·ρ_look` au sol par temps sec, 1/m. 0,0037 donne `1 − T = 0,2` à 60 m et `0,89` à 600 m (plans de DA §1.2).
    pub sigma_rho: f32,
    /// Hauteur d'échelle `H_f`, m.
    pub height_m: f32,
    /// Ce que l'averse ajoute à la densité (×(1 + rain_gain·pluie)).
    pub rain_gain: f32,
    /// Ce que l'humidité ajoute à la densité.
    pub wet_gain: f32,
    /// Anisotropie au calme et sur un drop.
    pub g_range: (f32, f32),
    /// Pic de la bouffée de drop (facteur de densité au sommet de l'enveloppe).
    pub puff_peak: f32,
    /// Densité pendant l'apnée, en part de `ρ_look`.
    pub apnea: f32,
    /// Vitesse du vent de la brume au repos, m/s.
    pub wind_mps: f32,
    /// Constante de temps (s) de la densité de fond : la salle s'épaissit lentement.
    pub density_tau_s: f32,
}

impl Default for FogTuning {
    fn default() -> Self {
        Self {
            sigma_rho: 0.0037,
            height_m: 25.0,
            rain_gain: 0.45,
            wet_gain: 0.2,
            g_range: (0.55, 0.85),
            puff_peak: 1.8,
            apnea: 0.3,
            wind_mps: 1.2,
            density_tau_s: 1.5,
        }
    }
}

/// Ce que la brume lit du monde à chaque image.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct FogInput {
    /// Pas de temps, s.
    pub dt_s: f32,
    /// Horloge du set, s (celle du limiteur).
    pub now_s: f64,
    /// Énergie lente `E_s`, 0..1 (l'intensité de l'analyse).
    pub energy_slow: f32,
    /// Énergie rapide `E_f`, 0..1 (le niveau instantané).
    pub energy_fast: f32,
    /// Enveloppe du bas-médium, 0..1 : les nappes poussent le vent.
    pub low_mid: f32,
    /// Averse, 0..1.
    pub rain: f32,
    /// Humidité, 0..1.
    pub wet: f32,
    /// Un drop est en cours (front montant : départ de la bouffée).
    pub drop: bool,
    /// Durée d'un temps, s.
    pub beat_s: f32,
    /// Apnée demandée (EV4) : la brume se retire en une mesure.
    pub apnea: bool,
    /// Flashs permis (réglage de l'opérateur).
    pub flashes: bool,
    /// Mode respiration : rien ne pulse.
    pub breathing: bool,
}

impl Default for FogInput {
    fn default() -> Self {
        Self {
            dt_s: 0.0,
            now_s: 0.0,
            energy_slow: 0.5,
            energy_fast: 0.5,
            low_mid: 0.0,
            rain: 0.0,
            wet: 0.0,
            drop: false,
            beat_s: 0.5,
            apnea: false,
            flashes: true,
            breathing: false,
        }
    }
}

/// L'état de la brume pour l'image.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct FogState {
    /// `σ_t·ρ0` au sol, 1/m.
    pub sigma_rho: f32,
    /// Hauteur d'échelle, m.
    pub height_m: f32,
    /// Anisotropie `g`.
    pub g: f32,
    /// Vitesse du vent, m/s.
    pub wind_mps: f32,
    /// Distance utile `d_brume` (m) : au-delà, `T < 2 %`.
    pub reach_m: f32,
    /// La coquille de kick est permise (ni respiration, ni flashs coupés).
    pub shell: bool,
}

/// L'état persistant de la modulation. Taille fixe, aucune allocation.
#[derive(Debug, Clone)]
pub struct FogModulation {
    density: f32,
    aniso: AttackRelease,
    puff: FlashLimiter,
    apnea: f32,
    was_drop: bool,
}

impl Default for FogModulation {
    fn default() -> Self {
        Self {
            // `0,6 + 0,8·E_s` à l'énergie neutre (0,5) : la brume d'ouverture est celle du look.
            density: 1.0,
            aniso: AttackRelease::new(0.05, 1.0),
            puff: FlashLimiter::new(),
            apnea: 0.0,
            was_drop: false,
        }
    }
}

impl FogModulation {
    /// Avance d'une image.
    pub fn step(&mut self, tuning: &FogTuning, input: &FogInput) -> FogState {
        let dt = if input.dt_s.is_finite() { input.dt_s.max(0.0) } else { 0.0 };
        let sat = |x: f32| if x.is_finite() { x.clamp(0.0, 1.0) } else { 0.0 };
        let beat_s = if input.beat_s.is_finite() { input.beat_s.clamp(0.25, 2.0) } else { 0.5 };

        // Densité de fond : ρ0 = ρ_look·(0,6 + 0,8·E_s), lissée (jamais un saut).
        let wanted = 0.6 + 0.8 * sat(input.energy_slow);
        self.density += (wanted - self.density) * alpha(dt, tuning.density_tau_s);

        // Anisotropie : sur un drop (E_f au-dessus de E_s) les faisceaux tranchent.
        let surge = (sat(input.energy_fast) - sat(input.energy_slow)).max(0.0);
        let a = if input.breathing { self.aniso.step(0.0, dt) } else { self.aniso.step(sat(surge * 2.0), dt) };
        let g = tuning.g_range.0 + (tuning.g_range.1 - tuning.g_range.0) * a;

        // Bouffée de drop [L] : au front montant, une enveloppe que le limiteur accorde ou refuse.
        self.puff.set_policy(input.flashes, input.breathing);
        if input.drop && !self.was_drop {
            self.puff.trigger(
                FlashRequest {
                    channel: Channel::Sweep,
                    peak: MAX_SWEEP_PEAK,
                    attack_s: beat_s * 0.5,
                    decay_s: beat_s * 8.0,
                },
                input.now_s,
            );
        }
        self.was_drop = input.drop;
        self.puff.step(dt);
        let puff = 1.0 + (tuning.puff_peak - 1.0) * self.puff.value(Channel::Sweep) / MAX_SWEEP_PEAK;

        // Apnée : la brume se retire (ou revient) en une mesure.
        let target = if input.apnea { 1.0 } else { 0.0 };
        let max_step = dt / (4.0 * beat_s);
        self.apnea += (target - self.apnea).clamp(-max_step, max_step);
        let apnea = 1.0 + (tuning.apnea - 1.0) * self.apnea;

        let weather = 1.0 + tuning.rain_gain * sat(input.rain) + tuning.wet_gain * sat(input.wet);
        let sigma_rho = tuning.sigma_rho * self.density * weather * puff * apnea;
        FogState {
            sigma_rho,
            height_m: tuning.height_m,
            g,
            wind_mps: tuning.wind_mps * (1.0 + 2.0 * sat(input.low_mid)),
            reach_m: fog_reach(sigma_rho),
            shell: input.flashes && !input.breathing,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run(m: &mut FogModulation, input: FogInput, seconds: f32, mut each: impl FnMut(&FogState, f64)) {
        let tuning = FogTuning::default();
        let dt = 1.0 / 60.0;
        let steps = (seconds / dt) as usize;
        let mut now = input.now_s;
        for _ in 0..steps {
            now += f64::from(dt);
            let s = m.step(&tuning, &FogInput { dt_s: dt, now_s: now, ..input });
            each(&s, now);
        }
    }

    #[test]
    fn a_l_ouverture_la_brume_est_celle_du_look() {
        let mut m = FogModulation::default();
        let s = m.step(&FogTuning::default(), &FogInput { dt_s: 1.0 / 60.0, ..FogInput::default() });
        assert!((s.sigma_rho - 0.0037).abs() < 1e-6);
        assert!((s.g - 0.55).abs() < 1e-3);
        assert!((s.reach_m - 1057.3).abs() < 2.0);
    }

    #[test]
    fn la_bouffee_de_drop_est_bornee_en_amplitude_et_en_pente() {
        let mut m = FogModulation::default();
        let mut last = 0.0037_f32;
        let mut peak = 0.0_f32;
        let mut max_rate = 0.0_f32;
        run(&mut m, FogInput { drop: true, ..FogInput::default() }, 6.0, |s, _| {
            max_rate = max_rate.max((s.sigma_rho - last).abs() * 60.0 / 0.0037);
            last = s.sigma_rho;
            peak = peak.max(s.sigma_rho);
        });
        // Pic ×1,8 au plus, atteint en un demi-temps au moins (0,25 s) : pente relative ≤ 0,8 / 0,25 par seconde.
        assert!(peak <= 0.0037 * 1.8 + 1e-6 && peak > 0.0037 * 1.7, "pic = {peak}");
        assert!(max_rate <= 0.8 / 0.25 + 0.05, "pente = {max_rate} /s");
        // Et elle retombe : 8 temps plus tard la brume est revenue.
        assert!(last < 0.0037 * 1.05, "fin = {last}");
    }

    #[test]
    fn un_drop_tenu_ne_redeclenche_pas_et_deux_drops_respectent_l_ecart() {
        let mut m = FogModulation::default();
        let tuning = FogTuning::default();
        let step = |m: &mut FogModulation, drop: bool, now: f64| {
            m.step(&tuning, &FogInput { dt_s: 1.0 / 60.0, now_s: now, drop, ..FogInput::default() }).sigma_rho
        };
        step(&mut m, true, 1.0);
        // Relâché puis redemandé 0,1 s plus tard : la voie est occupée, le limiteur refuse, la courbe reste lisse.
        let a = step(&mut m, false, 1.05);
        let b = step(&mut m, true, 1.1);
        assert!((b - a).abs() / 0.0037 < 0.8 / 0.25 / 60.0 + 1e-3);
    }

    #[test]
    fn flashs_coupes_ou_respiration_aucune_bouffee() {
        for (flashes, breathing) in [(false, false), (true, true)] {
            let mut m = FogModulation::default();
            let mut peak = 0.0_f32;
            let mut shell = true;
            let input = FogInput { drop: true, flashes, breathing, energy_fast: 1.0, energy_slow: 0.5, ..FogInput::default() };
            run(&mut m, input, 3.0, |s, _| {
                peak = peak.max(s.sigma_rho);
                shell = s.shell;
            });
            assert!(peak <= 0.0037 * 1.001, "pic = {peak}");
            assert!(!shell);
        }
    }

    #[test]
    fn l_apnee_retire_la_brume_en_une_mesure_sans_saut() {
        let mut m = FogModulation::default();
        let mut last = 0.0037_f32;
        let mut max_jump = 0.0_f32;
        run(&mut m, FogInput { apnea: true, ..FogInput::default() }, 2.5, |s, _| {
            max_jump = max_jump.max((s.sigma_rho - last).abs());
            last = s.sigma_rho;
        });
        assert!((last - 0.0037 * 0.3).abs() < 1e-5, "fin = {last}");
        assert!(max_jump < 0.0037 * 0.7 / 100.0, "saut = {max_jump}");
    }

    #[test]
    fn la_cadence_ne_change_pas_la_brume() {
        let tuning = FogTuning::default();
        let mut out = [0.0_f32; 3];
        for (i, fps) in [30.0_f32, 60.0, 144.0].into_iter().enumerate() {
            let mut m = FogModulation::default();
            let dt = 1.0 / fps;
            let mut s = None;
            for k in 0..(3.0 * fps) as usize {
                let input = FogInput { dt_s: dt, now_s: f64::from(k as f32 * dt), energy_slow: 1.0, ..FogInput::default() };
                s = Some(m.step(&tuning, &input));
            }
            out[i] = s.map_or(0.0, |s| s.sigma_rho);
        }
        assert!((out[0] - out[1]).abs() / out[1] < 0.01 && (out[2] - out[1]).abs() / out[1] < 0.01, "{out:?}");
    }
}
