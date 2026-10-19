//! Le limiteur Rust rejoue le scénario produit par le vrai `show-fx-limiter.ts` de Sillage
//! (`tools/sillage-golden/limiter.ts`) : mêmes décisions (accordé / refusé), mêmes niveaux à 2e-5 près
//! (Sillage calcule en f64, ici en f32), mêmes couleurs après `defuseRed`.
//!
//! Écart volontaire (J2-06b) : quand une voie repart sous `BUSY_LEVEL` avec un pic demandé plus bas que son niveau
//! courant, Sillage ramène le niveau au nouveau pic en une image (chute instantanée, garantie 2 violée) ; cyber-funk
//! garde le niveau et le fait redescendre à pente bornée. Ce cas ne peut pas figurer dans le scénario de référence
//! (les sorties divergeraient par construction) : `golden_scenario_never_hits_the_retrigger_divergence` vérifie qu'il
//! en est absent, pour que la comparaison reste exacte ; le cas lui-même est testé dans `limiter_props.rs`
//! (`retrigger_below_current_level_never_drops_instantly`).

use cf_show::limiter::{Channel, FlashLimiter, FlashRequest, Rgb, defuse_red};
use serde::Deserialize;

/// Écart toléré sur un niveau de lumière (0 → 0,4) : l'erreur f32 sur ~1500 opérations reste très en dessous.
const LEVEL_TOLERANCE: f32 = 2e-5;
const COLOR_TOLERANCE: f32 = 1e-5;

#[derive(Deserialize)]
#[serde(tag = "op", rename_all = "lowercase")]
enum Op {
    Step {
        dt: f32,
    },
    Trigger {
        channel: u8,
        peak: f32,
        attack: f32,
        decay: f32,
        now: f64,
    },
    Policy {
        flashes: bool,
        breathing: bool,
    },
}

#[derive(Deserialize)]
struct Out {
    granted: Option<bool>,
    flash: f32,
    sweep: f32,
}

#[derive(Deserialize)]
struct ColorCase {
    #[serde(rename = "in")]
    input: [f32; 3],
    out: [f32; 3],
}

#[derive(Deserialize)]
struct Golden {
    ops: Vec<Op>,
    outs: Vec<Out>,
    colors: Vec<ColorCase>,
}

fn load() -> Result<Golden, serde_json::Error> {
    serde_json::from_str(include_str!("golden/limiter.json"))
}

#[test]
fn limiter_matches_sillage_scenario() -> Result<(), serde_json::Error> {
    let golden = load()?;
    assert_eq!(golden.ops.len(), golden.outs.len());
    let mut limiter = FlashLimiter::new();
    let mut grants = 0;
    for (i, (op, expected)) in golden.ops.iter().zip(&golden.outs).enumerate() {
        let granted = match *op {
            Op::Step { dt } => {
                limiter.step(dt);
                None
            }
            Op::Trigger {
                channel,
                peak,
                attack,
                decay,
                now,
            } => {
                let channel = if channel == 0 { Channel::Flash } else { Channel::Sweep };
                let request = FlashRequest {
                    channel,
                    peak,
                    attack_s: attack,
                    decay_s: decay,
                };
                Some(limiter.trigger(request, now))
            }
            Op::Policy { flashes, breathing } => {
                limiter.set_policy(flashes, breathing);
                None
            }
        };
        assert_eq!(granted, expected.granted, "op {i} : décision différente de Sillage");
        grants += usize::from(granted == Some(true));
        let flash = limiter.value(Channel::Flash);
        let sweep = limiter.value(Channel::Sweep);
        assert!(
            (flash - expected.flash).abs() <= LEVEL_TOLERANCE,
            "op {i} : flash {flash} vs {}",
            expected.flash
        );
        assert!(
            (sweep - expected.sweep).abs() <= LEVEL_TOLERANCE,
            "op {i} : vague {sweep} vs {}",
            expected.sweep
        );
    }
    // Le scénario doit exercer le limiteur pour de bon, pas seulement des refus.
    assert!(grants >= 20, "scénario trop pauvre : {grants} départs accordés");
    Ok(())
}

#[test]
fn defuse_red_matches_sillage() -> Result<(), serde_json::Error> {
    let golden = load()?;
    assert!(golden.colors.len() > 50);
    for case in &golden.colors {
        let [r, g, b] = case.input;
        let out = defuse_red(Rgb::new(r, g, b));
        let [er, eg, eb] = case.out;
        assert!(
            (out.r - er).abs() <= COLOR_TOLERANCE
                && (out.g - eg).abs() <= COLOR_TOLERANCE
                && (out.b - eb).abs() <= COLOR_TOLERANCE,
            "defuse_red({r}, {g}, {b}) = {out:?}, Sillage : {case_out:?}",
            case_out = case.out
        );
    }
    Ok(())
}

/// Écart J2-06b : aucun départ accordé du scénario ne part d'un niveau au moins égal à son pic (plafonné). Sinon Sillage
/// et cyber-funk divergeraient sur ce départ et la comparaison de `limiter_matches_sillage_scenario` ne prouverait plus
/// la fidélité du reste ; si une régénération du JSON fait apparaître ce cas, ce test le signale au lieu d'un faux écart.
#[test]
fn golden_scenario_never_hits_the_retrigger_divergence() -> Result<(), serde_json::Error> {
    let golden = load()?;
    let mut limiter = FlashLimiter::new();
    for (i, op) in golden.ops.iter().enumerate() {
        match *op {
            Op::Step { dt } => limiter.step(dt),
            Op::Trigger {
                channel,
                peak,
                attack,
                decay,
                now,
            } => {
                let channel = if channel == 0 { Channel::Flash } else { Channel::Sweep };
                let before = limiter.value(channel);
                let request = FlashRequest {
                    channel,
                    peak,
                    attack_s: attack,
                    decay_s: decay,
                };
                if limiter.trigger(request, now) {
                    let target = peak.min(channel.cap());
                    assert!(before < target, "op {i} : départ à {before} vers un pic {target} plus bas (écart J2-06b)");
                }
            }
            Op::Policy { flashes, breathing } => limiter.set_policy(flashes, breathing),
        }
    }
    Ok(())
}
