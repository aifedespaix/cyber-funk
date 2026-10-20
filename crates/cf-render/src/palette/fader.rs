//! Le calendrier d'un changement de thème (`ThemeFader` de `engine/show/dj-transition.ts`) : le fondu attend un temps
//! fort, une vague de lumière part au milieu, puis on prévient que la palette est posée. Logique pure : le fader ne
//! connaît pas les couleurs, il dit *quand* démarrer, *combien de temps*, et quand la vague part.

/// Durée mini et maxi du fondu, en secondes (seize temps, borné).
pub const MIN_FADE: f32 = 6.0;
/// Voir [`MIN_FADE`].
pub const MAX_FADE: f32 = 10.0;
/// Durée sans tempo connu, en secondes.
pub const DEFAULT_FADE: f32 = 8.0;
/// On attend au plus ce temps (s) un temps fort avant de lancer le fondu quand même.
pub const MAX_WAIT: f32 = 4.0;
/// Un temps fort : un kick sur quatre.
pub const DOWNBEAT_EVERY: u32 = 4;
/// Moment du fondu (0 → 1) où part la vague de lumière.
pub const WAVE_AT: f32 = 0.5;

/// Ce que le fader lit à chaque image.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct FadeFrame {
    /// Durée de l'image, en secondes.
    pub dt_s: f32,
    /// Un kick dans cette image.
    pub kick: bool,
    /// Nombre de kicks depuis le début (pour repérer un temps fort).
    pub count: u32,
    /// Tempo, `None` tant qu'il n'est pas établi.
    pub bpm: Option<f32>,
}

/// Ce que le fader demande pour cette image.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct FadeStep {
    /// Lancer le fondu de la palette vers le thème demandé, sur cette durée (s).
    pub begin_s: Option<f32>,
    /// Lancer la vague de lumière (milieu du fondu).
    pub wave: bool,
    /// Le fondu est fini : la palette est celle du thème demandé.
    pub settled: bool,
}

/// Durée d'un fondu de seize temps, bornée (`fadeSeconds`).
#[must_use]
pub fn fade_seconds(bpm: Option<f32>) -> f32 {
    match bpm {
        Some(bpm) if bpm > 0.0 => (16.0 * 60.0 / bpm).clamp(MIN_FADE, MAX_FADE),
        _ => DEFAULT_FADE,
    }
}

/// Le calendrier d'un fondu de thème. Les quatre drapeaux sont ceux de `ThemeFader` dans Sillage (attente, fondu, vague
/// faite, silence) : ils se combinent librement, une énumération n'en rendrait pas mieux compte.
#[allow(clippy::struct_excessive_bools)]
#[derive(Debug, Clone, Default)]
pub struct ThemeFader {
    /// Une demande attend son temps fort.
    pending: bool,
    waited_s: f32,
    /// Un fondu lancé par nous est en cours.
    fading: bool,
    wave_done: bool,
    /// Entrée ou sortie du cinéma, ou passation menée par le chef du spectacle : pas de vague.
    quiet: bool,
}

impl ThemeFader {
    /// Un fondu (ou son attente) est en cours.
    #[must_use]
    pub fn busy(&self) -> bool {
        self.pending || self.fading
    }

    /// Demande un changement de thème qui attend un temps fort (au plus [`MAX_WAIT`] s), puis dure
    /// [`fade_seconds`] (`setLiveTheme(theme)`).
    pub fn request_on_downbeat(&mut self) {
        self.waited_s = 0.0;
        self.quiet = false;
        self.pending = true;
    }

    /// Lance le fondu tout de suite, `seconds` (secondes) : le chef du spectacle a déjà attendu son temps fort et
    /// donne la durée (une mesure), ou entrée/sortie du cinéma (`setLiveTheme(theme, true)` : `quiet`, sans vague).
    /// Renvoie la durée à donner au fondu.
    pub fn begin_now(&mut self, seconds: f32, quiet: bool) -> f32 {
        self.pending = false;
        self.waited_s = 0.0;
        self.quiet = quiet;
        self.wave_done = quiet;
        self.fading = true;
        seconds
    }

    /// Avance d'une image. `progress` et `morphing` sont ceux du fondu de la palette ([`super::PaletteMorph`]).
    ///
    /// À l'image où le fondu démarre, on s'arrête là : le fondu n'a pas encore été lancé côté palette (`morphing`
    /// serait faux), il ne faut pas le prendre pour fini.
    pub fn update(&mut self, frame: FadeFrame, progress: f32, morphing: bool) -> FadeStep {
        let mut step = FadeStep::default();
        if self.pending {
            self.waited_s += frame.dt_s;
            if (frame.kick && frame.count.is_multiple_of(DOWNBEAT_EVERY)) || self.waited_s >= MAX_WAIT {
                self.pending = false;
                self.wave_done = self.quiet;
                self.fading = true;
                step.begin_s = Some(fade_seconds(frame.bpm));
                return step;
            }
        }
        if !self.fading {
            return step;
        }
        if !self.wave_done && progress >= WAVE_AT {
            self.wave_done = true;
            step.wave = true;
        }
        if !morphing {
            self.fading = false;
            step.settled = true;
        }
        step
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn frame(kick: bool, count: u32) -> FadeFrame {
        FadeFrame {
            dt_s: 0.5,
            kick,
            count,
            bpm: Some(128.0),
        }
    }

    #[test]
    fn la_duree_suit_le_tempo_et_reste_bornee() {
        assert!((fade_seconds(Some(128.0)) - 7.5).abs() < 1e-5);
        assert!((fade_seconds(Some(200.0)) - MIN_FADE).abs() < 1e-5);
        assert!((fade_seconds(Some(60.0)) - MAX_FADE).abs() < 1e-5);
        assert!((fade_seconds(None) - DEFAULT_FADE).abs() < 1e-5);
        assert!((fade_seconds(Some(0.0)) - DEFAULT_FADE).abs() < 1e-5);
    }

    #[test]
    fn la_demande_attend_un_temps_fort() {
        let mut fader = ThemeFader::default();
        fader.request_on_downbeat();
        // Kicks 1, 2, 3 : pas un temps fort.
        for count in 1..=3 {
            assert_eq!(fader.update(frame(true, count), 1.0, false).begin_s, None);
        }
        // Le quatrième.
        let step = fader.update(frame(true, 4), 1.0, false);
        assert_eq!(step.begin_s, Some(7.5));
        assert!(!step.settled, "l'image du départ ne peut pas être celle de la fin");
    }

    #[test]
    fn sans_kick_on_attend_au_plus_quatre_secondes() {
        let mut fader = ThemeFader::default();
        fader.request_on_downbeat();
        let steps: Vec<_> = (0..8).map(|_| fader.update(frame(false, 0), 1.0, false)).collect();
        // dt = 0,5 s : le départ à la huitième image (4 s).
        assert!(steps[..7].iter().all(|s| s.begin_s.is_none()));
        assert!(steps[7].begin_s.is_some());
    }

    #[test]
    fn la_vague_part_une_fois_au_milieu_puis_on_signale_la_fin() {
        let mut fader = ThemeFader::default();
        let seconds = fader.begin_now(1.875, false);
        assert!((seconds - 1.875).abs() < 1e-6);
        assert!(fader.busy());
        let early = fader.update(frame(false, 0), 0.3, true);
        assert!(!early.wave);
        let middle = fader.update(frame(false, 0), 0.55, true);
        assert!(middle.wave);
        assert!(!fader.update(frame(false, 0), 0.9, true).wave, "une seule vague");
        let end = fader.update(frame(false, 0), 1.0, false);
        assert!(end.settled);
        assert!(!fader.busy());
    }

    #[test]
    fn entree_silencieuse_sans_vague() {
        let mut fader = ThemeFader::default();
        fader.begin_now(2.0, true);
        assert!(!fader.update(frame(false, 0), 0.7, true).wave);
        assert!(fader.update(frame(false, 0), 1.0, false).settled);
    }
}
