//! MIDI Clock sur horodatages synthétiques : gigue, changement de tempo, transport, perte d'impulsions, ports.

use cf_control::{MidiClock, MidiEvent, MidiMessage, PPQN, PortId, Transport};

/// Générateur pseudo-aléatoire à graine fixe (xorshift64*) : les tests restent déterministes.
struct Rng(u64);

impl Rng {
    /// Uniforme dans [-1, 1].
    fn signed(&mut self) -> f64 {
        self.0 ^= self.0 >> 12;
        self.0 ^= self.0 << 25;
        self.0 ^= self.0 >> 27;
        let x = self.0.wrapping_mul(0x2545_F491_4F6C_DD1D);
        (x >> 11) as f64 / (1u64 << 53) as f64 * 2.0 - 1.0
    }
}

const JITTER_S: f64 = 0.002;
const TICKS_PER_BAR: u32 = PPQN * 4;

/// Envoie `ticks` impulsions au tempo `bpm` à partir de la grille `*grid_s` (avancée), chacune décalée d'une gigue
/// uniforme de ±`jitter_s`. `check` reçoit l'horloge après chaque impulsion, avec son rang dans ce segment.
fn run(
    clock: &mut MidiClock,
    grid_s: &mut f64,
    bpm: f64,
    ticks: u32,
    jitter_s: f64,
    rng: &mut Rng,
    mut check: impl FnMut(&MidiClock, u32),
) {
    let interval = 60.0 / (bpm * f64::from(PPQN));
    for k in 0..ticks {
        clock.handle_message(*grid_s + jitter_s * rng.signed(), MidiMessage::TimingClock);
        check(clock, k);
        *grid_s += interval;
    }
}

#[test]
fn jitter_2ms_keeps_bpm_within_a_tenth() {
    for (bpm, seed) in [90.0, 120.0, 128.0, 174.0]
        .into_iter()
        .flat_map(|b| (0..8).map(move |s| (b, s)))
    {
        let mut clock = MidiClock::new();
        let mut rng = Rng(0x9E37_79B9_7F4A_7C15 ^ seed);
        let mut grid = 100.0;
        let mut worst: f64 = 0.0;
        run(
            &mut clock,
            &mut grid,
            bpm,
            32 * TICKS_PER_BAR,
            JITTER_S,
            &mut rng,
            |c, k| {
                // Une mesure pour s'établir, puis le tempo ne doit plus bouger de plus d'un dixième.
                if k >= TICKS_PER_BAR {
                    let measured = c.bpm().expect("tempo établi");
                    worst = worst.max((measured - bpm).abs());
                }
            },
        );
        println!("gigue ±2 ms, {bpm} BPM, graine {seed} : écart maximal {worst:.4} BPM");
        assert!(worst <= 0.1, "{bpm} BPM : écart maximal {worst:.4}");
        let estimate = clock.estimate().expect("estimation");
        assert!(estimate.confidence > 0.5, "confiance {}", estimate.confidence);
    }
}

#[test]
fn tempo_change_followed_in_less_than_two_bars() {
    for (from, to, seed) in [
        (120.0, 128.0, 7),
        (128.0, 124.0, 8),
        (124.0, 140.0, 9),
        (140.0, 70.0, 10),
    ] {
        let mut clock = MidiClock::new();
        let mut rng = Rng(0xD1B5_4A32_D192_ED03 ^ seed);
        let mut grid = 0.0;
        run(
            &mut clock,
            &mut grid,
            from,
            8 * TICKS_PER_BAR,
            JITTER_S,
            &mut rng,
            |_, _| {},
        );
        // Dernière impulsion du nouveau tempo où l'écart dépasse encore 0,1 BPM.
        let mut last_off = None;
        run(
            &mut clock,
            &mut grid,
            to,
            8 * TICKS_PER_BAR,
            JITTER_S,
            &mut rng,
            |c, k| {
                if c.bpm().is_none_or(|b| (b - to).abs() > 0.1) {
                    last_off = Some(k);
                }
            },
        );
        let settled_after = last_off.map_or(0, |k| k + 1);
        println!(
            "{from} → {to} BPM : stable à ±0,1 après {:.2} mesure",
            f64::from(settled_after) / f64::from(TICKS_PER_BAR)
        );
        assert!(
            settled_after < 2 * TICKS_PER_BAR,
            "{from} → {to} : stable à ±0,1 après {settled_after} impulsions ({:.2} mesure)",
            f64::from(settled_after) / f64::from(TICKS_PER_BAR)
        );
    }
}

#[test]
fn slow_drift_is_tracked() {
    // Pitch de platine : +0,5 BPM par mesure, sous le seuil de changement franc.
    let mut clock = MidiClock::new();
    let mut rng = Rng(42);
    let mut grid = 0.0;
    for bar in 0..16 {
        let bpm = 120.0 + 0.5 * f64::from(bar);
        run(&mut clock, &mut grid, bpm, TICKS_PER_BAR, JITTER_S, &mut rng, |_, _| {});
    }
    let measured = clock.bpm().expect("tempo");
    // La fenêtre de deux mesures centre l'estimation environ une mesure en arrière.
    assert!((measured - 127.0).abs() < 0.6, "{measured}");
}

#[test]
fn start_marks_the_downbeat_and_stop_freezes() {
    let mut clock = MidiClock::new();
    let mut rng = Rng(5);
    let mut grid = 0.0;
    // Horloge prise en cours de route : position libre, mesure inconnue.
    run(&mut clock, &mut grid, 120.0, 48, 0.0, &mut rng, |_, _| {});
    assert_eq!(clock.transport(), Transport::Unknown);
    let e = clock.estimate().expect("tempo établi");
    assert!(!e.bar_aligned);

    clock.handle_message(grid - 0.001, MidiMessage::Start);
    run(&mut clock, &mut grid, 120.0, 1, 0.0, &mut rng, |c, _| {
        assert!(
            c.song_beats().abs() < 1e-12,
            "la première impulsion après Start est le temps 1"
        );
    });
    run(&mut clock, &mut grid, 120.0, 2 * PPQN, 0.0, &mut rng, |_, _| {});
    let e = clock.estimate().expect("en marche");
    assert!(e.bar_aligned);
    assert!((e.beat - 2.0).abs() < 1e-12);
    assert!((e.bpm - 120.0).abs() < 1e-6);
    assert!((e.time_s - (grid - 0.5 / f64::from(PPQN))).abs() < 1e-9);

    clock.handle_message(grid, MidiMessage::Stop);
    run(&mut clock, &mut grid, 120.0, PPQN, 0.0, &mut rng, |_, _| {});
    assert_eq!(clock.transport(), Transport::Stopped);
    assert!(clock.estimate().is_none(), "arrêté : pas de source");
    assert!((clock.song_beats() - 2.0).abs() < 1e-12, "position figée");

    clock.handle_message(grid, MidiMessage::Continue);
    run(&mut clock, &mut grid, 120.0, 1, 0.0, &mut rng, |_, _| {});
    assert!((clock.song_beats() - (2.0 + 1.0 / f64::from(PPQN))).abs() < 1e-12);
}

#[test]
fn song_position_sets_the_bar() {
    let mut clock = MidiClock::new();
    let mut rng = Rng(6);
    let mut grid = 0.0;
    clock.handle_message(grid, MidiMessage::Stop);
    // 33 doubles croches = mesure 3, temps 1, plus une double croche : 8,25 temps.
    clock.handle_message(grid, MidiMessage::SongPosition { sixteenths: 33 });
    clock.handle_message(grid, MidiMessage::Continue);
    run(&mut clock, &mut grid, 120.0, PPQN + 1, 0.0, &mut rng, |_, _| {});
    let e = clock.estimate().expect("en marche");
    assert!(e.bar_aligned);
    assert!((e.beat - 9.25).abs() < 1e-12, "{}", e.beat);
}

#[test]
fn dropped_tick_keeps_the_published_tempo() {
    let mut clock = MidiClock::new();
    let mut rng = Rng(11);
    let mut grid = 0.0;
    run(
        &mut clock,
        &mut grid,
        125.0,
        4 * TICKS_PER_BAR,
        JITTER_S,
        &mut rng,
        |_, _| {},
    );
    grid += 60.0 / (125.0 * f64::from(PPQN)); // une impulsion perdue
    run(
        &mut clock,
        &mut grid,
        125.0,
        2 * TICKS_PER_BAR,
        JITTER_S,
        &mut rng,
        |c, _| {
            let bpm = c.bpm().expect("le tempo reste publié pendant la reconstruction");
            assert!((bpm - 125.0).abs() <= 0.1, "{bpm}");
        },
    );
    // L'impulsion perdue est comptée : 4 mesures + 1 + 2 mesures d'impulsions, la première à la position 0.
    let expected_ticks = 6 * TICKS_PER_BAR;
    assert!((clock.song_beats() - f64::from(expected_ticks) / f64::from(PPQN)).abs() < 1e-12);
}

#[test]
fn second_port_waits_for_the_first_to_fall_silent() {
    let mut clock = MidiClock::new();
    let tick = |time_s, port| MidiEvent {
        time_s,
        port: PortId(port),
        message: MidiMessage::TimingClock,
    };
    let interval = 0.5 / f64::from(PPQN);
    let mut t = 0.0;
    for _ in 0..48 {
        assert!(clock.handle(&tick(t, 1)));
        // Un autre logiciel envoie un clock à un autre tempo : ignoré.
        assert!(clock.handle(&tick(t + 0.003, 2)));
        t += interval;
    }
    assert_eq!(clock.port(), Some(PortId(1)));
    assert!((clock.bpm().expect("tempo") - 120.0).abs() < 1e-6);
    // Les notes ne sont pas pour l'horloge.
    let note = MidiEvent {
        time_s: t,
        port: PortId(1),
        message: MidiMessage::NoteOn {
            channel: 0,
            note: 36,
            velocity: 100,
        },
    };
    assert!(!clock.handle(&note));
    // Le port 1 se tait plus de deux secondes : le port 2 prend l'horloge.
    t += 2.5;
    clock.handle(&tick(t, 2));
    assert_eq!(clock.port(), Some(PortId(2)));
    assert!(clock.bpm().is_none(), "nouvelle horloge, tempo à reconstruire");
}
