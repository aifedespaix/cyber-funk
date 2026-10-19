//! Garanties du limiteur photosensible (WCAG 2.3.1), skill `live-safety` : une propriété `proptest` nommée par
//! garantie, sur des suites aléatoires de demandes, de pas de temps et de changements de réglage.
//!
//! Les entrées sont volontairement hostiles : pics énormes, négatifs, NaN ou infinis, durées nulles, trous d'image de
//! plusieurs secondes, horloge qui saute en arrière ou devient NaN.
//!
//! | # | Garantie                                            | Propriété                                              |
//! |---|-----------------------------------------------------|--------------------------------------------------------|
//! | 1 | au plus 3 flashs par seconde glissante              | `prop_g1_at_most_three_flashes_per_sliding_second`     |
//! | 2 | aucune variation plus rapide que `MIN_FADE`         | `prop_g2_no_luminance_change_faster_than_min_fade`     |
//! | 3 | pic ≤ `MAX_FLASH_PEAK` / `MAX_SWEEP_PEAK`           | `prop_g3_peaks_never_exceed_caps`                      |
//! | 4 | écart minimal entre deux départs                    | `prop_g4_starts_are_at_least_min_gap_apart`            |
//! | 5 | jamais de rouge saturé plein écran                  | `prop_g5_defuse_red_never_leaves_saturated_red`        |
//! | 6 | rien quand flashs coupés ou respiration             | `prop_g6_*`                                            |

use cf_show::limiter::{
    Channel, FlashLimiter, FlashRequest, MAX_FLASH_PEAK, MAX_FLASHES_PER_SECOND, MAX_STEP, MAX_SWEEP_PEAK, MIN_FADE,
    MIN_GAP, Rgb, SafeColor, defuse_red, is_saturated_red,
};
use proptest::prelude::*;

/// Tolérance relative sur la pente maximale (arrondis f32).
const SLOPE_EPS: f32 = 1e-3;

#[derive(Clone, Debug)]
enum Op {
    /// Image : `dt` s écoulées (l'horloge avance de la valeur réelle, le limiteur borne la sienne).
    Step(f32),
    /// Demande de flash à l'horloge courante.
    Trigger(FlashRequest),
    /// Demande de flash à une horloge arbitraire (NaN, infinie, ou en arrière).
    TriggerAt(FlashRequest, f64),
    /// Changement de réglage (flashs, respiration).
    Policy(bool, bool),
    /// Saut d'horloge (s), y compris en arrière.
    Jump(f64),
}

fn channel() -> impl Strategy<Value = Channel> {
    prop_oneof![Just(Channel::Flash), Just(Channel::Sweep)]
}

fn peak() -> impl Strategy<Value = f32> {
    prop_oneof![
        8 => 0.0f32..1.0,
        2 => 1.0f32..100.0,
        1 => -5.0f32..0.0,
        1 => Just(0.0),
        1 => Just(f32::NAN),
        1 => Just(f32::INFINITY),
        1 => Just(f32::NEG_INFINITY),
        1 => Just(f32::MAX),
    ]
}

fn duration() -> impl Strategy<Value = f32> {
    prop_oneof![
        6 => 0.0f32..0.5,
        2 => 0.5f32..30.0,
        1 => Just(0.0),
        1 => -3.0f32..0.0,
        1 => Just(f32::NAN),
        1 => Just(f32::INFINITY),
        1 => Just(f32::NEG_INFINITY),
        1 => Just(1e30f32),
    ]
}

fn request() -> impl Strategy<Value = FlashRequest> {
    (channel(), peak(), duration(), duration()).prop_map(|(channel, peak, attack_s, decay_s)| FlashRequest {
        channel,
        peak,
        attack_s,
        decay_s,
    })
}

fn bad_clock() -> impl Strategy<Value = f64> {
    prop_oneof![
        Just(f64::NAN),
        Just(f64::INFINITY),
        Just(f64::NEG_INFINITY),
        -100.0f64..100.0,
        Just(1e300),
        Just(-1e300),
    ]
}

fn dt() -> impl Strategy<Value = f32> {
    prop_oneof![
        8 => 0.0f32..0.05,
        3 => 0.05f32..0.3,
        1 => 0.3f32..5.0,
        1 => Just(0.0),
        1 => -0.2f32..0.0,
        1 => Just(f32::NAN),
        1 => Just(f32::INFINITY),
    ]
}

/// Suite d'opérations, sans changement de réglage : le réglage de départ est posé par le test.
fn ops_without_policy() -> impl Strategy<Value = Vec<Op>> {
    let op = prop_oneof![
        14 => dt().prop_map(Op::Step),
        8 => request().prop_map(Op::Trigger),
        1 => (request(), bad_clock()).prop_map(|(r, t)| Op::TriggerAt(r, t)),
        1 => (-3.0f64..3.0).prop_map(Op::Jump),
    ];
    prop::collection::vec(op, 0..400)
}

/// Suite complète, avec bascules de réglage.
fn ops_with_policy() -> impl Strategy<Value = Vec<Op>> {
    let op = prop_oneof![
        14 => dt().prop_map(Op::Step),
        8 => request().prop_map(Op::Trigger),
        1 => (request(), bad_clock()).prop_map(|(r, t)| Op::TriggerAt(r, t)),
        1 => (-3.0f64..3.0).prop_map(Op::Jump),
        2 => (any::<bool>(), any::<bool>()).prop_map(|(f, b)| Op::Policy(f, b)),
    ];
    prop::collection::vec(op, 0..400)
}

/// Ce qui s'est passé pendant une opération.
#[derive(Clone, Copy, Debug)]
struct Event {
    op_is_step: bool,
    /// Temps réellement intégré par le limiteur (s) : `min(dt, MAX_STEP)` si `dt` est un pas valide, sinon 0.
    effective_dt: f32,
    /// Niveaux (flash, vague) avant et après l'opération.
    before: [f32; 2],
    after: [f32; 2],
    /// Instant d'un départ accordé par cette opération.
    granted_at: Option<f64>,
    /// Les flashs étaient-ils permis au moment de l'opération ?
    permitted: bool,
}

fn levels(limiter: &FlashLimiter) -> [f32; 2] {
    [limiter.value(Channel::Flash), limiter.value(Channel::Sweep)]
}

/// Rejoue `ops` sur un limiteur neuf (réglage initial `policy`) et trace chaque opération.
fn run(policy: (bool, bool), ops: &[Op]) -> Vec<Event> {
    let mut limiter = FlashLimiter::new();
    limiter.set_policy(policy.0, policy.1);
    let mut now = 0.0f64;
    let mut events = Vec::with_capacity(ops.len());
    for op in ops {
        let before = levels(&limiter);
        let permitted = limiter.permitted();
        let mut event = Event {
            op_is_step: false,
            effective_dt: 0.0,
            before,
            after: before,
            granted_at: None,
            permitted,
        };
        match op {
            Op::Step(dt) => {
                event.op_is_step = true;
                if dt.is_finite() && *dt > 0.0 {
                    event.effective_dt = dt.min(MAX_STEP);
                    now += f64::from(*dt);
                }
                limiter.step(*dt);
            }
            Op::Trigger(request) => {
                if limiter.trigger(*request, now) {
                    event.granted_at = Some(now);
                }
            }
            Op::TriggerAt(request, at) => {
                if limiter.trigger(*request, *at) {
                    event.granted_at = Some(*at);
                }
            }
            Op::Policy(flashes, breathing) => limiter.set_policy(*flashes, *breathing),
            Op::Jump(delta) => now += delta,
        }
        event.after = levels(&limiter);
        events.push(event);
    }
    events
}

fn grants(events: &[Event]) -> Vec<f64> {
    events.iter().filter_map(|e| e.granted_at).collect()
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(512))]

    /// Garantie 1 : dans toute fenêtre glissante d'une seconde, au plus `MAX_FLASHES_PER_SECOND` départs, toutes voies
    /// confondues, même avec une horloge folle.
    #[test]
    fn prop_g1_at_most_three_flashes_per_sliding_second(ops in ops_with_policy()) {
        let g = grants(&run((true, false), &ops));
        for (i, &start) in g.iter().enumerate() {
            let in_window = g[i..].iter().take_while(|&&t| t - start < 1.0).count();
            prop_assert!(
                in_window <= MAX_FLASHES_PER_SECOND,
                "{in_window} départs dans [{start}, {start}+1[ : {g:?}"
            );
        }
    }

    /// Garantie 2 : aucune variation de luminance plus rapide que `MIN_FADE` (montée comme descente), par voie et pour
    /// leur somme ; et aucun saut : seules les images (`step`) font bouger le niveau.
    #[test]
    fn prop_g2_no_luminance_change_faster_than_min_fade(ops in ops_with_policy()) {
        let caps = [MAX_FLASH_PEAK, MAX_SWEEP_PEAK];
        for e in run((true, false), &ops) {
            let mut total_delta = 0.0f32;
            for (c, cap) in caps.iter().enumerate() {
                let delta = (e.after[c] - e.before[c]).abs();
                total_delta += delta;
                if e.op_is_step {
                    // Pleine amplitude (`cap`) en `MIN_FADE` au plus vite : pente maximale caps/MIN_FADE.
                    let allowed = cap / MIN_FADE * e.effective_dt * (1.0 + SLOPE_EPS) + 1e-6;
                    prop_assert!(delta <= allowed, "voie {c} : Δ={delta} > {allowed} pour dt_eff={}", e.effective_dt);
                } else {
                    prop_assert!(delta <= 0.0, "voie {c} : saut de {delta} hors d'un pas de temps");
                }
            }
            let allowed_total = (MAX_FLASH_PEAK + MAX_SWEEP_PEAK) / MIN_FADE * e.effective_dt * (1.0 + SLOPE_EPS) + 2e-6;
            prop_assert!(total_delta <= allowed_total, "somme : Δ={total_delta} > {allowed_total}");
        }
    }

    /// Garantie 3 : le niveau ne dépasse jamais le plafond de sa voie (0,4 flash, 0,14 vague), quelle que soit la
    /// demande, y compris infinie ou NaN, et reste fini et positif.
    #[test]
    fn prop_g3_peaks_never_exceed_caps(ops in ops_with_policy()) {
        for e in run((true, false), &ops) {
            for (c, cap) in [(0, MAX_FLASH_PEAK), (1, MAX_SWEEP_PEAK)] {
                let level = e.after[c];
                prop_assert!(level.is_finite() && (0.0..=cap).contains(&level), "voie {c} : {level} hors de 0..={cap}");
            }
        }
    }

    /// Garantie 4 : au moins `MIN_GAP` (0,25 s) entre deux départs accordés, voies confondues.
    #[test]
    fn prop_g4_starts_are_at_least_min_gap_apart(ops in ops_with_policy()) {
        let g = grants(&run((true, false), &ops));
        for pair in g.windows(2) {
            prop_assert!(pair[1] - pair[0] >= MIN_GAP, "départs trop proches : {} puis {}", pair[0], pair[1]);
        }
    }

    /// Garantie 6 (réglage coupé dès le départ) : flashs coupés (`FlashKill`) ou respiration, quelles que soient les
    /// demandes, aucun départ accordé et lumière toujours nulle.
    #[test]
    fn prop_g6_flash_kill_or_breathing_from_start_produces_nothing(
        cut in prop_oneof![Just((false, false)), Just((false, true)), Just((true, true))],
        ops in ops_without_policy(),
    ) {
        for e in run(cut, &ops) {
            prop_assert!(e.granted_at.is_none(), "départ accordé alors que les flashs sont coupés ({cut:?})");
            prop_assert!(e.after == [0.0, 0.0], "lumière {:?} alors que les flashs sont coupés ({cut:?})", e.after);
        }
    }

    /// Garantie 6 (coupure en cours de show) : dès la bascule, plus aucun départ ; la lumière encore présente ne fait
    /// que redescendre et est à 0 au bout de `MIN_FADE` de temps de limiteur (jamais de coupure sèche, jamais de
    /// remontée).
    #[test]
    fn prop_g6_cut_during_show_blocks_triggers_and_fades_out_within_min_fade(ops in ops_with_policy()) {
        let events = run((true, false), &ops);
        let mut elapsed_cut = 0.0f64;
        for e in &events {
            if e.permitted {
                elapsed_cut = 0.0;
                continue;
            }
            prop_assert!(e.granted_at.is_none(), "départ accordé pendant une coupure");
            for c in 0..2 {
                prop_assert!(e.after[c] <= e.before[c], "voie {c} : remontée pendant une coupure");
            }
            elapsed_cut += f64::from(e.effective_dt);
            if elapsed_cut >= f64::from(MIN_FADE) * 1.01 {
                prop_assert!(e.after == [0.0, 0.0], "encore {:?} après {elapsed_cut} s de coupure", e.after);
            }
        }
    }

    /// Garantie 5 : quelle que soit la couleur (même non finie ou négative), `defuse_red` et `SafeColor` ne laissent
    /// jamais un rouge saturé, ne créent pas de lumière (aucune composante ne dépasse la plus forte de l'entrée) et
    /// laissent intacte une teinte déjà sûre.
    #[test]
    fn prop_g5_defuse_red_never_leaves_saturated_red(
        r in color_component(), g in color_component(), b in color_component(),
        red_bias in 0.0f32..1.0, strong in 0.15f32..4.0,
    ) {
        // Couleur libre, et couleur forcée à dominante rouge (la zone dangereuse, trop rare au tirage libre).
        let free = Rgb::new(r, g, b);
        let reddish = Rgb::new(strong, strong * red_bias * 0.7, strong * red_bias * 0.3);
        for input in [free, reddish] {
            let out = defuse_red(input);
            prop_assert!(!is_saturated_red(out), "{input:?} → {out:?} reste un rouge saturé");
            prop_assert!(!is_saturated_red(SafeColor::new(input).rgb()), "SafeColor laisse passer {input:?}");
            let clean = |v: f32| if v.is_finite() && v > 0.0 { v } else { 0.0 };
            let max_in = clean(input.r).max(clean(input.g)).max(clean(input.b));
            for v in [out.r, out.g, out.b] {
                prop_assert!(v.is_finite() && (0.0..=max_in).contains(&v), "{input:?} → {out:?} : composante hors 0..={max_in}");
            }
            if !is_saturated_red(Rgb::new(clean(input.r), clean(input.g), clean(input.b))) {
                prop_assert!(out == Rgb::new(clean(input.r), clean(input.g), clean(input.b)), "teinte sûre modifiée : {input:?} → {out:?}");
            }
        }
    }
}

fn color_component() -> impl Strategy<Value = f32> {
    prop_oneof![
        8 => 0.0f32..1.0,
        2 => 1.0f32..4.0,
        1 => Just(0.0),
        1 => Just(f32::NAN),
        1 => Just(f32::INFINITY),
        1 => -1.0f32..0.0,
    ]
}

// --- Cas concrets : documentent le comportement et ne dépendent pas du hasard ---

fn flash(peak: f32) -> FlashRequest {
    FlashRequest {
        channel: Channel::Flash,
        peak,
        attack_s: 0.0,
        decay_s: 0.0,
    }
}

#[test]
fn fourth_flash_within_a_second_is_refused_then_granted() {
    let mut l = FlashLimiter::new();
    let t = [0.0, 0.3, 0.6];
    for &at in &t {
        // Laisse la voie se vider entre deux départs.
        for _ in 0..30 {
            l.step(1.0 / 30.0);
        }
        assert!(l.trigger(flash(1.0), at));
    }
    for _ in 0..30 {
        l.step(1.0 / 30.0);
    }
    assert!(!l.trigger(flash(1.0), 0.9), "4e flash dans la seconde");
    assert!(l.trigger(flash(1.0), 1.0), "la fenêtre a glissé");
}

#[test]
fn requests_are_clamped_to_caps_and_min_fade() {
    let mut l = FlashLimiter::new();
    assert!(l.trigger(flash(10.0), 0.0));
    // Attaque demandée à 0 s : relevée à MIN_FADE, donc pente MAX_FLASH_PEAK / MIN_FADE (0,03 s < MAX_STEP).
    l.step(0.03);
    let v = l.value(Channel::Flash);
    assert!((v - MAX_FLASH_PEAK / MIN_FADE * 0.03).abs() < 1e-4, "{v}");
    // Deux pas de plus (0,09 s au total ≥ MIN_FADE) : le sommet est atteint au plafond, pas au pic demandé (10).
    l.step(0.03);
    l.step(0.03);
    assert!(
        (l.value(Channel::Flash) - MAX_FLASH_PEAK).abs() < 1e-6,
        "{}",
        l.value(Channel::Flash)
    );
}
