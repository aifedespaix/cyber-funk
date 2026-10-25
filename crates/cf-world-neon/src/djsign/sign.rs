//! L'enseigne elle-même (`dj-sign.ts`) : la machine à états des lettres. Les lettres s'allument une à une sur les kicks,
//! une vague de lumière les parcourt sur la phrase, une lettre grésille de temps en temps ; un nouveau nom fait tomber
//! les anciennes lettres avant d'allumer les nouvelles.
//!
//! Pure : elle ne touche ni maillage ni texture. Elle publie, par lettre, une pose et un niveau de lumière
//! ([`LetterPose`]) que la couche Bevy recopie, et un compteur de reconstructions ([`DjSign::layout_seq`]) qui dit quand
//! la mise en page (donc l'ossature et les glyphes) a changé.
//!
//! Repère local : x le long du panneau, y vers le haut, la face lisible regarde +z (vers la voiture qui arrive).

use super::anim::{FALL_SECONDS, FALL_STAGGER, MIN_IGNITE_GAP_S, fall_state, ignite_level, sizzle, slew, wave_gain};
use super::frame::LEG_HEIGHT;
use super::layout::{SIGN_LAYOUT, SignLayout, layout_sign};

/// Sans kick (silence, tempo inconnu), une lettre s'allume toutes les 0,64 s (`0,4 × 1,6` de Sillage).
const FALLBACK_IGNITE_S: f32 = 0.4 * 1.6;
/// Marges du plan d'une lettre, en hauteurs de capitale (`PAD_*` de `dj-sign-glyph.ts`) : elles doivent être celles de
/// la texture cuite, le halo y déborde.
pub const PAD_X: f32 = 0.34;
/// Marge au-dessus de la capitale.
pub const PAD_TOP: f32 = 0.6;
/// Marge sous la ligne de base.
pub const PAD_BOTTOM: f32 = 0.34;
/// Graine du grésillement (le `7` de Sillage).
const SIZZLE_SEED: f64 = 7.0;
/// Tempo supposé sans tempo connu.
const DEFAULT_BPM: f32 = 120.0;

/// Mesure de l'avance d'un glyphe, en hauteurs de capitale.
pub type Measure = Box<dyn Fn(char) -> f32 + Send + Sync>;

/// Ce que l'enseigne reçoit à chaque image.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SignInput {
    /// Durée de l'image, s.
    pub dt: f32,
    /// Un kick vient de frapper.
    pub kick: bool,
    /// Pouls du tempo, 0 → 1.
    pub pulse: f32,
    /// Tempo connu, battements par minute.
    pub bpm: Option<f32>,
    /// Part des lettres qu'on a le droit d'allumer, 0 → 1 (la pré-intro les libère peu à peu).
    pub allowed: f32,
    /// Les flashs sont permis (`ShowState::flashes`, hors respiration). Faux : allumage en fondu, ni vague, ni
    /// grésillement, ni pouls, comme en mode calme.
    pub flashes: bool,
}

/// La pose d'une lettre pour l'image.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct LetterPose {
    /// Le caractère à dessiner.
    pub ch: char,
    /// Largeur du glyphe, m.
    pub width: f32,
    /// Position du pivot dans le repère de l'enseigne, m.
    pub x: f32,
    /// Altitude du pivot au-dessus du toit, m (la chute la baisse).
    pub y: f32,
    /// Bascule autour de z, rad.
    pub tilt: f32,
    /// Lumière, 0 éteinte, 1 allumée (peut dépasser pour un coup de pouls).
    pub lit: f32,
}

#[derive(Debug, Clone, Copy)]
struct Letter {
    ch: char,
    width: f32,
    base_x: f32,
    base_y: f32,
    ignite_at: Option<f64>,
    /// Dernière lumière montrée, après limitation de la vitesse de variation.
    shown: f32,
    pose: LetterPose,
}

/// L'enseigne : ses lettres et leur animation.
pub struct DjSign {
    calm: bool,
    measure: Measure,
    letters: Vec<Letter>,
    current: SignLayout,
    pending: Option<String>,
    wanted: String,
    /// Le dernier nom reçu, tel quel (avant nettoyage).
    raw: String,
    clock: f64,
    phrase: f32,
    ignited: usize,
    since_ignite: f32,
    fall_start: Option<f64>,
    level: f32,
    layout_seq: u32,
}

impl std::fmt::Debug for DjSign {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("DjSign")
            .field("wanted", &self.wanted)
            .field("letters", &self.letters.len())
            .field("ignited", &self.ignited)
            .finish_non_exhaustive()
    }
}

impl DjSign {
    /// Une enseigne vide. `calm` : allumage sans bégaiement ni vague ni grésillement.
    #[must_use]
    pub fn new(measure: Measure, calm: bool) -> Self {
        Self {
            calm,
            measure,
            letters: Vec::new(),
            current: SignLayout::empty(),
            pending: None,
            wanted: String::new(),
            raw: String::new(),
            clock: 0.0,
            phrase: 0.0,
            ignited: 0,
            since_ignite: 0.0,
            fall_start: None,
            level: 0.0,
            layout_seq: 0,
        }
    }

    /// Les lettres existent (la chute n'a pas forcément commencé).
    #[must_use]
    pub fn present(&self) -> bool {
        !self.letters.is_empty()
    }

    /// Mise en page courante (largeur totale, hauteur de capitale).
    #[must_use]
    pub fn layout(&self) -> &SignLayout {
        &self.current
    }

    /// Monte à chaque reconstruction des lettres : l'ossature et les glyphes sont à refaire.
    #[must_use]
    pub fn layout_seq(&self) -> u32 {
        self.layout_seq
    }

    /// Part des lettres allumées, 0 → 1.
    #[must_use]
    pub fn lit(&self) -> f32 {
        self.level
    }

    /// Le nom voulu (déjà nettoyé).
    #[must_use]
    pub fn name(&self) -> &str {
        &self.wanted
    }

    /// Un nom a été demandé et ses lettres ne sont pas encore posées : la chute des anciennes est en cours.
    #[must_use]
    pub fn falling(&self) -> bool {
        self.fall_start.is_some()
    }

    /// La pose de chaque lettre à l'image courante.
    pub fn poses(&self) -> impl Iterator<Item = LetterPose> + '_ {
        self.letters.iter().map(|l| l.pose)
    }

    /// Écrit un nom. S'il y en a déjà un d'allumé, ses lettres tombent d'abord.
    pub fn set_name(&mut self, raw: &str) {
        // Le chef rappelle `set_name` à chaque image avec le même nom : sans cette sortie, la mise en page (donc des
        // allocations) serait refaite 60 fois par seconde (règle 4).
        if raw == self.raw {
            return;
        }
        self.raw.clear();
        self.raw.push_str(raw);
        let next = layout_sign(raw, &*self.measure, &SIGN_LAYOUT).name;
        if next == self.wanted {
            return;
        }
        self.wanted.clone_from(&next);
        if self.letters.is_empty() || self.ignited == 0 {
            self.rebuild(&next);
        } else {
            self.pending = Some(next);
            self.fall_start.get_or_insert(self.clock);
        }
    }

    /// Éteint et efface tout (l'enseigne est recyclée) ; le nom voulu est conservé.
    pub fn clear(&mut self) {
        self.letters.clear();
        self.pending = None;
        self.fall_start = None;
        self.ignited = 0;
        self.level = 0.0;
        self.current = SignLayout::empty();
        self.layout_seq = self.layout_seq.wrapping_add(1);
    }

    /// Recommence l'allumage depuis la première lettre (nouvelle apparition).
    pub fn rearm(&mut self) {
        let name = self.wanted.clone();
        self.rebuild(&name);
    }

    /// Avance l'enseigne d'une image. `time_s` : l'horloge du spectacle (le grésillement s'y cale).
    pub fn update(&mut self, f: &SignInput, time_s: f64) {
        self.clock += f64::from(f.dt);
        let n = self.letters.len();
        if n == 0 {
            self.level = 0.0;
            return;
        }
        self.phrase = (self.phrase + f.dt * f.bpm.unwrap_or(DEFAULT_BPM) / 60.0 / 16.0).fract();

        // Allumage : une lettre par kick, tant que la pré-intro (ou la chute en cours) le permet.
        // Deux allumages sont espacés d'au moins `MIN_IGNITE_GAP_S` même si les kicks s'enchaînent : un flash par lettre,
        // au plus trois par seconde (règle 7).
        self.since_ignite += f.dt;
        let calm = self.calm || !f.flashes;
        let allowed = if self.fall_start.is_some() {
            0
        } else {
            (f.allowed.clamp(0.0, 1.0) * n as f32 + 1e-6) as usize
        };
        let due = (f.kick && self.since_ignite >= MIN_IGNITE_GAP_S) || self.since_ignite > FALLBACK_IGNITE_S;
        if self.ignited < allowed && due {
            self.letters[self.ignited].ignite_at = Some(self.clock);
            self.ignited += 1;
            self.since_ignite = 0.0;
        }

        let mut total = 0.0;
        for (i, letter) in self.letters.iter_mut().enumerate() {
            let mut value = letter
                .ignite_at
                .map_or(0.0, |at| ignite_level((self.clock - at) as f32, calm));
            if value > 0.0 {
                let wave = if calm { 0.0 } else { wave_gain(i, n, self.phrase) };
                let flicker = if calm { 1.0 } else { sizzle(time_s, i, n, SIZZLE_SEED) };
                let pulse = if calm { 0.0 } else { f.pulse };
                value *= (1.0 + 0.12 * pulse + 0.3 * wave) * flicker;
            }
            // La lumière ne change jamais plus vite que le fondu minimal du limiteur.
            value = slew(letter.shown, value, f.dt);
            letter.shown = value;
            let (mut drop, mut tilt) = (0.0, 0.0);
            if let Some(start) = self.fall_start {
                let fall = fall_state((self.clock - start) as f32 - i as f32 * FALL_STAGGER);
                drop = fall.drop;
                tilt = fall.tilt * if i % 2 == 0 { 1.0 } else { -1.0 };
                value *= fall.light;
            }
            letter.pose = LetterPose {
                ch: letter.ch,
                width: letter.width,
                x: letter.base_x,
                y: letter.base_y - drop,
                tilt,
                lit: value,
            };
            total += value;
        }
        self.level = total / n as f32;
        if let Some(start) = self.fall_start
            && self.clock - start > f64::from(FALL_SECONDS + n as f32 * FALL_STAGGER)
        {
            let next = self.pending.take().unwrap_or_else(|| self.wanted.clone());
            self.rebuild(&next);
        }
    }

    fn rebuild(&mut self, name: &str) {
        self.fall_start = None;
        self.pending = None;
        self.current = layout_sign(name, &*self.measure, &SIGN_LAYOUT);
        let cap = self.current.height;
        // Le centre du plan d'une lettre : le plan déborde plus haut (halo) que bas, la capitale reste sur le pied.
        let base_y = LEG_HEIGHT + cap * ((1.0 + PAD_TOP - PAD_BOTTOM) / 2.0);
        self.letters.clear();
        self.letters.extend(self.current.letters.iter().map(|spec| Letter {
            ch: spec.ch,
            width: spec.width,
            base_x: spec.x,
            base_y,
            ignite_at: None,
            shown: 0.0,
            pose: LetterPose {
                ch: spec.ch,
                width: spec.width,
                x: spec.x,
                y: base_y,
                tilt: 0.0,
                lit: 0.0,
            },
        }));
        self.ignited = 0;
        self.since_ignite = MIN_IGNITE_GAP_S;
        self.level = 0.0;
        self.layout_seq = self.layout_seq.wrapping_add(1);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sign() -> DjSign {
        DjSign::new(Box::new(|_| 0.7), false)
    }

    fn frame(kick: bool) -> SignInput {
        SignInput {
            dt: 1.0 / 60.0,
            kick,
            pulse: 0.0,
            bpm: Some(128.0),
            allowed: 1.0,
            flashes: true,
        }
    }

    fn run(s: &mut DjSign, seconds: f32, kick_every: Option<f32>) {
        let frames = (seconds * 60.0) as usize;
        let every = kick_every.map(|k| (k * 60.0) as usize);
        for i in 0..frames {
            let kick = every.is_some_and(|e| i % e == 0);
            s.update(&frame(kick), f64::from(i as u32) / 60.0);
        }
    }

    #[test]
    fn un_nom_pose_les_lettres_eteintes() {
        let mut s = sign();
        s.set_name("dj neon");
        assert!(s.present());
        assert_eq!(s.name(), "DJ NEON");
        assert_eq!(s.poses().count(), 6);
        assert!(s.poses().all(|p| p.lit.abs() < f32::EPSILON));
        assert!(s.lit().abs() < f32::EPSILON);
    }

    #[test]
    fn les_lettres_s_allument_une_par_kick() {
        let mut s = sign();
        s.set_name("ABCD");
        s.update(&frame(true), 0.0);
        assert_eq!(s.ignited, 1);
        // Des kicks collés ne rallument rien : un flash par lettre, au plus trois par seconde (règle 7).
        for _ in 0..10 {
            s.update(&frame(true), 0.0);
        }
        assert_eq!(s.ignited, 1);
        // Un kick plus tard allume la lettre d'après.
        for _ in 0..20 {
            s.update(&frame(false), 0.0);
        }
        s.update(&frame(true), 0.0);
        assert_eq!(s.ignited, 2);
    }

    #[test]
    fn sans_kick_une_lettre_s_allume_quand_meme() {
        let mut s = sign();
        s.set_name("ABCD");
        run(&mut s, 3.0, None);
        assert_eq!(s.ignited, 4);
    }

    #[test]
    fn la_pre_intro_libere_les_lettres_peu_a_peu() {
        let mut s = sign();
        s.set_name("ABCD");
        let mut f = frame(false);
        f.allowed = 0.5;
        // Un kick toutes les 0,4 s.
        for i in 0..120 {
            f.kick = i % 24 == 0;
            s.update(&f, 0.0);
        }
        assert_eq!(s.ignited, 2);
        f.allowed = 1.0;
        for i in 0..120 {
            f.kick = i % 24 == 0;
            s.update(&f, 0.0);
        }
        assert_eq!(s.ignited, 4);
    }

    #[test]
    fn les_lettres_allumees_finissent_pleinement_allumees() {
        let mut s = DjSign::new(Box::new(|_| 0.7), true);
        s.set_name("AB");
        run(&mut s, 3.0, Some(0.5));
        assert!(s.lit() > 0.9, "{}", s.lit());
    }

    #[test]
    fn un_nouveau_nom_fait_tomber_les_anciennes_lettres_puis_reconstruit() {
        let mut s = sign();
        s.set_name("AB");
        run(&mut s, 2.0, Some(0.25));
        let seq = s.layout_seq();
        let rest_y = s.poses().next().map_or(0.0, |p| p.y);
        s.set_name("CDE");
        assert!(s.falling());
        assert_eq!(
            s.poses().count(),
            2,
            "les anciennes lettres tiennent encore pendant la chute"
        );
        run(&mut s, 0.5, None);
        assert!(s.poses().any(|p| p.y < rest_y - 3.0), "une lettre est descendue");
        run(&mut s, 2.0, None);
        assert!(!s.falling());
        assert_eq!(s.name(), "CDE");
        assert_eq!(s.poses().count(), 3);
        assert_ne!(s.layout_seq(), seq);
    }

    #[test]
    fn changer_de_nom_avant_tout_allumage_reconstruit_tout_de_suite() {
        let mut s = sign();
        s.set_name("AB");
        s.set_name("XYZ");
        assert!(!s.falling());
        assert_eq!(s.poses().count(), 3);
    }

    #[test]
    fn le_meme_nom_ne_fait_rien() {
        let mut s = sign();
        s.set_name("AB");
        let seq = s.layout_seq();
        s.set_name(" ab ");
        assert_eq!(s.layout_seq(), seq);
    }

    #[test]
    fn clear_efface_mais_garde_le_nom_voulu() {
        let mut s = sign();
        s.set_name("AB");
        s.clear();
        assert!(!s.present());
        assert_eq!(s.name(), "AB");
        s.rearm();
        assert!(s.present());
    }
}
