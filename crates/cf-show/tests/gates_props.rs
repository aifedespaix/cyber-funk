//! Propriété de J2-02 : aucun nombre de porte ne saute de plus d'un pas d'interpolation par image.
//!
//! Le pas d'une image est `k = 1 - exp(-dt / 3 s)` de l'écart entre la valeur courante et la cible (`snap` mis à part,
//! qui est un saut demandé). Donc, pour chacun des neuf nombres : la valeur reste entre l'ancienne et la cible, ne
//! bouge pas de plus de `k` fois l'écart, et la distance à la cible ne fait que diminuer. Entre deux images
//! consécutives aucune porte ne peut donc sauter, quels que soient les looks, le mélange, les changements de cible
//! d'une image à l'autre ou la durée de l'image.

use cf_show::gates::{Blend, GateBlender, SMOOTH_SECONDS, open_gates};
use cf_show::model::{ParamPath, ShowGates};
use proptest::prelude::*;

/// Arrondis f32 sur le pas et sur le calcul de la cible.
const EPS: f32 = 1e-4;

/// Les neuf nombres d'une porte, avec la plage de leur paramètre.
fn numbers(g: &ShowGates) -> [f32; 9] {
    [
        g.flight_rate,
        g.stunt_gap_scale,
        g.event_rate,
        g.meteor_rate,
        g.cruise,
        g.ground_traffic,
        g.fx.rgb_split,
        g.fx.flash,
        g.fx.city_pulse,
    ]
}

const PARAMS: [ParamPath; 9] = [
    ParamPath::FlightRate,
    ParamPath::StuntGapScale,
    ParamPath::EventRate,
    ParamPath::MeteorRate,
    ParamPath::Cruise,
    ParamPath::GroundTraffic,
    ParamPath::FxRgbSplit,
    ParamPath::FxFlash,
    ParamPath::FxCityPulse,
];

/// Portes valides au hasard : chaque nombre dans la plage de son paramètre, les listes ne comptent pas ici.
fn gates() -> impl Strategy<Value = ShowGates> {
    let unit: Vec<_> = PARAMS
        .iter()
        .map(|p| {
            let range = p.range();
            *range.start()..=*range.end()
        })
        .collect();
    unit.prop_map(|values| {
        let mut g = open_gates();
        g.flight_rate = values[0];
        g.stunt_gap_scale = values[1];
        g.event_rate = values[2];
        g.meteor_rate = values[3];
        g.cruise = values[4];
        g.ground_traffic = values[5];
        g.fx.rgb_split = values[6];
        g.fx.flash = values[7];
        g.fx.city_pulse = values[8];
        g
    })
}

fn blend() -> impl Strategy<Value = Blend> {
    prop_oneof![Just(Blend::ACT_PROGRESS), Just(Blend::CUE_FADE)]
}

/// Une image : looks de départ et d'arrivée, mélange, durée.
#[derive(Debug, Clone)]
struct Frame {
    from: ShowGates,
    to: ShowGates,
    mix: f32,
    dt: f32,
    blend: Blend,
}

fn frame() -> impl Strategy<Value = Frame> {
    // Durées d'image de 1 ms à 10 s (trou d'image), mélange de 0 à 1.
    (gates(), gates(), 0.0f32..=1.0, 0.001f32..10.0, blend()).prop_map(|(from, to, mix, dt, blend)| Frame {
        from,
        to,
        mix,
        dt,
        blend,
    })
}

/// La cible d'un nombre, comme la calcule le blender.
fn target(frame: &Frame, index: usize) -> f32 {
    let (own, next) = (numbers(&frame.from)[index], numbers(&frame.to)[index]);
    // `flight_rate` (0) et `meteor_rate` (3) restent à zéro tant que le look de départ les met à zéro.
    if frame.blend.hold_zero && (index == 0 || index == 3) && own == 0.0 {
        0.0
    } else {
        own + (next - own) * frame.mix
    }
}

proptest! {
    /// Image après image, chaque nombre fait au plus un pas d'interpolation vers sa cible, sans la dépasser.
    #[test]
    fn prop_no_gate_number_jumps_more_than_one_interpolation_step(
        start in gates(),
        frames in prop::collection::vec(frame(), 1..60),
    ) {
        let mut blender = GateBlender::new();
        // Départ : on saute aux portes de `start` (un `snap` est une demande de saut, pas une image).
        blender.update(&start, &start, 0.0, Blend::ACT_PROGRESS, 0.0, true);
        for frame in &frames {
            let before = numbers(blender.gates());
            blender.update(&frame.from, &frame.to, frame.mix, frame.blend, frame.dt, false);
            let after = numbers(blender.gates());
            let k = 1.0 - (-frame.dt / SMOOTH_SECONDS).exp();
            for i in 0..9 {
                let goal = target(frame, i);
                let gap = goal - before[i];
                let moved = after[i] - before[i];
                // Au plus k fois l'écart…
                prop_assert!(moved.abs() <= k * gap.abs() + EPS, "nombre {i} : {moved} pour un écart {gap}, k {k}");
                // …dans le sens de la cible, sans la dépasser.
                prop_assert!(moved * gap >= -EPS, "nombre {i} s'éloigne de sa cible");
                prop_assert!((goal - after[i]).abs() <= gap.abs() + EPS, "nombre {i} s'éloigne de sa cible");
            }
        }
    }

    /// Le même pas tient pour une image courte à 60 i/s : un nombre ne bouge jamais de plus de 0,55 % de son écart.
    #[test]
    fn prop_sixty_fps_steps_stay_small(from in gates(), to in gates(), mix in 0.0f32..=1.0) {
        let mut blender = GateBlender::new();
        blender.update(&from, &from, 0.0, Blend::CUE_FADE, 0.0, true);
        let before = numbers(blender.gates());
        blender.update(&from, &to, mix, Blend::CUE_FADE, 1.0 / 60.0, false);
        let after = numbers(blender.gates());
        for i in 0..9 {
            let span = (numbers(&to)[i] - numbers(&from)[i]).abs();
            prop_assert!((after[i] - before[i]).abs() <= 0.0056 * span + EPS);
        }
    }

    /// Les listes ne sont recopiées (donc allouées) que si elles changent : à look constant, les portes sont stables.
    #[test]
    fn prop_steady_state_is_a_fixed_point(look in gates(), dt in 0.001f32..1.0) {
        let mut blender = GateBlender::new();
        blender.update(&look, &look, 0.0, Blend::CUE_FADE, 0.0, true);
        let before = blender.gates().clone();
        blender.update(&look, &look, 0.7, Blend::CUE_FADE, dt, false);
        let after = blender.gates();
        for (a, b) in numbers(&before).iter().zip(numbers(after)) {
            prop_assert!((a - b).abs() <= EPS);
        }
        prop_assert_eq!(&before.events, &after.events);
    }
}
