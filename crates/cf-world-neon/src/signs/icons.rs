//! Les visuels de remplacement (`art-icons.ts`) : chacun illustre le slogan qui s'affiche, à la place de l'icône par
//! défaut du produit. Ce module ne choisit que l'icône ; le dessin est dans `paint::icons`.

use super::ads::AdKind;

/// Les icônes de remplacement (`IconId`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum IconId {
    /// Un œuf derrière des barreaux.
    Cage,
    /// Étiquette de prix.
    Tag,
    /// Bobine de fil et aiguille.
    Needle,
    /// Cadenas.
    Lock,
    /// Puce électronique.
    Chip,
    /// Sablier.
    Hourglass,
    /// Manette de jeu.
    Pad,
    /// Bouton pause.
    Pause,
    /// Bouton d'alimentation.
    Power,
    /// Urne et bulletin.
    Ballot,
    /// Histogramme.
    Chart,
    /// Le sourire du candidat et son nez.
    Liar,
    /// Maison.
    House,
    /// Scène et projecteurs.
    Spot,
}

impl IconId {
    /// Nom de l'icône dans les données de référence de Sillage.
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::Cage => "cage",
            Self::Tag => "tag",
            Self::Needle => "needle",
            Self::Lock => "lock",
            Self::Chip => "chip",
            Self::Hourglass => "hourglass",
            Self::Pad => "pad",
            Self::Pause => "pause",
            Self::Power => "power",
            Self::Ballot => "ballot",
            Self::Chart => "chart",
            Self::Liar => "liar",
            Self::House => "house",
            Self::Spot => "spot",
        }
    }
}

type Rule = (&'static [&'static str], IconId);

/// Pour chaque produit, les mots du slogan qui appellent une autre image que celle par défaut (cerveau, t-shirt,
/// sourire...). Les règles de Sillage sont des alternances de mots : une règle est vraie si le slogan en contient un.
const fn rules(kind: AdKind) -> &'static [Rule] {
    match kind {
        AdKind::Food => &[
            (
                &["POULET", "ÉLEV", "ANIMAL", "BÊTE", "PRAIRIE", "VÉCU", "VU L'EXT", "ÉTIQUETTE", "BIEN-ÊTRE"],
                IconId::Cage,
            ),
            (&["PRIX", "KILO", "COÛTE", "PORTION", "REFAIT"], IconId::Tag),
        ],
        AdKind::Fashion => &[
            (&["SOLDE", "STOCK", "MARGE", "PRIX", "%", "RECYCLEZ", "VALEUR"], IconId::Tag),
            (
                &["ATELIER", "FABRIQU", "FENÊTRE", "COLLECTION", "BRODÉ", "TAILLE", "FIBRE"],
                IconId::Needle,
            ),
        ],
        AdKind::Mind => &[
            (
                &["ABONNEMENT", "PRÉLÈVEMENT", "CRÉDIT", "MENSUALIT", "LOUEZ", "999"],
                IconId::Lock,
            ),
            (
                &["IA PENSE", "NEURAL", "APPLICATION", "ATTENTION", "GISEMENT", "ZÉRO PENSÉE"],
                IconId::Chip,
            ),
            (
                &["TEMPS", "MINUTES", "BURN-OUT", "ÉCHÉANCE", "BIEN-ÊTRE", "PRODUCTIVITÉ"],
                IconId::Hourglass,
            ),
        ],
        AdKind::Escape => &[
            (
                &[
                    "NIVEAU",
                    "STATISTIQUES",
                    "GAGNEZ",
                    "HEURES DE JEU",
                    "TEMPS DE JEU",
                    "DLC",
                    "VICTOIRE",
                    "VIE PARALLÈLE",
                ],
                IconId::Pad,
            ),
            (&["PAUSE", "MAINTENANCE", "FIN DU MONDE"], IconId::Pause),
            (
                &["DÉCONNECT", "CONNECTEZ", "OUVREZ UN COMPTE", "FUIR", "FUYEZ"],
                IconId::Power,
            ),
        ],
        AdKind::Politics => &[
            (&["POUR MOI", "MARTINE"], IconId::Liar),
            (
                &["VOTEZ DANS", "VOTEZ UTILE", "PEUPLE", "DÉMOCRATIE", "SONDAGE", "ÉGAUX"],
                IconId::Ballot,
            ),
            (
                &["CROISSANCE", "CAPITAUX", "SALAIRE", "BANQUIER", "AUSTÉRITÉ", "MILLIARDAIRE", "SPONSOR"],
                IconId::Chart,
            ),
            (&["PROMESSE", "MENTI", "ÉVOLUÉ", "CONTINUITÉ"], IconId::Liar),
        ],
        AdKind::Estate => &[
            (&["CONCERT", "DAFT", "STADE", "FESTIVAL", "AFTER", "BILLET"], IconId::Spot),
            (
                &["LOYER", "STUDIO", "LOGEMENT", "PROPRI", "PIERRE", "DOMICILE", "CHEZ-SOI", "VUE", "IMMEUBLE"],
                IconId::House,
            ),
        ],
        AdKind::Brute | AdKind::Watch | AdKind::Neon => &[],
    }
}

/// L'icône que le slogan appelle, ou `None` pour garder celle du produit.
///
/// Sillage testait `CRÉDITS?` (« CRÉDIT » ou « CRÉDITS ») : la forme sans `S` couvre les deux.
#[must_use]
pub fn icon_for(kind: AdKind, text: &str) -> Option<IconId> {
    rules(kind)
        .iter()
        .find(|(words, _)| words.iter().any(|w| text.contains(w)))
        .map(|(_, icon)| *icon)
}
