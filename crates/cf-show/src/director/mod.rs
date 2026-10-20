//! Le chef du spectacle (`ShowDirector` de Sillage, `engine/show/director.ts`), réécrit pour une soirée en créneaux et
//! en cues.
//!
//! Une seule struct, sans Bevy, appelée une fois par image avant tous les autres systèmes :
//!
//! ```text
//! ShowInput { dt, now, heure locale, AudioFeatures, drop réel, actions }
//!   └─▶ ShowDirector::update ─▶ &ShowState (mis à jour sur place, lu par tous)
//!                             └▶ drain_events() : ShowEvent pour l'interface et le binaire
//! ```
//!
//! Machine à états de la conduite (par créneau) :
//!
//! ```text
//!            GO, échéance (Auto/Semi, sans Hold ni repli)       entrée prête : Cut → temps fort,
//!   cue N ─────────────────────────────────────▶ appelé N+1 ─── Fade → tout de suite, OnPhrase → phrase,
//!     ▲                                           │  ▲           OnDrop → drop, puis phrase après MAX_LATE,
//!     │            second GO : tout de suite ──────┘  │           puis tout de suite
//!     └──────────────── entre (triggers, portes) ◀────┘
//!   Back / GotoCue : entrée immédiate.  Hold, SafeLook : l'avancée automatique se fige.
//!
//!   NextDj / PrevDj : temps fort ─▶ fondu de palette d'une mesure ─▶ appel du cue 0 du créneau suivant
//!                     (Handover)                                   ─▶ à son entrée : SlotLoaded, profils
//! ```
//!
//! Les modes (`docs/ARCHITECTURE.md` §2.3) : en Auto et en Semi, les règles d'avancée appellent le cue suivant quand
//! personne ne l'a fait ; un GO avant l'échéance appelle le suivant et réarme son propre chronomètre. Le chef du
//! spectacle ne refuse jamais un ordre de l'opérateur (comme Sillage, où les flèches marchaient toujours) : Auto et
//! Semi ne diffèrent donc que par ce que l'interface met en avant. En Manuel, seules les actions font avancer.
//!
//! Tout aléatoire vient de RNG dérivés de la graine (`seed`) : même graine, mêmes entrées, même spectacle. Aucune
//! allocation par image en régime établi (test `director_no_alloc`) : tout ce qui alloue est préparé à l'ouverture.

mod actions;
mod clock;
mod conduct;
mod event;
mod fade;
mod input;
mod looks;
mod orders;
mod prepared;
mod state;

use std::fmt;
use std::sync::Arc;

use cf_analysis::AudioFeatures;
use rand_core::{Rng, SeedableRng};

pub use clock::{
    DOWNBEAT_EVERY, DOWNBEAT_MAX_WAIT, FALLBACK_BPM, MAX_LATE, NO_BEAT_FALLBACK, PHRASE_BEATS, PendingCue, bar_seconds,
};
pub use event::{CueCause, ShowEvent, ShowToggle};
pub use fade::{BLACKOUT_FADE, Fade, OPERATOR_FADE};
pub use input::ShowInput;
pub use looks::{MACROS, MacroKnob};
pub use orders::{ORDER_TTL, Order, Orders, TRIGGER_QUEUE};
pub use prepared::{DirectorError, look_level};
pub use state::{Handover, HandoverPhase, ShowState};

use self::clock::{Beats, CueClock};
use self::looks::{LookMixer, Target};
use self::prepared::Prepared;
use crate::drop::{DropConductor, DropInput, DropPlan, DropTitles};
use crate::freshness::Freshness;
use crate::gates::Variation;
use crate::load::LoadedShow;
use crate::model::{FlashPolicy, LayerMix, SlotMode};
use crate::time_scale::TimeScale;

/// Constante de temps (s) de la chaleur (`HEAT_SECONDS` de Sillage).
const HEAT_SECONDS: f32 = 5.0;
/// Les variations de l'après-set commencent quand le dernier cue d'un créneau Auto ou Semi joue depuis ce temps (s).
/// Dans Sillage, elles partent à la fin du set, soit 45 % d'un set de 180 min après l'entrée du niveau 5.
pub const VARIATION_AFTER_S: f64 = 4860.0;
/// Événements publiés entre deux vidanges sans allocation.
const EVENT_CAPACITY: usize = 64;
/// Décalage de graine du RNG des variations (celui du drop prend la graine telle quelle).
const VARIATION_STREAM: u64 = 0x9E37_79B9_7F4A_7C15;

/// Tirage uniforme dans [0, 1[ des variations : un RNG à graine enfermé dans une fermeture (boîte allouée une fois).
type UnitRandom = Box<dyn FnMut() -> f64 + Send + Sync>;

/// Ce que l'image en cours sait de la musique et de l'horloge (copie des entrées, pour les étapes de l'image).
#[derive(Debug, Clone, Copy)]
struct Frame {
    dt: f32,
    now: f64,
    real_drop: bool,
    audio: AudioFeatures,
}

/// Le chef du spectacle. `R` : générateur à graine (`rand_pcg::Pcg32` par exemple).
pub struct ShowDirector<R> {
    state: ShowState,
    prepared: Prepared,
    titles: DropTitles,
    conductor: DropConductor<R>,
    freshness: Freshness,
    variation: Variation<UnitRandom>,
    scale: TimeScale,
    /// Chronomètre du cue à l'écran.
    clock: CueClock,
    /// Chronomètre du cue appelé, pendant son attente d'entrée (il devient `clock` à l'entrée).
    called: CueClock,
    events: Vec<ShowEvent>,
    seed: u64,
    frame: Frame,
    force_drop: bool,
    phrase: Option<u32>,
    master: Fade,
    blackout: Fade,
    /// Fondus de l'opérateur par couche : fond, monde, avant-plan, surcouche.
    layer_fades: [Fade; 4],
    heat_snap: bool,
}

impl<R> fmt::Debug for ShowDirector<R> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ShowDirector")
            .field("state", &self.state)
            .field("seed", &self.seed)
            .field("variation", &self.variation)
            .finish_non_exhaustive()
    }
}

impl<R: Rng + SeedableRng + Send + Sync + 'static> ShowDirector<R> {
    /// Prépare le spectacle d'un événement validé et entre dans le premier cue du premier créneau.
    ///
    /// `titles` : les cartons-titres des drops (et de l'action `Title`) ; `seed` : graine de tout l'aléatoire, à
    /// garder dans la sauvegarde. Les événements d'ouverture (`SlotLoaded`, `CueEntered`) attendent la première
    /// vidange.
    ///
    /// # Errors
    ///
    /// [`DirectorError`] si l'événement n'a pas de créneau, un créneau pas de cue, ou si un look manque (impossible
    /// avec un [`LoadedShow`] validé).
    pub fn new(show: &LoadedShow, titles: DropTitles, seed: u64) -> Result<Self, DirectorError> {
        let prepared = Prepared::new(show)?;
        let slot = prepared.slots.first().ok_or(DirectorError::NoSlot)?;
        let first = slot.cues.first().ok_or(DirectorError::EmptySlot { slot: 0 })?;
        let conductor = DropConductor::new(R::seed_from_u64(seed), titles.clone());
        let mut freshness = Freshness::default();
        conductor.register_known(&mut freshness);
        let mut variation_rng = R::seed_from_u64(seed ^ VARIATION_STREAM);
        let random: UnitRandom = Box::new(move || unit(&mut variation_rng));
        let state = ShowState {
            slot: 0,
            slot_seq: 0,
            mode: slot.mode,
            armed_slot: None,
            handover: None,
            cue: 0,
            cue_seq: 0,
            pending: None,
            cue_elapsed: 0.0,
            cue_progress: None,
            next_cue_in: None,
            elapsed: 0.0,
            hold: false,
            level: first.level,
            heat: 0.0,
            macros: [MacroKnob::IDLE; MACROS],
            master: 1.0,
            layers: first.look.layers,
            hue_shift_deg: first.look.hue_shift_deg,
            theme: first.theme.clone().unwrap_or_else(|| Arc::clone(&slot.theme)),
            theme_seq: 0,
            theme_fade_s: 0.0,
            blackout: 0.0,
            blackout_on: false,
            safe_look: false,
            flashes: show.event().settings.flashes == FlashPolicy::Limited,
            breathing: false,
            drop_now: false,
            time_scale: 1.0,
            orders: Orders::default(),
            dj_show_seq: 0,
            dj_logo_seq: 0,
            mixer: LookMixer::new(&first.look),
        };
        let mut director = Self {
            state,
            prepared,
            titles,
            conductor,
            freshness,
            variation: Variation::new(random),
            scale: TimeScale::default(),
            clock: CueClock::default(),
            called: CueClock::default(),
            events: Vec::with_capacity(EVENT_CAPACITY),
            seed,
            frame: Frame {
                dt: 0.0,
                now: 0.0,
                real_drop: false,
                audio: AudioFeatures::NEUTRAL,
            },
            force_drop: false,
            phrase: None,
            master: Fade::new(1.0, OPERATOR_FADE),
            blackout: Fade::new(0.0, BLACKOUT_FADE),
            layer_fades: [Fade::new(1.0, OPERATOR_FADE); 4],
            heat_snap: true,
        };
        director.events.push(ShowEvent::SlotLoaded { slot: 0, previous: 0 });
        director.call_now(0, 0, CueCause::Open);
        Ok(director)
    }
}

impl<R: Rng> ShowDirector<R> {
    /// Avance le spectacle d'une image et rend l'état, à lire par tous les systèmes de l'image.
    pub fn update(&mut self, input: &ShowInput<'_>) -> &ShowState {
        let dt = if input.dt.is_finite() { input.dt.max(0.0) } else { 0.0 };
        self.frame = Frame {
            dt,
            now: input.now,
            real_drop: input.real_drop,
            audio: *input.audio,
        };
        let audio = &self.frame.audio;
        let phrase = audio.count / PHRASE_BEATS;
        // Comme Sillage (phrase initiale à -1), la première image compte comme un début de phrase.
        let phrase_start = self.phrase != Some(phrase);
        self.phrase = Some(phrase);
        let downbeat = audio.kick && audio.count.is_multiple_of(DOWNBEAT_EVERY);
        let live = audio.live;

        self.state.orders.tick(dt);
        if let Some(pending) = &mut self.state.pending {
            pending.waited += dt;
        }
        for action in input.actions {
            self.apply(action);
        }
        self.step_fades(dt);
        self.state.elapsed += f64::from(dt);
        self.clock.tick(dt, &self.frame.audio, input.local_time_s);
        if self.state.pending.is_some() {
            self.called.tick(dt, &self.frame.audio, input.local_time_s);
        }

        self.run_conductor();
        let beats = Beats {
            downbeat,
            phrase_start,
            live,
            drop_now: self.state.drop_now,
        };
        let cue_seq = self.state.cue_seq;
        self.step_handover(beats);
        self.auto_advance(beats);
        self.step_pending(beats);
        self.derive(dt);
        // Une entrée de cue sur ce drop a ses propres cartons : pas de second titre par-dessus (comme Sillage).
        if self.state.drop_now
            && self.state.cue_seq == cue_seq
            && let Some(title) = self.conductor.plan().and_then(|plan| plan.title.clone())
        {
            self.events.push(ShowEvent::Title(title));
        }
        self.state.time_scale = self.scale.update(dt);
        &self.state
    }

    /// L'état publié par la dernière image.
    #[must_use]
    pub fn state(&self) -> &ShowState {
        &self.state
    }

    /// Les événements publiés depuis la dernière vidange, dans l'ordre. À vider à chaque image : la file réutilise sa
    /// mémoire et n'alloue pas tant qu'elle reste sous sa capacité.
    pub fn drain_events(&mut self) -> std::vec::Drain<'_, ShowEvent> {
        self.events.drain(..)
    }

    /// Vrai une seule fois après un ordre encore vivant (`take` de Sillage), à appeler par le système concerné.
    pub fn take(&mut self, order: Order) -> bool {
        self.state.orders.take(order)
    }

    /// Prend le plus ancien déclencheur du monde en attente (`Trigger`), à appeler par le système du trafic.
    pub fn take_trigger(&mut self) -> Option<cf_control::TriggerId> {
        self.state.orders.take_trigger()
    }

    /// Le drop préparé pendant une montée, lisible jusqu'à l'image où il frappe (`ShowState.drop` de Sillage).
    #[must_use]
    pub fn drop_plan(&self) -> Option<&DropPlan> {
        self.conductor.plan()
    }

    /// Mémoire d'anti-répétition commune (`shot:`, `stunt:`, `event:`, `landmark:`).
    #[must_use]
    pub fn freshness(&self) -> &Freshness {
        &self.freshness
    }

    /// Accès en écriture à la fraîcheur : `mark` quand un système joue un élément.
    pub fn freshness_mut(&mut self) -> &mut Freshness {
        &mut self.freshness
    }

    /// Ralentit le monde à `scale` (0,1 → 1) pendant `seconds`, puis revient progressivement à 1.
    pub fn request_time_scale(&mut self, scale: f32, seconds: f32) {
        self.scale.request(scale, seconds);
    }

    /// Graine de tout l'aléatoire du spectacle (à garder dans la sauvegarde).
    #[must_use]
    pub fn seed(&self) -> u64 {
        self.seed
    }

    /// Fondus de sécurité et de l'opérateur : ils suivent leur consigne à vitesse bornée.
    fn step_fades(&mut self, dt: f32) {
        self.blackout.set_target(if self.state.blackout_on { 1.0 } else { 0.0 });
        self.state.blackout = self.blackout.step(dt);
        self.state.master = self.master.step(dt);
        for fade in &mut self.layer_fades {
            fade.step(dt);
        }
    }

    /// Le chef d'orchestre du drop : repère la montée, prépare, frappe.
    fn run_conductor(&mut self) {
        let forced = std::mem::take(&mut self.force_drop);
        let input = drop_input(&self.state, &self.frame, &self.prepared, forced);
        self.conductor.update(&mut self.freshness, &input);
        self.state.drop_now = self.conductor.drop_now();
    }

    /// Avancement, chaleur, portes, couches et teinte de l'image.
    fn derive(&mut self, dt: f32) {
        let s = &mut self.state;
        let audio = &self.frame.audio;
        let slot = &self.prepared.slots[s.slot];
        let cue = &slot.cues[s.cue];
        let next = slot.cues.get(s.cue + 1);
        let manual = s.mode == SlotMode::Manual;
        let waiting = s.pending.is_some();
        s.cue_elapsed = self.clock.elapsed;
        s.cue_progress = self.clock.progress(audio);
        s.next_cue_in = if waiting || manual || next.is_none() {
            None
        } else {
            self.clock.remaining(audio)
        };
        // Glissement d'acte de Sillage : vers le look du cue suivant au fil de l'échéance (règles à échéance seules).
        let act = s.cue_progress.filter(|_| !manual && !s.safe_look && next.is_some());
        s.level = if s.safe_look { self.prepared.safe_level } else { cue.level };
        let target_heat = if s.level == 0 {
            0.0
        } else {
            (f32::from(s.level) - 1.0 + act.unwrap_or(0.0)) / 5.0
        };
        let k = if self.heat_snap {
            1.0
        } else {
            1.0 - (-dt / HEAT_SECONDS).exp()
        };
        s.heat += (target_heat - s.heat) * k;
        self.heat_snap = false;

        let after_set = !manual
            && !s.safe_look
            && !waiting
            && next.is_none()
            && s.cue > 0
            && self.clock.elapsed >= VARIATION_AFTER_S;
        let varied = self.variation.update(f64::from(dt), after_set) && self.variation.uses_lower();
        s.mixer.refresh(&cue.look, &s.macros, s.master, next.map(|c| &c.look));
        let target = if s.safe_look {
            Target::Safe(&self.prepared.safe)
        } else if let Some(progress) = act {
            Target::Act(progress)
        } else if varied {
            Target::Variation(&slot.cues[s.cue - 1].look.gates, self.variation.mix())
        } else {
            Target::Still
        };
        s.mixer.frame(target, dt);
        let l = s.mixer.layers();
        let [bg, world, fg, overlay] = &self.layer_fades;
        s.layers = LayerMix {
            background: l.background * bg.value(),
            world: l.world * world.value(),
            foreground: l.foreground * fg.value(),
            overlay: l.overlay * overlay.value(),
        };
        s.hue_shift_deg = s.mixer.hue();
    }
}

/// Les entrées du chef d'orchestre du drop pour l'image en cours.
fn drop_input<'a>(state: &'a ShowState, frame: &Frame, prepared: &Prepared, forced: bool) -> DropInput<'a> {
    DropInput {
        dt: frame.dt,
        now: frame.now,
        intensity: frame.audio.intensity,
        real_drop: frame.real_drop,
        forced,
        gates: state.gates(),
        heat: state.heat,
        level: state.level,
        flashes: state.flashes && !state.breathing,
        language: prepared.language,
    }
}

/// Réel uniforme dans [0, 1[, 53 bits (`Math.random` de Sillage, avec la graine du show).
fn unit<R: Rng>(rng: &mut R) -> f64 {
    (rng.next_u64() >> 11) as f64 / (1u64 << 53) as f64
}
