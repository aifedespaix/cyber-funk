//! Palettes et fondu de thème (portage de `engine/palette.ts`, `engine/live-palette.ts`, `ThemeFader` de
//! `engine/show/dj-transition.ts` et de la teinte `mood` de `engine/sections.ts`).
//!
//! ```text
//! themes.ron (6 thèmes)  ┐
//! CustomTheme (5 → 11)   ┴▶ ThemeLibrary ──▶ PaletteMorph (fondu sur place, par la teinte quand c'est vif)
//!
//! CurrentShow.theme / theme_seq / theme_fade_s ─┐  le chef du spectacle a déjà attendu son temps fort : le fondu
//! PaletteRequest (opérateur, cinéma)            ┴▶ démarre tout de suite, une mesure (ou attend un temps fort)
//!                                                  ThemeFader : quand démarrer, vague au milieu, fin
//!                         Res<ScenePalette> = palette du fondu tournée de `hue_shift_deg` (CfSet::Present)
//!                         Res<SceneMood>    = azure → violet → rose selon `AudioFeatures::intensity`
//! ```
//!
//! Qui écrit quoi : le seul système [`drive_palette`] écrit [`ScenePalette`] ; [`update_mood`] écrit [`SceneMood`].
//! Les autres modules lisent, et recalculent leurs couleurs dérivées quand `Res<ScenePalette>` a changé.

mod color;
mod fader;
mod mood;
mod morph;
mod scene;

use bevy::prelude::*;
use cf_show::model::Theme;

pub use color::{Hsl, LinearRgb, mix_srgb8, parse_hex};
pub use fader::{
    DEFAULT_FADE, DOWNBEAT_EVERY, FadeFrame, FadeStep, MAX_FADE, MAX_WAIT, MIN_FADE, ThemeFader, WAVE_AT, fade_seconds,
};
pub use mood::{NEUTRAL_INTENSITY, SceneMood, mood_color};
pub use morph::{CHANGE_INTERVAL, MorphEvent, PaletteMorph};
pub use scene::{PRESETS, PaletteError, SceneColor, ScenePalette, ThemeLibrary, preset_name};

use crate::CfSet;
use crate::audio::AudioFeatures;
use crate::show::CurrentShow;

/// Quand un changement de thème demandé par l'opérateur démarre.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PaletteTiming {
    /// Sur le prochain temps fort (au plus [`MAX_WAIT`] s d'attente), en [`fade_seconds`] (seize temps).
    Downbeat,
    /// Tout de suite, en [`DEFAULT_FADE`] s et sans vague de lumière (entrée ou sortie du cinéma).
    Now,
}

/// Demande de thème hors spectacle (l'opérateur, l'entrée du cinéma). Pendant un show, le thème vient du chef du
/// spectacle (`CurrentShow`) : passation de DJ, cue, surcharge de palette du look.
#[derive(Message, Debug, Clone, PartialEq)]
pub struct PaletteRequest {
    /// Le thème voulu.
    pub theme: Theme,
    /// Quand démarrer.
    pub timing: PaletteTiming,
}

/// Le fondu avance ou vient de finir : à lire pour les rafraîchissements coûteux (textures peintes, couleurs cuites
/// dans des sommets) qui ne suivent pas `Res<ScenePalette>` à chaque image (`watchPalette` de Sillage).
#[derive(Message, Debug, Clone, Copy, PartialEq, Eq)]
pub struct PaletteChanged {
    /// Vrai une dernière fois, quand la palette est exactement la cible.
    pub settled: bool,
}

/// Le fondu est à mi-chemin : la vague de lumière du post-process peut partir (elle passe par le limiteur).
#[derive(Message, Debug, Clone, Copy, PartialEq, Eq)]
pub struct PaletteWave;

/// L'état du fondu : la palette qui glisse, le calendrier, le thème attendu. Écrit par le seul [`drive_palette`].
#[derive(Resource, Debug)]
pub struct PaletteDriver {
    morph: PaletteMorph,
    fader: ThemeFader,
    /// Thème vers lequel on va (ou qu'on attend d'un temps fort).
    requested: Option<Theme>,
    /// Dernier `theme_seq` du show pris en compte (`None` : pas encore vu).
    shown_seq: Option<u32>,
}

impl PaletteDriver {
    /// Un fondu est en cours ou attend son temps fort.
    #[must_use]
    pub fn busy(&self) -> bool {
        self.fader.busy()
    }

    /// Avancement du fondu, 0 → 1 (1 au repos).
    #[must_use]
    pub fn progress(&self) -> f32 {
        self.morph.progress()
    }
}

/// Branche les palettes : thèmes livrés, fondu, teinte d'ambiance.
///
/// À ajouter après `CorePlugin`. `CfAudioPlugin` et `ShowPlugin` sont facultatifs : sans eux, pas de temps fort ni de
/// thème de show, mais les demandes [`PaletteRequest`] fonctionnent (attente d'au plus [`MAX_WAIT`] s).
#[derive(Debug, Default)]
pub struct PalettePlugin;

impl Plugin for PalettePlugin {
    fn build(&self, app: &mut App) {
        let library = ThemeLibrary::builtin().unwrap_or_else(|error| {
            // Le fichier est compilé dans le binaire et testé : ce chemin n'existe qu'en cas de bug. Sans thèmes, la
            // palette reste noire plutôt que de planter le projecteur.
            tracing::error!(%error, "palettes : thèmes livrés illisibles");
            ThemeLibrary::default()
        });
        let start = *library.preset(cf_show::model::PresetTheme::Velvet);
        app.add_message::<PaletteRequest>()
            .add_message::<PaletteChanged>()
            .add_message::<PaletteWave>()
            .insert_resource(library)
            .insert_resource(start)
            .insert_resource(PaletteDriver {
                morph: PaletteMorph::new(start),
                fader: ThemeFader::default(),
                requested: None,
                shown_seq: None,
            })
            .init_resource::<SceneMood>()
            .add_systems(Update, (drive_palette, update_mood).chain().in_set(CfSet::Present));
    }
}

/// Lance le fondu vers `theme`, `seconds` (secondes).
fn start_morph(driver: &mut PaletteDriver, library: &ThemeLibrary, theme: &Theme, seconds: f32) -> MorphEvent {
    driver.morph.morph_to(&library.resolve(theme), seconds)
}

/// Le système unique des palettes : prend le thème du show et les demandes, avance le calendrier puis le fondu, et
/// publie [`ScenePalette`] (fondu + rotation de teinte du look).
#[allow(clippy::too_many_arguments)] // Les paramètres d'un système Bevy : ressources et files de messages.
pub fn drive_palette(
    time: Res<Time>,
    features: Option<Res<AudioFeatures>>,
    show: Option<Res<CurrentShow>>,
    library: Res<ThemeLibrary>,
    mut driver: ResMut<PaletteDriver>,
    mut requests: MessageReader<PaletteRequest>,
    mut palette: ResMut<ScenePalette>,
    mut changed: MessageWriter<PaletteChanged>,
    mut wave: MessageWriter<PaletteWave>,
) {
    let dt_s = time.delta_secs();
    let driver = &mut *driver;
    let mut settled_now = false;

    // 1. Le thème du show. La première fois, on le pose sans fondu (ouverture : pas de palette qui glisse depuis le
    //    thème de départ) ; ensuite chaque changement est un fondu de `theme_fade_s`, jamais une coupe.
    if let Some(show) = show.as_deref()
        && driver.shown_seq != Some(show.theme_seq)
    {
        let first = driver.shown_seq.is_none();
        driver.shown_seq = Some(show.theme_seq);
        let theme = (*show.theme).clone();
        let seconds = if first {
            0.0
        } else {
            driver.fader.begin_now(show.theme_fade_s, false)
        };
        settled_now |= start_morph(driver, &library, &theme, seconds) == MorphEvent::Settled;
        driver.requested = Some(theme);
    }

    // 2. Les demandes de l'opérateur ou du cinéma.
    for request in requests.read() {
        driver.requested = Some(request.theme.clone());
        match request.timing {
            PaletteTiming::Downbeat => driver.fader.request_on_downbeat(),
            PaletteTiming::Now => {
                let seconds = driver.fader.begin_now(DEFAULT_FADE, true);
                settled_now |= start_morph(driver, &library, &request.theme, seconds) == MorphEvent::Settled;
            }
        }
    }

    // 3. Le calendrier : départ sur un temps fort, vague au milieu, fin.
    let frame = features.as_deref().map_or(
        FadeFrame {
            dt_s,
            kick: false,
            count: 0,
            bpm: None,
        },
        |f| FadeFrame {
            dt_s,
            kick: f.kick,
            count: f.count,
            bpm: f.bpm,
        },
    );
    let step = driver
        .fader
        .update(frame, driver.morph.progress(), driver.morph.morphing());
    if let Some(seconds) = step.begin_s
        && let Some(theme) = driver.requested.clone()
    {
        settled_now |= start_morph(driver, &library, &theme, seconds) == MorphEvent::Settled;
    }
    if step.wave {
        wave.write(PaletteWave);
    }

    // 4. Le fondu de la palette.
    match driver.morph.update(dt_s) {
        MorphEvent::None => {}
        MorphEvent::Changed => {
            changed.write(PaletteChanged { settled: false });
        }
        MorphEvent::Settled => settled_now = true,
    }
    if settled_now {
        changed.write(PaletteChanged { settled: true });
    }

    // 5. La palette à l'écran : le fondu, tourné de la teinte du look.
    let hue = show.as_deref().map_or(0.0, |s| s.hue_shift_deg);
    palette.set_if_neq(driver.morph.palette().hue_rotated(hue));
}

/// La teinte d'ambiance de l'image : suit la palette à l'écran et l'intensité du morceau.
pub fn update_mood(palette: Res<ScenePalette>, features: Option<Res<AudioFeatures>>, mut mood: ResMut<SceneMood>) {
    let intensity = features.as_deref().map_or(NEUTRAL_INTENSITY, |f| f.intensity);
    mood.set_if_neq(SceneMood {
        color: mood_color(&palette, intensity),
        intensity,
    });
}
