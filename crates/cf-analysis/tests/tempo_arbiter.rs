//! Arbitrage des sources de tempo sur des scénarios synthétiques, avec le vrai décodeur de MIDI Clock de `cf-control` :
//! priorité, bascule sans saut de phase, repli après perte, tap, `Resync`, `NudgePhase`, préférence du créneau.

use cf_analysis::{AudioBeatGrid, AudioFeatures, EXTERNAL_LOSS_S, TempoArbiter, TempoReading, TempoSource};
use cf_control::{MidiClock, MidiMessage, PPQN, TapTempo, TempoEstimate};

/// Une image à 60 i/s.
const DT: f64 = 1.0 / 60.0;
const JITTER_S: f64 = 0.002;

/// Générateur à graine fixe (xorshift64*), uniforme dans [-1, 1].
struct Rng(u64);

impl Rng {
    fn signed(&mut self) -> f64 {
        self.0 ^= self.0 >> 12;
        self.0 ^= self.0 << 25;
        self.0 ^= self.0 >> 27;
        let x = self.0.wrapping_mul(0x2545_F491_4F6C_DD1D);
        (x >> 11) as f64 / (1u64 << 53) as f64 * 2.0 - 1.0
    }
}

/// Le pont de l'application : recopie l'estimation de `cf-control` dans la lecture de `cf-analysis`.
fn reading(e: TempoEstimate) -> TempoReading {
    TempoReading {
        time_s: e.time_s,
        beat: e.beat,
        bpm: e.bpm,
        bar_aligned: e.bar_aligned,
        confidence: e.confidence,
    }
}

/// Un plateau : horloge MIDI à gigue, détection audio (kicks quantifiés à l'image), tap, arbitre, à 60 i/s.
struct Rig {
    arbiter: TempoArbiter,
    clock: MidiClock,
    grid: AudioBeatGrid,
    tap: TapTempo,
    rng: Rng,
    now: f64,
    /// Tempo de l'horloge MIDI, `None` : plus aucune impulsion.
    clock_bpm: Option<f64>,
    next_tick: f64,
    /// Tempo de la musique entendue, `None` : pas de signal.
    audio_bpm: Option<f64>,
    next_kick: f64,
    /// Pire écart, sur une image, entre l'avance de la position publiée et l'avance attendue au tempo publié, rapporté
    /// à l'avance attendue (0 : avance exacte ; 1 : la position s'arrête ou double).
    worst_step: f64,
    prev: Option<(f64, f64)>,
}

impl Rig {
    fn new(seed: u64) -> Self {
        Self {
            arbiter: TempoArbiter::new(),
            clock: MidiClock::new(),
            grid: AudioBeatGrid::new(),
            tap: TapTempo::new(),
            rng: Rng(0x9E37_79B9_7F4A_7C15 ^ seed),
            now: 0.0,
            clock_bpm: None,
            next_tick: 0.0,
            audio_bpm: None,
            next_kick: 0.0,
            worst_step: 0.0,
            prev: None,
        }
    }

    /// Démarre l'horloge MIDI (Start puis impulsions) dans `delay_s`.
    fn start_clock(&mut self, bpm: f64, delay_s: f64) {
        self.clock.handle_message(self.now, MidiMessage::Start);
        self.clock_bpm = Some(bpm);
        self.next_tick = self.now + delay_s;
    }

    fn frame(&mut self) {
        let end = self.now + DT;
        if let Some(bpm) = self.clock_bpm {
            while self.next_tick <= end {
                let t = self.next_tick + JITTER_S * self.rng.signed();
                self.clock.handle_message(t, MidiMessage::TimingClock);
                self.next_tick += 60.0 / (bpm * f64::from(PPQN));
            }
        }
        let mut kick = false;
        if let Some(bpm) = self.audio_bpm
            && self.next_kick <= end
        {
            kick = true;
            self.next_kick += 60.0 / bpm;
        }
        self.now = end;
        let audio = self.grid.observe(
            self.now,
            self.audio_bpm.is_some(),
            self.audio_bpm.map(|b| b as f32),
            kick,
        );
        self.arbiter
            .offer(TempoSource::Clock, self.clock.estimate().map(reading));
        self.arbiter.offer(TempoSource::Audio, audio);
        self.arbiter.offer(TempoSource::Tap, self.tap.estimate().map(reading));
        self.arbiter.update(self.now);
        if let (Some(beat), Some(bpm)) = (self.arbiter.beat(), self.arbiter.bpm()) {
            if let Some((prev_beat, prev_bpm)) = self.prev
                && (prev_bpm - bpm).abs() < 1.0
            {
                let expected = DT * bpm / 60.0;
                self.worst_step = self.worst_step.max(((beat - prev_beat) - expected).abs() / expected);
            }
            self.prev = Some((beat, bpm));
        }
    }

    fn run_for(&mut self, seconds: f64) {
        let end = self.now + seconds;
        while self.now + DT / 2.0 < end {
            self.frame();
        }
    }

    fn published(&self) -> AudioFeatures {
        let mut f = AudioFeatures::NEUTRAL;
        self.arbiter.publish(&mut f);
        f
    }
}

/// Écart de phase dans la mesure entre deux positions (temps), ramené dans [-2, 2].
fn bar_phase_gap(a: f64, b: f64) -> f64 {
    (a - b + 2.0).rem_euclid(4.0) - 2.0
}

#[test]
fn clock_takes_over_audio_then_loss_falls_back_without_phase_jump() {
    let mut rig = Rig::new(1);
    rig.audio_bpm = Some(120.0);
    rig.next_kick = 0.13;
    rig.run_for(6.0);
    assert_eq!(rig.arbiter.source(), Some(TempoSource::Audio));
    assert!(
        rig.arbiter.confidence() >= 0.5,
        "audio stable : {}",
        rig.arbiter.confidence()
    );

    // Le logiciel de mix démarre son horloge, hors phase avec la grille audio.
    rig.start_clock(120.0, 0.37);
    let started = rig.now;
    while rig.arbiter.source() != Some(TempoSource::Clock) {
        rig.frame();
        assert!(
            rig.now - started < 3.0,
            "l'horloge stable prend la main en moins de 3 s"
        );
    }
    println!("horloge en service {:.2} s après son départ", rig.now - started);

    // Une mesure plus tard (2 s à 120 BPM), les temps 1 publiés sont ceux de l'horloge.
    rig.run_for(2.5);
    let clock_beat = rig
        .clock
        .estimate()
        .map(|e| reading(e).beat_at(rig.now))
        .expect("horloge");
    let published = rig.arbiter.beat().expect("position");
    assert!(
        bar_phase_gap(published, clock_beat).abs() < 0.01,
        "{published} / {clock_beat}"
    );
    let mut worst_bpm: f64 = 0.0;
    for _ in 0..600 {
        rig.frame();
        worst_bpm = worst_bpm.max((rig.arbiter.bpm().expect("tempo") - 120.0).abs());
    }
    assert!(worst_bpm <= 0.1, "tempo publié à ±{worst_bpm:.3}");

    // Le câble est arraché : plus d'impulsion. L'horloge reste en roue libre deux secondes, puis repli sur l'audio.
    rig.clock_bpm = None;
    let last_tick = rig.clock.last_tick_s().expect("impulsions reçues");
    rig.run_for(EXTERNAL_LOSS_S - (rig.now - last_tick) - 0.1);
    assert_eq!(rig.arbiter.source(), Some(TempoSource::Clock));
    rig.run_for(0.2);
    assert_eq!(
        rig.arbiter.source(),
        Some(TempoSource::Audio),
        "repli après 2 s de perte"
    );
    rig.run_for(3.0);

    let f = rig.published();
    assert_eq!(f.tempo_source, TempoSource::Audio);
    assert_eq!(f.bpm, Some(120.0));
    println!("pire écart d'avance par image : {:.0} %", rig.worst_step * 100.0);
    assert!(rig.worst_step <= 0.8, "bascules sans saut : {:.2}", rig.worst_step);
}

#[test]
fn tempo_change_reaches_the_published_tempo_in_less_than_two_bars() {
    let mut rig = Rig::new(2);
    rig.start_clock(124.0, 0.01);
    rig.run_for(16.0);
    assert_eq!(rig.arbiter.source(), Some(TempoSource::Clock));
    // Le DJ passe à 130 BPM : deux mesures à 130 = 3,69 s.
    rig.clock_bpm = Some(130.0);
    let changed = rig.now;
    let mut settled = changed;
    while rig.now - changed < 8.0 {
        rig.frame();
        if (rig.arbiter.bpm().expect("tempo") - 130.0).abs() > 0.1 {
            settled = rig.now;
        }
    }
    let bars = (settled - changed) * 130.0 / 60.0 / 4.0;
    println!("124 → 130 BPM : tempo publié à ±0,1 après {bars:.2} mesure");
    assert!(bars < 2.0, "{bars:.2} mesures");
}

#[test]
fn stop_falls_back_at_once() {
    let mut rig = Rig::new(3);
    rig.audio_bpm = Some(126.0);
    rig.start_clock(126.0, 0.05);
    rig.run_for(6.0);
    assert_eq!(rig.arbiter.source(), Some(TempoSource::Clock));
    rig.clock.handle_message(rig.now, MidiMessage::Stop);
    rig.frame();
    assert_eq!(rig.arbiter.source(), Some(TempoSource::Audio));
}

#[test]
fn tap_alone_sets_tempo_and_downbeat() {
    let mut rig = Rig::new(4);
    rig.run_for(1.0);
    assert_eq!(rig.arbiter.source(), None);
    assert!(rig.published().confidence.abs() < f32::EPSILON);
    let first = rig.now;
    for k in 0..4 {
        rig.tap.tap(first + 0.5 * f64::from(k));
    }
    rig.run_for(1.5 + 0.25);
    assert_eq!(rig.arbiter.source(), Some(TempoSource::Tap));
    let f = rig.published();
    assert_eq!(f.tempo_source, TempoSource::Tap);
    assert!((f.bpm.expect("tempo") - 120.0).abs() < 1e-3);
    // Première frappe = temps 1 : `now − first` vaut 1,75 s = 3,5 temps.
    let elapsed_beats = (rig.now - first) * 2.0;
    assert!(
        (f64::from(f.phase) - elapsed_beats.rem_euclid(4.0) / 4.0).abs() < 0.01,
        "{}",
        f.phase
    );
}

#[test]
fn resync_and_nudge_move_the_grid() {
    let mut rig = Rig::new(5);
    rig.audio_bpm = Some(120.0);
    rig.next_kick = 0.2;
    rig.run_for(5.0);
    rig.arbiter.resync(rig.now);
    let f = rig.published();
    assert!(
        f.phase.abs() < 1e-4 || (f.phase - 1.0).abs() < 1e-4,
        "temps 1 : {}",
        f.phase
    );
    let before = rig.arbiter.beat().expect("position");
    rig.arbiter.nudge(0.25);
    assert!((rig.arbiter.beat().expect("position") - before - 0.25).abs() < 1e-12);
    // Le décalage tient dans la durée.
    rig.run_for(4.0);
    let f = rig.published();
    assert!((f64::from(f.phase) * 4.0 - 0.25).abs() < 0.05, "{}", f.phase);
}

#[test]
fn preferred_source_wins_and_auto_restores_priority() {
    let mut rig = Rig::new(6);
    rig.audio_bpm = Some(120.0);
    rig.run_for(5.0);
    for k in 0..4 {
        rig.tap.tap(rig.now + 0.48 * f64::from(k));
    }
    rig.run_for(2.0);
    assert_eq!(
        rig.arbiter.source(),
        Some(TempoSource::Audio),
        "le tap reste en secours"
    );
    rig.arbiter.set_preferred(Some(TempoSource::Tap));
    rig.run_for(1.2);
    assert_eq!(rig.arbiter.source(), Some(TempoSource::Tap));
    rig.arbiter.set_preferred(None);
    rig.run_for(1.2);
    assert_eq!(rig.arbiter.source(), Some(TempoSource::Audio));
}

#[test]
fn without_any_source_the_grid_freewheels() {
    let mut rig = Rig::new(7);
    for k in 0..4 {
        rig.tap.tap(0.5 * f64::from(k));
    }
    rig.run_for(2.0);
    let beat = rig.arbiter.beat().expect("position");
    let confidence = rig.arbiter.confidence();
    rig.tap.reset();
    rig.run_for(2.0);
    assert_eq!(rig.arbiter.source(), None);
    assert!((rig.arbiter.beat().expect("roue libre") - beat - 4.0).abs() < 0.05);
    assert!(rig.arbiter.confidence() < confidence * 0.7);
    let f = rig.published();
    assert_eq!(f.tempo_source, TempoSource::Tap);
    assert!(f.bar >= 1);
}
