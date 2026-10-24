//! Les garde-fous de la pose finale : la caméra reste là où la ville est complète, hors des murs, et jamais en NaN.

use crate::pose::is_finite;
use crate::world::CinemaFrame;

use super::CinemaDirector;

/// La caméra ne recule jamais au-delà de cette distance derrière la voiture (m) : la ville y est bien fournie et noyée
/// de brume.
pub const MAX_BEHIND: f64 = 90.0;
/// Et devant elle (m) : la ville s'étend sur ~490 m devant la voiture, au-delà de 190 m on verrait son bord avant la
/// brume.
pub const MAX_AHEAD: f64 = 190.0;
/// Altitude maximale de la caméra au-dessus du sol sous la voiture (m).
pub const MAX_HEIGHT: f64 = 170.0;
/// Rayon (m) de la sphère qui doit rester libre autour de l'objectif : le plan proche de la caméra (≈ 0,1 m) et un peu
/// de marge, pour qu'aucune façade ne remplisse l'image.
pub const CAMERA_RADIUS: f64 = 0.6;
/// L'Ascension monte au-delà du plafond ordinaire : la caméra la suit jusqu'à cette hauteur au-dessus de la voiture.
const ASCENSION_HEADROOM: f64 = 60.0;
/// Dégagement d'un mur : nombre de pas, rapprochement de l'axe de la route et montée par pas (`keepClear` de Sillage).
const WALL_STEPS: usize = 6;
const WALL_PULL: f64 = 0.72;
const WALL_LIFT: f64 = 2.5;

impl CinemaDirector {
    /// La ville n'existe qu'autour de la voiture : la caméra reste là où elle est complète, jamais devant son bord
    /// effacé.
    pub(super) fn confine(&mut self, frame: &CinemaFrame<'_>) {
        let p = &mut self.out.position;
        p.z = p.z.clamp(frame.car.z - MAX_AHEAD, frame.car.z + MAX_BEHIND);
        let ground = frame.ground();
        let ceiling = match frame.stunt {
            Some(stunt) if stunt.kind == "ascension" => MAX_HEIGHT.max(frame.car.y - ground + ASCENSION_HEADROOM),
            _ => MAX_HEIGHT,
        };
        p.y = p.y.min(ground + ceiling);
    }

    /// Une pose dans un mur (une transition qui coupe un coin, un plan mal dégagé) : on la rapproche de la chaussée et
    /// on la remonte, comme les plans le font de leur consigne. Sillage s'en remettait aux plans seuls ; le réalisateur
    /// vérifie ici la pose finale, transitions comprises.
    pub(super) fn avoid_walls(&mut self, frame: &CinemaFrame<'_>) {
        if !self.out.position.is_finite() || !frame.collides(self.out.position, CAMERA_RADIUS) {
            return;
        }
        self.stats.wall_saves += 1;
        let p = &mut self.out.position;
        for _ in 0..WALL_STEPS {
            p.x *= WALL_PULL;
            p.y += WALL_LIFT;
            if !frame.collides(*p, CAMERA_RADIUS) {
                break;
            }
        }
    }

    /// Une pose non finie (NaN) ferait disparaître l'image, une pose encore dans un mur le remplirait : on garde le
    /// dernier bon cadrage et on nomme le coupable.
    pub(super) fn guard(&mut self, frame: &CinemaFrame<'_>) {
        let finite = is_finite(&self.out);
        if finite && !frame.collides(self.out.position, CAMERA_RADIUS) {
            self.last_good = self.out;
            return;
        }
        self.stats.fallbacks += 1;
        let shot = self.current.spec.and_then(|i| self.catalog.get(i)).map(|s| s.id());
        let incoming = self.incoming.spec.and_then(|i| self.catalog.get(i)).map(|s| s.id());
        tracing::warn!(
            target: "cinema",
            shot,
            incoming,
            transition = ?self.transition(),
            finite,
            car = ?frame.car,
            "pose invalide : dernier bon cadrage"
        );
        self.out = self.last_good;
        // Un plan qui produit des NaN ne s'améliorera pas : on le remplace dès la prochaine image.
        if !finite && !self.transition.active {
            self.poisoned = true;
        }
    }
}
