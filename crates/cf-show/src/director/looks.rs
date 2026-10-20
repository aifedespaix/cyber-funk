//! Les portes de l'image : look du cue (avec ses macros et l'intensité globale), entrée en fondu, glissement d'acte
//! vers le cue suivant, variations de l'après-set, repli sur le look de secours. Tout passe par le [`GateBlender`] :
//! les nombres glissent, jamais de saut.
//!
//! Les looks de travail sont des copies tenues à jour sur place : leurs listes ne sont recopiées qu'au changement de
//! cue (en réutilisant la mémoire), leurs nombres à chaque image (sans allocation).

use cf_control::MACRO_COUNT;

use crate::gates::{Blend, GateBlender, SMOOTH_SECONDS};
use crate::model::{LayerMix, Look, ParamPath, ShowGates};

/// Nombre de potards de macro publiés.
pub const MACROS: usize = MACRO_COUNT as usize;

/// Un potard de macro du look en cours.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct MacroKnob {
    /// Position 0..1.
    pub value: f32,
    /// Le potard a été touché depuis l'entrée du look : ses paramètres suivent la position. Sinon, le look garde ses
    /// propres valeurs et `value` montre la position qui les reproduit (de quoi reprendre le potard sans saut).
    pub engaged: bool,
}

impl MacroKnob {
    /// Potard libre, au repos.
    pub const IDLE: Self = Self {
        value: 0.0,
        engaged: false,
    };
}

/// Ce que les portes visent sur cette image.
#[derive(Debug, Clone, Copy)]
pub(crate) enum Target<'a> {
    /// Repli : le look de secours, tel quel.
    Safe(&'a Look),
    /// Le look du cue, et rien d'autre.
    Still,
    /// Glissement d'acte : vers le look du cue suivant selon l'avancement (`Blend::ACT_PROGRESS` de Sillage).
    Act(f32),
    /// Variation de l'après-set : depuis un look plus bas, mêlé au look du cue.
    Variation(&'a ShowGates, f32),
}

#[derive(Debug, Clone)]
pub(crate) struct LookMixer {
    blender: GateBlender,
    /// Portes à l'écran au moment de l'entrée du cue (départ du fondu d'entrée).
    from: ShowGates,
    from_layers: LayerMix,
    from_hue: f32,
    /// Look du cue : base + macros + intensité globale.
    cur: Look,
    /// Look du cue suivant : base + intensité globale (cible du glissement d'acte).
    next: Look,
    fade_s: f32,
    fade_t: f32,
    layers: LayerMix,
    hue: f32,
    snap: bool,
}

impl LookMixer {
    /// Un mixeur posé sur `look`, qui sautera à la cible à la première image.
    pub(crate) fn new(look: &Look) -> Self {
        Self {
            blender: GateBlender::new(),
            from: look.gates.clone(),
            from_layers: look.layers,
            from_hue: look.hue_shift_deg,
            cur: look.clone(),
            next: look.clone(),
            fade_s: 0.0,
            fade_t: 0.0,
            layers: look.layers,
            hue: look.hue_shift_deg,
            snap: true,
        }
    }

    pub(crate) fn gates(&self) -> &ShowGates {
        self.blender.gates()
    }

    pub(crate) fn layers(&self) -> LayerMix {
        self.layers
    }

    pub(crate) fn hue(&self) -> f32 {
        self.hue
    }

    /// La prochaine image saute à la cible (ouverture, reprise).
    pub(crate) fn snap(&mut self) {
        self.snap = true;
    }

    /// Un fondu d'entrée est en cours.
    pub(crate) fn entering(&self) -> bool {
        self.fade_t < self.fade_s
    }

    /// Entrée d'un cue : on part de ce qui est à l'écran ; `fade_s` > 0 pour un fondu (listes à mi-chemin), 0 pour
    /// une coupe (listes tout de suite, nombres qui glissent comme dans Sillage).
    pub(crate) fn begin_entry(&mut self, look: &Look, fade_s: f32) {
        copy_gates(&mut self.from, self.blender.gates());
        self.from_layers = self.layers;
        self.from_hue = self.hue;
        copy_gates(&mut self.cur.gates, &look.gates);
        self.fade_s = if fade_s.is_finite() { fade_s.max(0.0) } else { 0.0 };
        self.fade_t = 0.0;
    }

    /// Le cue suivant (cible du glissement d'acte), s'il y en a un.
    pub(crate) fn set_next(&mut self, look: Option<&Look>) {
        if let Some(look) = look {
            copy_gates(&mut self.next.gates, &look.gates);
        }
    }

    /// Recalcule les nombres des looks de travail : `base` avec ses macros et l'intensité globale, et le cue suivant.
    pub(crate) fn refresh(&mut self, base: &Look, knobs: &[MacroKnob; MACROS], master: f32, next: Option<&Look>) {
        write_numbers(&mut self.cur, base);
        for (m, knob) in base.macros.iter().zip(knobs) {
            if knob.engaged {
                for drive in &m.drives {
                    *self.cur.param_mut(drive.param) = drive.value_at(knob.value);
                }
            }
        }
        // Les macros peuvent pousser un paramètre d'intensité : l'intensité globale s'applique après elles.
        scale_intensity(&mut self.cur.gates, master);
        if let Some(next) = next {
            write_numbers(&mut self.next, next);
            scale_intensity(&mut self.next.gates, master);
        }
    }

    /// Fait avancer les portes, les couches et la teinte d'une image vers la cible.
    pub(crate) fn frame(&mut self, target: Target<'_>, dt: f32) {
        let dt = if dt.is_finite() { dt.max(0.0) } else { 0.0 };
        let entering = self.entering();
        if entering {
            self.fade_t += dt;
        }
        let (layers, hue) = match target {
            Target::Safe(look) => {
                self.blender
                    .update(&look.gates, &look.gates, 0.0, Blend::CUE_FADE, dt, self.snap);
                (look.layers, look.hue_shift_deg)
            }
            _ if entering => {
                let t = if self.fade_s > 0.0 {
                    (self.fade_t / self.fade_s).min(1.0)
                } else {
                    1.0
                };
                self.blender
                    .update(&self.from, &self.cur.gates, t, Blend::CUE_FADE, dt, self.snap);
                (
                    lerp_layers(self.from_layers, self.cur.layers, t),
                    lerp(self.from_hue, self.cur.hue_shift_deg, t),
                )
            }
            Target::Act(progress) => {
                self.blender.update(
                    &self.cur.gates,
                    &self.next.gates,
                    progress,
                    Blend::ACT_PROGRESS,
                    dt,
                    self.snap,
                );
                (
                    lerp_layers(self.cur.layers, self.next.layers, progress),
                    lerp(self.cur.hue_shift_deg, self.next.hue_shift_deg, progress),
                )
            }
            Target::Variation(lower, mix) => {
                self.blender
                    .update(lower, &self.cur.gates, mix, Blend::ACT_PROGRESS, dt, self.snap);
                (self.cur.layers, self.cur.hue_shift_deg)
            }
            Target::Still => {
                self.blender
                    .update(&self.cur.gates, &self.cur.gates, 0.0, Blend::CUE_FADE, dt, self.snap);
                (self.cur.layers, self.cur.hue_shift_deg)
            }
        };
        let k = if self.snap {
            1.0
        } else {
            1.0 - (-dt / SMOOTH_SECONDS).exp()
        };
        self.layers = lerp_layers(self.layers, layers, k);
        self.hue = lerp(self.hue, hue, k);
        self.snap = false;
    }
}

/// Copie les nombres pilotables de `base` dans `dst` (sans toucher aux listes).
fn write_numbers(dst: &mut Look, base: &Look) {
    for param in ParamPath::ALL {
        *dst.param_mut(param) = base.param(param);
    }
}

/// L'intensité globale (`Master`, 0..1) dose l'énergie du look : effets plein écran, envols, événements du trafic et
/// astéroïdes. À 1, le look tel qu'écrit ; à 0, un monde calme qui roule encore (allure, trafic et couches intacts).
fn scale_intensity(gates: &mut ShowGates, master: f32) {
    let m = if master.is_finite() {
        master.clamp(0.0, 1.0)
    } else {
        1.0
    };
    gates.flight_rate *= m;
    gates.event_rate *= m;
    gates.meteor_rate *= m;
    gates.fx.rgb_split *= m;
    gates.fx.flash *= m;
    gates.fx.city_pulse *= m;
}

/// Recopie toutes les portes de `src` dans `dst` en réutilisant la mémoire des listes de `dst`.
fn copy_gates(dst: &mut ShowGates, src: &ShowGates) {
    dst.max_floor = src.max_floor;
    dst.flight_rate = src.flight_rate;
    dst.flight_on_drop_only = src.flight_on_drop_only;
    dst.tricks = src.tricks;
    dst.stunt_tiers.clone_from(&src.stunt_tiers);
    dst.stunt_kinds.clone_from(&src.stunt_kinds);
    dst.stunt_gap_scale = src.stunt_gap_scale;
    dst.shot_tiers.clone_from(&src.shot_tiers);
    dst.shot_block.clone_from(&src.shot_block);
    dst.cut_grid = src.cut_grid;
    dst.salvo = src.salvo;
    dst.events.clone_from(&src.events);
    dst.event_rate = src.event_rate;
    dst.meteor_rate = src.meteor_rate;
    dst.meteor_shower = src.meteor_shower;
    dst.cruise = src.cruise;
    dst.ground_traffic = src.ground_traffic;
    dst.fx = src.fx;
    dst.ascension = src.ascension;
}

fn lerp(a: f32, b: f32, t: f32) -> f32 {
    a + (b - a) * t
}

fn lerp_layers(a: LayerMix, b: LayerMix, t: f32) -> LayerMix {
    LayerMix {
        background: lerp(a.background, b.background, t),
        world: lerp(a.world, b.world, t),
        foreground: lerp(a.foreground, b.foreground, t),
        overlay: lerp(a.overlay, b.overlay, t),
    }
}

/// La position de potard qui reproduit la valeur propre du look (premier paramètre piloté), 0..1.
pub(crate) fn rest_position(look: &Look, index: usize) -> f32 {
    let Some(drive) = look.macros.get(index).and_then(|m| m.drives.first()) else {
        return 0.0;
    };
    let span = drive.at_one - drive.at_zero;
    if span.abs() < f32::EPSILON {
        return 0.5;
    }
    let t = (look.param(drive.param) - drive.at_zero) / span;
    if t.is_finite() { t.clamp(0.0, 1.0) } else { 0.0 }
}
