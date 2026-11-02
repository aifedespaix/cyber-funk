//! La boîte de vitesses simulée du conducteur : quel rapport est engagé selon la vitesse, où est le levier dans la grille en H,
//! et la chorégraphie d'un changement de rapport (la main quitte le volant, pousse le levier, y retourne).
//!
//! Aucun effet sur la conduite : la voiture n'a pas de moteur. La boîte ne sert qu'à ce que le geste du conducteur ait un
//! sens (et à ce que le levier de la voiture ne reste pas figé). La grille reprend celle de `CyberCar.ts` (`GEAR_POS`).

/// Un rapport engagé.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Gear {
    /// Marche arrière.
    Reverse,
    /// Point mort.
    Neutral,
    /// Rapport avant, de 1 à 6.
    Forward(u8),
}

/// Vitesse (m/s) au-dessus de laquelle on passe du rapport `n` au rapport `n + 1` (indice `n - 1`). La croisière est à
/// 40 m/s : elle tombe en sixième.
const UPSHIFT_MPS: [f32; 5] = [7.0, 14.0, 22.0, 30.0, 37.0];
/// On rétrograde sous cette part de la vitesse de passage (hystérésis : pas de va-et-vient autour du seuil).
const DOWNSHIFT_RATIO: f32 = 0.8;
/// En dessous (en valeur absolue), la voiture est à l'arrêt : point mort.
const STOPPED_MPS: f32 = 0.5;

impl Gear {
    /// Le rapport qui convient à `speed_mps` (négative en marche arrière), un seul cran à la fois : une montée de
    /// 1ʳᵉ en 4ᵉ passe par la 2ᵉ et la 3ᵉ, chacune avec son geste.
    #[must_use]
    pub fn next(self, speed_mps: f32) -> Self {
        match self {
            Self::Reverse if speed_mps > -STOPPED_MPS => Self::Neutral,
            Self::Reverse => Self::Reverse,
            Self::Neutral if speed_mps >= STOPPED_MPS => Self::Forward(1),
            Self::Neutral if speed_mps <= -STOPPED_MPS => Self::Reverse,
            Self::Neutral => Self::Neutral,
            Self::Forward(1) if speed_mps < STOPPED_MPS * 0.6 => Self::Neutral,
            Self::Forward(n) if n < 6 && speed_mps > UPSHIFT_MPS[usize::from(n) - 1] => Self::Forward(n + 1),
            Self::Forward(n) if n > 1 && speed_mps < UPSHIFT_MPS[usize::from(n) - 2] * DOWNSHIFT_RATIO => {
                Self::Forward(n - 1)
            }
            other @ Self::Forward(_) => other,
        }
    }

    /// Le rapport stable pour `speed_mps`, sans animation : sert à l'initialisation, quand la voiture démarre déjà lancée.
    #[must_use]
    pub fn settled(speed_mps: f32) -> Self {
        let mut gear = Self::Neutral;
        for _ in 0..16 {
            let next = gear.next(speed_mps);
            if next == gear {
                break;
            }
            gear = next;
        }
        gear
    }

    /// Position du pommeau dans la grille : (décalage latéral en m, + = vers la gauche du conducteur ; avant/arrière en m,
    /// + = vers l'avant).
    #[must_use]
    pub fn gate(self) -> (f32, f32) {
        // Les quatre colonnes de la grille : 1-2, 3-4, 5-6, marche arrière.
        const COLUMN_M: [f32; 4] = [0.0525, 0.0175, -0.0175, -0.0525];
        const FORE_M: f32 = 0.05;
        match self {
            Self::Neutral => (0.0, 0.0),
            Self::Reverse => (COLUMN_M[3], FORE_M),
            Self::Forward(n) => {
                let n = n.clamp(1, 6);
                let column = COLUMN_M[usize::from(n - 1) / 2];
                // Les rapports impairs sont vers l'avant, les pairs vers l'arrière.
                (column, if n % 2 == 1 { FORE_M } else { -FORE_M })
            }
        }
    }
}

/// Distance du pivot au centre du pommeau (m), celle de `CyberCar.ts`.
pub const LEVER_LENGTH_M: f32 = 0.2;

/// Rotations du pivot du levier (tangage, roulis en rad) pour un pommeau en (`lateral_m`, `fore_m`) : `CyberCar.ts`,
/// `updateLever`.
#[must_use]
pub fn lever_angles(lateral_m: f32, fore_m: f32) -> (f32, f32) {
    let sin = |m: f32| (m / LEVER_LENGTH_M).clamp(-1.0, 1.0).asin();
    (sin(fore_m), sin(lateral_m))
}

/// Un changement de rapport en cours.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Shift {
    /// Rapport quitté.
    pub from: Gear,
    /// Rapport engagé à la fin du geste.
    pub to: Gear,
    /// Temps écoulé depuis le début du geste, s.
    pub elapsed_s: f32,
}

/// La main rejoint le levier.
pub const REACH_S: f32 = 0.18;
/// Le levier passe d'une case de la grille à l'autre, par le point mort.
pub const MOVE_S: f32 = 0.22;
/// La main revient au volant.
pub const RETURN_S: f32 = 0.2;
/// Durée totale d'un changement de rapport.
pub const SHIFT_S: f32 = REACH_S + MOVE_S + RETURN_S;

fn smooth(x: f32) -> f32 {
    let x = x.clamp(0.0, 1.0);
    x * x * (3.0 - 2.0 * x)
}

impl Shift {
    /// Un geste qui commence.
    #[must_use]
    pub fn new(from: Gear, to: Gear) -> Self {
        Self {
            from,
            to,
            elapsed_s: 0.0,
        }
    }

    /// Le geste est fini.
    #[must_use]
    pub fn is_done(&self) -> bool {
        self.elapsed_s >= SHIFT_S
    }

    /// Part du temps de la main sur le levier plutôt que sur le volant : 0 sur le volant, 1 sur le pommeau.
    #[must_use]
    pub fn hand_on_lever(&self) -> f32 {
        let t = self.elapsed_s;
        if t < REACH_S {
            smooth(t / REACH_S)
        } else if t < REACH_S + MOVE_S {
            1.0
        } else {
            1.0 - smooth((t - REACH_S - MOVE_S) / RETURN_S)
        }
    }

    /// Position du pommeau dans la grille (m) : de la case de départ à celle d'arrivée en passant par la rangée du point mort
    /// quand on change de colonne.
    #[must_use]
    pub fn knob(&self) -> (f32, f32) {
        let progress = smooth((self.elapsed_s - REACH_S) / MOVE_S);
        let (start, end) = (self.from.gate(), self.to.gate());
        // Trajet en trois segments : on ressort sur la rangée du point mort, on glisse, on rentre dans la case.
        let path = [start, (start.0, 0.0), (end.0, 0.0), end];
        let lengths = [
            (path[1].1 - path[0].1).abs(),
            (path[2].0 - path[1].0).abs(),
            (path[3].1 - path[2].1).abs(),
        ];
        let total: f32 = lengths.iter().sum();
        if total < 1e-6 {
            return end;
        }
        let mut left = progress * total;
        for (i, length) in lengths.iter().enumerate() {
            if left <= *length || i == 2 {
                let k = if *length > 1e-6 {
                    (left / length).clamp(0.0, 1.0)
                } else {
                    1.0
                };
                let (from, to) = (path[i], path[i + 1]);
                return (from.0 + (to.0 - from.0) * k, from.1 + (to.1 - from.1) * k);
            }
            left -= length;
        }
        end
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Fait avancer la boîte jusqu'à ce qu'elle se stabilise.
    fn settle(mut gear: Gear, speed: f32) -> Gear {
        for _ in 0..16 {
            gear = gear.next(speed);
        }
        gear
    }

    #[test]
    fn a_l_arret_c_est_le_point_mort_et_en_croisiere_la_sixieme() {
        assert_eq!(Gear::settled(0.0), Gear::Neutral);
        assert_eq!(Gear::settled(40.0), Gear::Forward(6));
        assert_eq!(Gear::settled(-3.0), Gear::Reverse);
    }

    #[test]
    fn la_boite_monte_d_un_seul_cran_a_la_fois() {
        assert_eq!(Gear::Forward(1).next(35.0), Gear::Forward(2));
        assert_eq!(Gear::Neutral.next(35.0), Gear::Forward(1));
        assert_eq!(Gear::Reverse.next(35.0), Gear::Neutral);
    }

    #[test]
    fn l_hysteresis_evite_le_va_et_vient_autour_d_un_seuil() {
        // 2ᵉ : on monte à 14 m/s mais on ne redescend qu'à 0,8 × 7 = 5,6 m/s.
        let gear = settle(Gear::Forward(2), 6.5);
        assert_eq!(gear, Gear::Forward(2));
        let gear = settle(Gear::Forward(3), 14.2);
        assert_eq!(gear, Gear::Forward(3));
        for speed in [13.0, 14.1, 12.0] {
            assert_eq!(settle(Gear::Forward(3), speed), Gear::Forward(3), "{speed}");
        }
    }

    #[test]
    fn la_grille_en_h_a_huit_cases_distinctes() {
        let gears = [
            Gear::Reverse,
            Gear::Neutral,
            Gear::Forward(1),
            Gear::Forward(2),
            Gear::Forward(3),
            Gear::Forward(4),
            Gear::Forward(5),
            Gear::Forward(6),
        ];
        for (i, a) in gears.iter().enumerate() {
            for b in &gears[i + 1..] {
                assert_ne!(a.gate(), b.gate(), "{a:?} et {b:?} au même endroit");
            }
        }
        // Rapports impairs vers l'avant, pairs vers l'arrière.
        assert!(Gear::Forward(1).gate().1 > 0.0 && Gear::Forward(2).gate().1 < 0.0);
    }

    #[test]
    fn les_angles_du_levier_sont_nuls_au_point_mort() {
        let (lateral, fore) = Gear::Neutral.gate();
        assert_eq!(lever_angles(lateral, fore), (0.0, 0.0));
        let (pitch, roll) = lever_angles(0.0525, 0.05);
        assert!(
            pitch > 0.2 && pitch < 0.3 && roll > 0.25 && roll < 0.3,
            "{pitch} {roll}"
        );
    }

    #[test]
    fn le_geste_commence_et_finit_la_main_sur_le_volant() {
        let mut shift = Shift::new(Gear::Forward(2), Gear::Forward(3));
        assert!(shift.hand_on_lever().abs() < 1e-6);
        shift.elapsed_s = REACH_S + MOVE_S / 2.0;
        assert!((shift.hand_on_lever() - 1.0).abs() < 1e-6);
        shift.elapsed_s = SHIFT_S;
        assert!(shift.hand_on_lever().abs() < 1e-6 && shift.is_done());
    }

    #[test]
    fn le_pommeau_va_de_la_case_de_depart_a_celle_d_arrivee_par_le_point_mort() {
        let (from, to) = (Gear::Forward(2), Gear::Forward(5));
        let mut shift = Shift::new(from, to);
        shift.elapsed_s = 0.0;
        assert_eq!(shift.knob(), from.gate());
        shift.elapsed_s = SHIFT_S;
        let end = shift.knob();
        assert!((end.0 - to.gate().0).abs() < 1e-6 && (end.1 - to.gate().1).abs() < 1e-6);
        // Au milieu du trajet, le pommeau est sur la rangée du point mort.
        shift.elapsed_s = REACH_S + MOVE_S / 2.0;
        assert!(shift.knob().1.abs() < 0.01, "{:?}", shift.knob());
    }

    #[test]
    fn le_pommeau_ne_saute_jamais_d_une_image_a_l_autre() {
        let mut shift = Shift::new(Gear::Forward(1), Gear::Forward(6));
        let mut last = shift.knob();
        while !shift.is_done() {
            shift.elapsed_s += 1.0 / 240.0;
            let now = shift.knob();
            assert!(
                (now.0 - last.0).abs() < 0.01 && (now.1 - last.1).abs() < 0.01,
                "{last:?} -> {now:?}"
            );
            last = now;
        }
    }
}
