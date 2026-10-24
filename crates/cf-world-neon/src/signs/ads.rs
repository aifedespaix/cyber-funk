//! Le catalogue des enseignes (`signs/ads.ts`) : publicités d'un futur où la consommation est brutale, coupable et
//! envahissante. Les corporations parlent au joueur avec mépris ou fausse bienveillance, et ne mentent jamais tout à
//! fait : le système se dénonce par ses propres aveux, sur un ton de communiqué. Chaque entrée associe un visuel (cuit
//! par `paint`, animé par le matériau) à ses slogans.
//!
//! Les textes sont en majuscules, surtout en français : le monde est une ville d'affichage, quelques slogans restent en
//! anglais comme sur de vraies enseignes. Les slogans de cinq mots ou moins (voir `show_signs`) passent en priorité sur
//! les panneaux proches de la route en mode spectacle ; les longs restent au fond et dans les bandeaux.
//!
//! Deux visuels lisent la parité du slogan tiré : « estate » affiche le texte aux rangs pairs et une affiche de concert
//! aux rangs impairs ; « brute » affiche « ACHÈTE MOI » aux rangs pairs et l'enseigne aux rangs impairs. Leurs autres
//! slogans ne défilent que dans le bandeau.

use cf_render::SceneColor;

/// Le visuel d'une publicité (`AdKind`). `Watch` : le portique-caméra ; `Neon` : les enseignes en tubes. Ni l'un ni
/// l'autre n'a de visuel de produit.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum AdKind {
    /// Viande : un pavé qui tourne.
    Food,
    /// Mode : un t-shirt.
    Fashion,
    /// Cerveau scanné et électrocardiogramme.
    Mind,
    /// Casque VR et « GAME OVER ».
    Escape,
    /// Propagande : un sourire.
    Politics,
    /// Immobilier et concert.
    Estate,
    /// Matraquage : « ACHÈTE MOI ».
    Brute,
    /// Écran-caméra.
    Watch,
    /// Tubes de néon.
    Neon,
}

impl AdKind {
    /// Nom du visuel dans Sillage (`AdKind` de `types.ts`) et dans les données de référence.
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::Food => "food",
            Self::Fashion => "fashion",
            Self::Mind => "mind",
            Self::Escape => "escape",
            Self::Politics => "politics",
            Self::Estate => "estate",
            Self::Brute => "brute",
            Self::Watch => "watch",
            Self::Neon => "neon",
        }
    }
}

/// Une publicité : son visuel, ses slogans (un par rotation) et sa couleur dominante.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Ad {
    /// Le visuel.
    pub kind: AdKind,
    /// Couleur dominante (un rôle de la palette de la scène).
    pub accent: SceneColor,
    /// Les slogans, un par rotation ; le bandeau les enchaîne.
    pub slogans: &'static [&'static str],
}

/// Rang d'une publicité dans [`CATALOGUE`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct AdId(pub u8);

impl AdId {
    /// La publicité de ce rang.
    ///
    /// Un rang hors du catalogue ne peut pas être construit hors de ce module ; le repli (la première publicité) évite
    /// une panique si une donnée corrompue en fabriquait un.
    #[must_use]
    pub fn ad(self) -> &'static Ad {
        CATALOGUE.get(usize::from(self.0)).unwrap_or(&CATALOGUE[0])
    }
}

/// Nombre de publicités « produit » (`ADS`), en tête du catalogue.
pub const PRODUCT_ADS: usize = 7;
/// Rang du portique-caméra (`WATCH_AD`) : il filme la voiture et lui adresse un procès-verbal.
pub const WATCH_AD: AdId = AdId(7);
/// Plan de face, caméra basse devant le capot : le ton d'une publicité de voiture, la chute en moins.
pub const CAR_FRONT_AD: AdId = AdId(8);
/// Plan de profil, caméra qui suit la voiture à sa hauteur.
pub const CAR_SIDE_AD: AdId = AdId(9);
/// Rang de la première enseigne en tubes (`NEON_ADS`).
pub const NEON_FIRST: usize = 10;
/// Nombre d'enseignes en tubes de néon : peu de mots, grosses lettres.
pub const NEON_COUNT: usize = 22;
/// Taille du catalogue.
pub const CATALOGUE_LEN: usize = NEON_FIRST + NEON_COUNT;

/// Le catalogue : `ADS` (0 à 6), puis le portique, les deux plans de voiture, puis `NEON_ADS`.
pub const CATALOGUE: [Ad; CATALOGUE_LEN] = [
    Ad {
        kind: AdKind::Food,
        accent: SceneColor::Rose,
        slogans: &[
            "LE MOT « STEAK » EST PROTÉGÉ. CE QU'IL Y A DERRIÈRE, MOINS.",
            "ELLE A VÉCU SIX SEMAINES, DANS LE NOIR. C'EST DÉJÀ BEAUCOUP POUR UN POULET. BON APPÉTIT.",
            "NOS ÉLEVAGES SONT OUVERTS AU PUBLIC. SUR PHOTO. DE LOIN. AVEC DES PRAIRIES.",
            "NOUS NE DISONS PAS « ABATTOIR ». NOUS DISONS « CHAÎNE DE VALEUR ».",
            "FAITES CONFIANCE À L'ÉTIQUETTE. ELLE A VU PLUS DE PRAIRIE QUE LA BÊTE.",
            "BIEN-ÊTRE ANIMAL : UNE CASE COCHÉE, UN SOURIRE IMPRIMÉ, UN PRIX BAS.",
            "LA VIANDE NE COÛTE PAS CHER. QUELQU'UN D'AUTRE PAIE LA DIFFÉRENCE.",
            "ON NE MANGE PAS UN ANIMAL. ON CONSOMME UNE PORTION.",
            "LE JAMBON N'A JAMAIS RIEN DEMANDÉ. C'EST BIEN LÀ SON AVANTAGE.",
            "PRODUIT LOCALEMENT. ABATTU ENCORE PLUS LOCALEMENT.",
            "ELLE N'A JAMAIS VU L'EXTÉRIEUR. VOUS NON PLUS, ENTRE DEUX LIVRAISONS.",
            "SATISFAIT OU REFAIT : NOUS REFAISONS TOUJOURS PLUS GRAND.",
            "LE PRIX AU KILO N'INCLUT NI LA FORÊT, NI L'EAU, NI LE RESTE.",
            "MANGEZ À VOTRE FAIM. NOUS PRODUISONS À LA NÔTRE.",
            "UNE VIE. TRÈS COURTE.",
            "CLEAN LABEL. DIRTY HANDS.",
            "FREE RANGE, 2 M² EACH.",
        ],
    },
    Ad {
        kind: AdKind::Fashion,
        accent: SceneColor::Rose,
        slogans: &[
            "TON STYLE EST OBSOLÈTE. ACHÈTE LA NOUVELLE FIBRE BLANCHIE. SOIS QUELQU'UN.",
            "IMPRESSION THERMIQUE CUTANÉE. PORTEZ NOTRE LOGO DIRECTEMENT DANS VOTRE CHAIR.",
            "PAUVRETÉ INTERDITE DANS LE SECTEUR 4. RENOUVELEZ VOTRE GARDE-ROBE OU SOYEZ EFFACÉ.",
            "LE VÊTEMENT EST ÉCOLOGIQUE : IL SERA JETÉ AVANT D'AVOIR POLLUÉ.",
            "COLLECTION PRINTEMPS, ÉTÉ, AUTOMNE, HIVER, PRINTEMPS, ÉTÉ.",
            "FABRIQUÉ LOIN, VENDU PRÈS, PORTÉ DEUX FOIS.",
            "VOUS ÊTES UNIQUE. COMME LES 40 000 AUTRES QUI PORTENT CECI.",
            "NOS ATELIERS SONT OUVERTS 24H/24. LES FENÊTRES, NON.",
            "SOLDES -70% : LA MARGE ÉTAIT À +900%.",
            "ÊTRE SOI, C'EST ÊTRE LA TAILLE M DE LA SAISON.",
            "PORTEZ VOTRE VALEUR. ELLE EST BRODÉE SUR LA POITRINE.",
            "DERNIÈRE CHANCE D'EXISTER SOCIALEMENT : STOCK LIMITÉ.",
            "RECYCLEZ VOS ANCIENS VÊTEMENTS. NOUS LES REVENDRONS À PLUS PAUVRE.",
            "LE LUXE, C'EST NE JAMAIS CROISER CELLES QUI COUDENT.",
            "PORTE TA VALEUR.",
            "SIZE M, SOUL XS.",
            "SOLDES SUR TA PERSONNALITÉ.",
        ],
    },
    Ad {
        kind: AdKind::Mind,
        accent: SceneColor::Mint,
        slogans: &[
            "VOTRE CERVEAU EST TROP LENT. LOUEZ NOTRE MISE À JOUR COGNITIVE. (999 CRÉDITS/MOIS)",
            "SOUTIEN SCOLAIRE PAR INJECTION NEURALE. NE SOYEZ PLUS LE MAILLON FAIBLE DU SYSTÈME.",
            "ABONNEMENT VITAL PREMIUM : SI VOUS ARRÊTEZ DE PAYER, NOUS EFFAÇONS VOS SOUVENIRS.",
            "L'ATTENTION EST UNE RESSOURCE. VOUS ÊTES LE GISEMENT.",
            "VOUS N'AVEZ PAS À PENSER. NOUS AVONS DÉJÀ PENSÉ POUR 4 MILLIONS DE PERSONNES.",
            "FORMATION CONTINUE : APPRENEZ À VOUS ADAPTER À CE QUE VOUS NE CHOISIREZ PAS.",
            "BIEN-ÊTRE AU TRAVAIL : UN PUFF, UNE PLANTE, UNE ÉCHÉANCE.",
            "VOUS AVEZ DU TEMPS LIBRE ? NOUS AVONS UNE APPLICATION POUR ÇA.",
            "MÉDITEZ 10 MINUTES. REPRENEZ LE TRAVAIL 10 MINUTES PLUS EFFICACE.",
            "VOTRE BURN-OUT EST PERSONNEL. NOTRE SOLUTION EST PAR ABONNEMENT.",
            "L'IA PENSE À VOTRE PLACE. VOUS POUVEZ ENFIN VOUS CONCENTRER SUR VOS MENSUALITÉS.",
            "ZÉRO PENSÉE NÉGATIVE GARANTIE JUSQU'AU PROCHAIN PRÉLÈVEMENT.",
            "PRODUCTIVITÉ : L'ART DE FAIRE PLUS POUR GAGNER LA MÊME CHOSE.",
            "RIEN N'A DE SENS. CELA N'EMPÊCHE PAS LA LIVRAISON EXPRESS.",
            "PENSE MOINS. PAIE PLUS.",
            "TON ATTENTION EST VENDUE.",
            "OUT OF MIND, IN CART.",
        ],
    },
    Ad {
        kind: AdKind::Escape,
        accent: SceneColor::Alarm,
        slogans: &[
            "LA RÉALITÉ EST UN ÉCHEC. CONNECTEZ-VOUS. RESTEZ-Y.",
            "BOOSTEZ VOS STATISTIQUES. ACHETEZ VOTRE VICTOIRE AVEC VOTRE DURÉE DE VIE.",
            "FUYEZ VOTRE EXISTENCE MISÉRABLE POUR SEULEMENT 10 CRÉDITS LA MINUTE.",
            "LA VRAIE VIE EST EN MAINTENANCE. REVENEZ DANS UNE ÉTERNITÉ.",
            "NIVEAU 99 ATTEINT. QUELQUE CHOSE D'IMPORTANT VOUS ATTENDAIT, MAIS QUOI ?",
            "VOTRE TEMPS DE JEU EST NOTRE TEMPS DE CERVEAU DISPONIBLE.",
            "UNE VIE PARALLÈLE POUR LE PRIX DE LA PREMIÈRE.",
            "LA FIN DU MONDE EST EN DLC. PRÉCOMMANDEZ.",
            "GAGNEZ ICI CE QUE VOUS NE GAGNEREZ PLUS LÀ-BAS.",
            "OUVREZ UN COMPTE. FERMEZ LES YEUX. NOUS NOUS OCCUPONS DU RESTE.",
            "SE DÉCONNECTER EST POSSIBLE. NOUS ALLONS VOUS EN DISSUADER, DOUCEMENT.",
            "DÉJÀ 40 000 HEURES DE JEU. IL N'Y A PAS DE HONTE. IL Y A DES STATISTIQUES.",
            "LE MONDE N'A PAS DE BOUTON PAUSE. NOUS SI. 4,99/MOIS.",
            "FUIR EST UN MARCHÉ EN CROISSANCE.",
            "LOG IN. LET GO.",
            "RÉALITÉ : EN MAINTENANCE.",
            "TOUCH GRASS? SOLD OUT.",
        ],
    },
    Ad {
        kind: AdKind::Politics,
        accent: SceneColor::Amber,
        slogans: &[
            "VOTEZ MARTINE LAHAINE POUR UNE VILLE SANS ARBRES.",
            "VOTEZ POUR MOI. VOTEZ POUR MOI. VOTEZ POUR MOI. VOTEZ POUR MOI. VOTEZ POUR MOI.",
            "NOS PROMESSES SONT BIODÉGRADABLES. ELLES DISPARAISSENT D'ELLES-MÊMES.",
            "UN CANDIDAT, DEUX SPONSORS, TROIS MILLIARDAIRES. LA DÉMOCRATIE EST UN TRAVAIL D'ÉQUIPE.",
            "VOTEZ UTILE. NOUS AVONS DÉJÀ ÉLIMINÉ L'INUTILE.",
            "CHANGEMENT. CONTINUITÉ. CHANGEMENT. CONTINUITÉ. (SELON LE SONDAGE)",
            "MARTINE LAHAINE : ELLE N'A PAS MENTI. ELLE A ÉVOLUÉ.",
            "L'AUSTÉRITÉ, C'EST COMME LE RÉGIME : SEULS LES AUTRES MAIGRISSENT.",
            "LES FRONTIÈRES DE LA VILLE SONT FERMÉES. LES CAPITAUX CIRCULENT LIBREMENT.",
            "TOUS ÉGAUX DEVANT LA LOI. SURTOUT CEUX QUI PEUVENT PAYER L'AVOCAT.",
            "NOUS ÉCOUTONS LE PEUPLE. PUIS NOUS RÉPONDONS À SON BANQUIER.",
            "LA CROISSANCE EST INFINIE. LA PLANÈTE, EN OPTION.",
            "VOTRE SALAIRE EST UNE VARIABLE D'AJUSTEMENT. SOYEZ FIER DE VOTRE RÔLE.",
            "VOTEZ DANS LE CALME. LE MARCHÉ A DÉJÀ VOTÉ.",
            "PROMISES EXPIRE SUNDAY.",
            "VOTE EARLY. REGRET LATER.",
            "ENCORE UN MANDAT. ENCORE.",
        ],
    },
    Ad {
        kind: AdKind::Estate,
        accent: SceneColor::Violet,
        slogans: &[
            "VOUS ÊTES SANS DOMICILE FIXE ? ACHETEZ UN IMMEUBLE À -20% !",
            "ÉVÉNEMENT : NOUVEAU CONCERT DES DAFT PURGES AU STADE BARDELA.",
            "STUDIO DE 9 M² AVEC VUE SUR UN AUTRE STUDIO. 4 200 CRÉDITS/MOIS.",
            "DAFT PURGES : TOURNÉE D'ADIEU, LA 12E. ILS ADORENT NOUS DIRE ADIEU.",
            "INVESTISSEZ DANS LA PIERRE : ELLE NE DEMANDE JAMAIS D'AUGMENTATION.",
            "STADE BARDELA : 80 000 PLACES. UNE SEULE ÉMOTION, PARTAGÉE EN SÉRIE.",
            "LOGEMENT SOCIAL : 14 ANS D'ATTENTE. VOS PETITS-ENFANTS SERONT RAVIS.",
            "DAFT PURGES EN DIRECT. LE PUBLIC EST COMPRIS DANS LA RETRANSMISSION.",
            "LE LOYER SUIT VOTRE SALAIRE. PAR COÏNCIDENCE, TOUJOURS PLUS VITE.",
            "FESTIVAL ÉLECTRIQUE : LE CAMPEMENT EST EN OPTION. LA BOUE EST OFFERTE.",
            "PROPRIÉTAIRE À 80 ANS, C'EST POSSIBLE. IL SUFFIT DE NE RIEN VOULOIR D'AUTRE.",
            "CONCERT GRATUIT. VOS DONNÉES SONT LE BILLET.",
            "VUE IMPRENABLE JUSQU'À LA PROCHAINE CONSTRUCTION.",
            "AFTER-PARTY SUR L'EMPLACEMENT DE L'ANCIEN PARC. LE PARC EST SUR L'AFFICHE.",
            "UN CHEZ-SOI N'A PAS DE PRIX. NOUS EN AVONS TROUVÉ UN QUAND MÊME.",
            "DAFT PURGES : COMPLET. LES REVENDEURS ONT DÉJÀ ACHETÉ VOS PLACES.",
        ],
    },
    Ad {
        kind: AdKind::Brute,
        accent: SceneColor::Alarm,
        slogans: &[
            "ACHÈTE MOI.",
            "TU VEUX DU SEXXX ? ACHÈTE-MOI !",
            "ACHÈTE. REGRETTE. RACHÈTE.",
            "TU NE TE SOUVIENS PLUS POURQUOI TU VOULAIS ÇA. C'EST NORMAL. ACHÈTE.",
            "-50% SUR LE SUPERFLU. -0% SUR TON LOYER.",
            "OFFRE LIMITÉE À L'ÉTERNITÉ.",
            "TOUT EST EN SOLDE SAUF TON TEMPS.",
            "LIVRAISON GRATUITE. LE LIVREUR, NON.",
            "AJOUTE AU PANIER. AJOUTE AU PANIER. AJOUTE AU PANIER. (PAS BESOIN DE LIRE)",
            "LE VIDE N'EST PAS UN PROBLÈME. C'EST UN MARCHÉ.",
            "TU AS UNE ÂME ? NOUS AVONS UN FORFAIT POUR ÇA.",
            "TON DÉSIR EST PRÉVISIBLE. NOUS L'AVONS DÉJÀ VENDU.",
            "REGARDE-MOI. AIME-MOI. ACHÈTE-MOI. DANS CET ORDRE.",
            "CE PANNEAU A COÛTÉ PLUS CHER QUE LE SALAIRE DE CELUI QUI L'A ACCROCHÉ.",
        ],
    },
    Ad {
        kind: AdKind::Watch,
        accent: SceneColor::Alarm,
        slogans: &[
            "INFRACTION : EXCÈS DE STYLE. AMENDE : 12 CRÉDITS.",
            "SOURIEZ. CE PORTIQUE REVEND VOTRE IMAGE.",
            "VITESSE ENREGISTRÉE. VOTRE ASSUREUR VOUS REMERCIE.",
            "SUJET IDENTIFIÉ : VOUS. PROFIL MIS À JOUR.",
            "CE PORTIQUE EST VOTRE AMI. IL NE LE DIT PAS, MAIS IL SAIT.",
            "TRAJET OPTIMISÉ POUR LA CONSOMMATION. VOTRE DESTINATION A ÉTÉ SPONSORISÉE.",
            "AMENDE PRÉLEVÉE AUTOMATIQUEMENT. MERCI DE VOTRE CONFIANCE.",
            "LE PÉAGE EST GRATUIT. VOTRE ATTENTION, NON.",
            "RETARD DÉTECTÉ SUR VOTRE CONSOMMATION MENSUELLE.",
            "SUSPECT : SOURIRE INSUFFISANT.",
        ],
    },
    Ad {
        kind: AdKind::Watch,
        accent: SceneColor::Rose,
        slogans: &[
            "LA NOUVELLE SILLAGE. ELLE AVANCE. VOUS, PAR PROCURATION.",
            "ZÉRO ÉMISSION. LES AUTRES, ON LES VEND À PART.",
            "LA LIBERTÉ A QUATRE ROUES. LE CRÉDIT EN A QUATRE-VINGT-SEIZE.",
            "UNE VOITURE QUI VOUS RESSEMBLE. SURTOUT SUR LA PHOTO.",
            "ESSAI GRATUIT. REPRISE EN CRÉDITS. SORTIE PAR LA PORTE DE DERRIÈRE.",
            "SON REGARD SAIT OÙ VOUS ALLEZ. ET QUI VOUS Y ATTEND.",
            "CONDUIRE, C'EST CHOISIR. LÀ, C'EST DÉJÀ CHOISI.",
            "ÉLECTRIQUE, SILENCIEUSE, FINANCÉE SUR 96 MOIS.",
        ],
    },
    Ad {
        kind: AdKind::Watch,
        accent: SceneColor::Azure,
        slogans: &[
            "DE PROFIL, ELLE FAIT MOINS PEUR À L'ASSUREUR.",
            "LA LIGNE QUI A COÛTÉ TROIS DESIGNERS ET UN STAGIAIRE.",
            "AÉRODYNAMIQUE : 0,19. PRIX : NOUS Y VENONS.",
            "ELLE GLISSE. VOTRE BUDGET AUSSI.",
            "TRACTION INTÉGRALE. DETTE INTÉGRALE.",
            "UN PROFIL D'EXCEPTION. LE VÔTRE EST DÉJÀ EN BASE.",
            "0 À 100 EN 3 SECONDES. 0 À LA FAILLITE EN 4.",
        ],
    },
    Ad {
        kind: AdKind::Neon,
        accent: SceneColor::Rose,
        slogans: &[
            "OUVERT 24H/24",
            "DORMIR COÛTE CHER",
        ],
    },
    Ad {
        kind: AdKind::Neon,
        accent: SceneColor::Mint,
        slogans: &[
            "NOODLES",
            "SOUPE SYNTHÉTIQUE",
        ],
    },
    Ad {
        kind: AdKind::Neon,
        accent: SceneColor::Amber,
        slogans: &[
            "MOTEL ∞",
            "CHAMBRE 404",
        ],
    },
    Ad {
        kind: AdKind::Neon,
        accent: SceneColor::Azure,
        slogans: &[
            "CRÉDIT FACILE",
            "DETTE +",
        ],
    },
    Ad {
        kind: AdKind::Neon,
        accent: SceneColor::Violet,
        slogans: &[
            "RÊVES -70%",
            "SOLDES D'ÂMES",
        ],
    },
    Ad {
        kind: AdKind::Neon,
        accent: SceneColor::Lime,
        slogans: &[
            "OBÉIS",
            "SOURIEZ",
        ],
    },
    Ad {
        kind: AdKind::Neon,
        accent: SceneColor::Alarm,
        slogans: &[
            "PROMO",
            "PROMO ÉTERNELLE",
        ],
    },
    Ad {
        kind: AdKind::Neon,
        accent: SceneColor::Rose,
        slogans: &[
            "TRAVAILLE",
            "CONSOMME",
        ],
    },
    Ad {
        kind: AdKind::Neon,
        accent: SceneColor::Mint,
        slogans: &[
            "100% BIO*",
            "*PAS DU TOUT",
        ],
    },
    Ad {
        kind: AdKind::Neon,
        accent: SceneColor::Amber,
        slogans: &[
            "PRIX CASSÉS",
            "EMPLOYÉS AUSSI",
        ],
    },
    Ad {
        kind: AdKind::Neon,
        accent: SceneColor::Azure,
        slogans: &[
            "À QUOI BON",
            "ACHETEZ",
        ],
    },
    Ad {
        kind: AdKind::Neon,
        accent: SceneColor::Violet,
        slogans: &[
            "TOUT SE VEND",
            "VOUS AUSSI",
        ],
    },
    Ad {
        kind: AdKind::Neon,
        accent: SceneColor::Lime,
        slogans: &[
            "LIVRAISON 5 MIN",
            "LIVREUR 5 MIN",
        ],
    },
    Ad {
        kind: AdKind::Neon,
        accent: SceneColor::Alarm,
        slogans: &[
            "MERCI",
            "REVENEZ",
        ],
    },
    Ad {
        kind: AdKind::Neon,
        accent: SceneColor::Rose,
        slogans: &[
            "LIKE",
            "TOI AUSSI",
        ],
    },
    Ad {
        kind: AdKind::Neon,
        accent: SceneColor::Amber,
        slogans: &[
            "SMILE OR ELSE",
            "SOURIRE OBLIGATOIRE",
        ],
    },
    Ad {
        kind: AdKind::Neon,
        accent: SceneColor::Mint,
        slogans: &[
            "ZÉRO DÉCHET*",
            "*SAUF VOUS",
        ],
    },
    Ad {
        kind: AdKind::Neon,
        accent: SceneColor::Azure,
        slogans: &[
            "BUY NOW",
            "CRY LATER",
        ],
    },
    Ad {
        kind: AdKind::Neon,
        accent: SceneColor::Violet,
        slogans: &[
            "REVENDEZ-VOUS",
            "PRIX DE GROS",
        ],
    },
    Ad {
        kind: AdKind::Neon,
        accent: SceneColor::Lime,
        slogans: &[
            "WIFI GRATUIT",
            "CERVEAU EN OPTION",
        ],
    },
    Ad {
        kind: AdKind::Neon,
        accent: SceneColor::Rose,
        slogans: &[
            "LAST CALL",
            "FOREVER",
        ],
    },
    Ad {
        kind: AdKind::Neon,
        accent: SceneColor::Alarm,
        slogans: &[
            "AUJOURD'HUI SEULEMENT",
            "DEMAIN AUSSI",
        ],
    },
];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn le_catalogue_a_la_forme_attendue() {
        assert_eq!(CATALOGUE.len(), CATALOGUE_LEN);
        assert!(CATALOGUE[..PRODUCT_ADS].iter().all(|a| !matches!(a.kind, AdKind::Watch | AdKind::Neon)));
        assert!(CATALOGUE[NEON_FIRST..].iter().all(|a| a.kind == AdKind::Neon));
        for id in [WATCH_AD, CAR_FRONT_AD, CAR_SIDE_AD] {
            assert_eq!(id.ad().kind, AdKind::Watch);
        }
        assert!(CATALOGUE.iter().all(|a| !a.slogans.is_empty()));
    }
}
