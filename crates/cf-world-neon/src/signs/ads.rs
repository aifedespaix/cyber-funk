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
            // Agroalimentaire & Vivant
            "L'HERBE N'ÉTAIT QU'UN INTERMÉDIAIRE.",
            "UNE CHAIR SANS PASSÉ. UN REPAS SANS QUESTIONS.",
            "LA TERRE EXIGEAIT UNE PATIENCE QUE NOUS N'AVONS PLUS.",
            "L'ORIGINE S'EFFACE. LA VALEUR RESTE.",
            "L'INSTINCT ÉTAIT UNE DÉPENSE ÉNERGÉTIQUE INUTILE.",
            "NOUS NE DISONS PAS « ABATTOIR ». NOUS DISONS « CHAÎNE DE VALEUR ».",
            "FAITES CONFIANCE À L'ÉTIQUETTE. ELLE A VU PLUS DE PRAIRIE QUE LA BÊTE.",
            "LE BIEN-ÊTRE ANIMAL™ : LA GARANTIE D'UN PRIX BAS, SANS LES QUESTIONS.",
            "MATURATION ACCÉLÉRÉE EN ENVIRONNEMENT CONTRÔLÉ. LE SUMMUM DE L'EFFICACITÉ PROTEINÉE.",
            "LE SOUVENIR DE LA VIANDE, SANS LE PRIX DE L'EAU.",
            "L'HORIZON N'ÉTAIT QU'UNE PERTE DE MÈTRES CARRÉS.",
            "LE BLEU DU CIEL ÉTAIT INSTABLE. NOUS L'AVONS FIXÉ.",
            "NOUS AVONS CONSERVÉ L'IDÉE DE LA FORÊT.",
            "LE DEHORS EST UNE HÉSITATION. LE DEDANS EST UNE CERTITUDE.",
            "L'EAU CLAIRE A DÉSORMAIS UNE FORMULE, PLUS UN COURS D'EAU.",
            "AQUACULTURE ZÉRO RAYON. POISSONS NÉS ET CONDITIONNÉS AU MÊME ÉTAGE.",
            "L'AIR PUR N'EST PAS UN DROIT. C'EST UNE SÉLECTION.",
            "LE DOUTE DÉPENSE PLUS DE CALORIES QU'IL N'EN PRODUIT.",
            "LE SILENCE S'APPRÉCIE D'AUTANT MIEUX QU'IL EST MENSUALISÉ.",
            "NOUS AVONS BUDGÉTISÉ LA SUITE. IL NE VOUS RESTE QU'À LA VIVRE.",
            "RIEN NE S'EST PERDU. TOUT S'EST RÉSUMÉ.",
            "LA FATIGUE EST UNE ERREUR DE GESTION PERSONNELLE.",
            "VOS RÊVES SONT TROP LONGS. SOYEZ CONCIS.",
            "POURQUOI SE REBELLER QUAND ON PEUT S'ABONNER ?",
            "LA FIN DU MONDE ÉTAIT HIER. LES SOLDES SONT AUJOURD'HUI.",
        ],
    },
    Ad {
        kind: AdKind::Fashion,
        accent: SceneColor::Rose,
        slogans: &[
            "VOTRE IDENTITÉ DE LA SEMAINE DERNIÈRE A EXPIRÉ.",
            "L'ÉLÉGANCE EST UNE CONDITION D'ACCÈS AU SECTEUR 4.",
            "MARQUAGE THERMIQUE CUTANÉ : L'APPARTENANCE DÉFINITIVE.",
            "UN TEXTILE PENSÉ POUR S'EFFACER AVANT D'ENCOMBRER.",
            "LE TEMPS S'ACCÉLÈRE. ADOPTEZ LA COLLECTION SEMAINE 42.",
            "TROIS MILLE KILOMÈTRES DE LOGISTIQUE. DEUX USAGES SEULEMENT.",
            "L'EXCLUSIVITÉ À ÉCHELLE INDUSTRIELLE : PARTAGÉE PAR 40 000 CITOYENS.",
            "PRODUCTION CONTINUELLE EN MILIEU HERMÉTIQUE. SANS AUCUNE PERTE DE LUMIÈRE.",
            "L'EFFORT DE PRIX EST UN CALCUL DÉJÀ AMORTI.",
            "TAILLE M : LA CONFORMITÉ ÉLÉGANTE DE LA SAISON.",
            "VOTRE VALEUR SOCIALE NE SE DEVINE PAS. ELLE S'AFFICHE AU COLLIER.",
            "LE STOCK EST LIMITÉ. VOTRE EXISTENCE SOCIALE AUSSI.",
            "CIRCULARITÉ RATIONNELLE : VOS REBUTS DEVIENNENT LEUR LUXE.",
            "L'ÉLÉGANCE COMMENCE LÀ OÙ LA MAIN-D'ŒUVRE S'EFFACE.",
            "SILHOUETTE M. CONSCIENCE XS.",
            "REMISE IMMÉDIATE SUR VOTRE INDIVIDUALITÉ.",
            "LE LIEN SOCIAL, SANS L'INCONVÉNIENT DE L'AUTRE.",
            "LA MORT N'EST QU'UNE FIN DE CONTRAT NON RENOUVELÉE.",
            "LE PASSÉ EST UN ESPACE SANS RENDEMENT. REGARDEZ DEVANT.",
            "LE REPOS N'EST QU'UNE MAINTENANCE OBLIGATOIRE DE L'ACTIF.",
            "L'ILLUSION DU CHOIX EST INCLUSE DANS LE FORFAIT DE BASE.",
        ],
    },
    Ad {
        kind: AdKind::Mind,
        accent: SceneColor::Mint,
        slogans: &[
            "LA LENTEUR ÉTAIT UNE FATALITÉ BIOLOGIQUE. C'EST DÉSORMAIS UN CHOIX DE FORFAIT.",
            "PÉDAGOGIE NEURALE ACCÉLÉRÉE : L'APPRENTISSAGE N'EST PLUS UNE HÉSITATION.",
            "VOS SOUVENIRS RESTENT ACCESSIBLES TANT QUE VOTRE SOUSCRIPTION EST ACTIVE.",
            "L'ATTENTION EST UNE RESSOURCE. VOUS ÊTES LE GISEMENT.",
            "LA PENSÉE INDIVIDUELLE EST UN DOUBLON. NOUS L'AVONS DÉJÀ OPTIMISÉE.",
            "FORMATION CONTINUÉE : L'ART DE S'ADAPTER À CE QUE VOUS N'AVEZ PAS CHOISI.",
            "ÉQUILIBRE AU TRAVAIL : UNE PLANTE SYNTHÉTIQUE, UN JET D'OXYGENE, UNE ÉCHÉANCE.",
            "LE TEMPS VACANT EST UNE INEFFICACITÉ. NOUS AVONS L'APPLICATION POUR LE REMPLIR.",
            "MÉDITATION EXPRESS : RESTAUREZ L'OUTIL POUR LA PROCHAINE ÉCHÉANCE.",
            "LA FATIGUE EST UNE ERREUR INDIVIDUELLE. NOTRE PROTOCOLE EST SUR ABONNEMENT.",
            "DÉLÉGUEZ LA PENSÉE. GARDEZ LES MENSUALITÉS.",
            "SÉRÉNITÉ SOUS LICENCE : ZÉRO ANXIÉTÉ JUSQU'AU PROCHAIN PRÉLÈVEMENT.",
            "RENDEMENT CONSTANT : FAIRE HIER CE QUE DEMAIN EXIGERA.",
            "LE SENS DE L'EXISTENCE N'EST PAS GUARANTI. LA LIVRAISON SOUS 12 MINUTES, SI.",
            "L'ABSENCE DE PENSÉE EST LE CONFORT SUPRÊME.",
            "VOTRE REGARD A DÉJÀ UNE VALEUR MARCHANDE.",
            "PANIER VALIDÉ AVANT MÊME L'ÉMERGENCE DU DÉSIR.",
            "L'ANXIÉTÉ EST UNE MAUVAISE GESTION DE VOS RESSOURCES ATTENTIONNELLES.",
            "VOTRE NEURO-STABILITÉ EST À PORTÉE DE MISE À JOUR.",
            "POURQUOI DOUTER QUAND L'ALGORITHME A DÉJÀ CALCULÉ VOTRE SATISFACTION ?",
            "LE BURN-OUT EST LE SIGNE D'UN PROCESSEUR MAL VENTILÉ.",
            "NE LAISSEZ PAS VOS PENSÉES DÉPASSER LE CADRE DE VOTRE FORFAIT.",
        ],
    },
    Ad {
        kind: AdKind::Escape,
        accent: SceneColor::Alarm,
        slogans: &[
            "LA RÉALITÉ ÉTAIT UN DÉFAUT DE CONCEPTION.",
            "ACCÉLÉRATION COGNITIVE : LA VICTOIRE VAUT BIEN QUELQUES ANNÉES RESTANTES.",
            "L'ÉVASION SANS L'ENCOMBREMENT DU RÉEL.",
            "LE MONDE PHYSIQUE EST EN MAINTENANCE PROLONGÉE.",
            "ACCOMPLISSEMENT MAXIMAL ATTEINT. QU'IMPORTE CE QUE VOUS AVEZ LAISSÉ DEHORS.",
            "VOTRE IMMERSION EST NOTRE ACTIF LE PLUS PRÉCIEUX.",
            "UNE EXISTENCE PARALLÈLE. POUR LE PRIX DE LA PREMIÈRE.",
            "L'EXTINCTION DU MONDE PHYSIQUE : DISPONIBLE DANS LA PROCHAINE EXTENSION.",
            "GAGNEZ ICI CE QUE LE RÉEL NE PEUT PLUS VOUS OFFRIR.",
            "OUVREZ UN COMPTE. FERMEZ LES YEUX. LE MONDE SE CHARGERA DE DISPARAÎTRE.",
            "LA DÉCONNEXION EST UNE OPTION. NOUS SAURONS VOUS EN DISSUADER.",
            "QUARANTE MILLE HEURES D'IMMERSION. AUCUNE HONTE : QUE DES STATISTIQUES.",
            "LE RÉEL N'A PAS DE BOUTON PAUSE. NOTRE SIMULATION SI.",
            "L'ÉVASION EST LE SEUL MARCHÉ À CROISSANCE INFINIE.",
            "S'ABANDONNER EST LA SEULE DÉCISION RATIONNELLE.",
            "LA NATURE N'EST PLUS DISPONIBLE DANS LE FORFAIT DE BASE.",
            "LA VRAIE VIE ÉTAIT TROP BRUYANTE. BIENVENUE DANS VOTRE FRÉQUENCE.",
            "POURQUOI HABITER LE MONDE QUAND ON PEUT LE CONCEVOIR ?",
            "VOTRE CORPS EST EN PAUSE. VOTRE ESPRIT PRODUIT ENFIN.",
            "LE SOUVENIR DU MONDE EXTÉRIEUR S'ESTOMPE SANS DOULEUR.",
            "RESTEZ IMMERGÉ. LE DEHORS N'A PLUS RIEN À VOUS OFFRIR.",
        ],
    },
    Ad {
        kind: AdKind::Politics,
        accent: SceneColor::Amber,
        slogans: &[
            "MARINE LEPEN EST UNE SOMBRE CONNASSE.",
            "SÉCURITÉ URBAINE : LE VÉGÉTAL N'EST PLUS UN OBSTACLE LOGISTIQUE.",
            "SÉLECTION CIVIQUE OPTIMISÉE : VOTRE BULLETIN A ÉTÉ PRÉ-COCHÉ.",
            "ENGAGEMENT RECYCLABLE : NOS ORIENTATIONS S'AJUSTENT AUX RÉSULTATS.",
            "LA DÉMOCRATIE EST UN PARTENARIAT PUBLIC-PRIVÉ.",
            "VOTEZ UTILE : LES CHOIX NON CONFORMES ONT ÉTÉ RATIONALISÉS.",
            "STABILITÉ INSTITUTIONNELLE : LA CONTINUITÉ DANS LA RECALIBRATION.",
            "AUCUN RENIEMENT : UNE SIMPLE ADAPTATION PRAGMATIQUE AU MARCHÉ.",
            "AUSTÉRITÉ CIBLÉE : L'EFFORT BUDGETAIRE EST UNE DISCIPLINE COLLECTIVE.",
            "HERMÉTICITÉ DES INDIVIDUS. FLUIDITÉ TOTALE DES CAPITAUX.",
            "ÉGALITÉ JURIDIQUE : LA DÉFENSE RESTANT ACCESSIBLE SUR DEVIS.",
            "CONSULTATION CITOYENNE EFFECTUÉE. DÉCISION TRANSMISE AU COMITÉ FINANCIER.",
            "CROISSANCE INFINIE : LES LIMITES PHYSIQUES NE SONT QU'UN FREIN RÉGLEMENTAIRE.",
            "VOTRE RÉMUNÉRATION EST UNE VARIABLE D'AJUSTEMENT. MERCI DE VOTRE CONTRIBUTION.",
            "VOTEZ EN TOUTE SÉRÉNITÉ. LE MARCHÉ A DÉJÀ VALIDÉ LA SUITE.",
            "EXPIRATION DES ENGAGEMENTS ÉLECTORAUX : DIMANCHE À MINUIT.",
            "L'EXERCICE DU DROT DE VOTE EST UN ACCESSOIRE DE CONFORT.",
            "MANDAT PROLONGÉ : L'ALTERNANCE ÉTAIT UNE SOURCE D'FRAGILITÉ.",
            "LA PAIX SOCIALE EST UN COÛT DE FONCTIONNEMENT AMORTI.",
            "LE SONDAGE N'EST PAS UNE OPINION. C'EST VOTRE CONSIGNE.",
            "LE CITOYEN COMPLAISANT EST UN PARTENAIRE D'AVENIR.",
            "LA LOI NE CHANGE PAS. SEUL SON PRIX S'AJUSTE.",
            "POURQUOI VOTER QUAND L'ALGORITHME CONNAÎT DÉJÀ VOTRE PREFÉRENCE ?",
        ],
    },
    Ad {
        kind: AdKind::Estate,
        accent: SceneColor::Violet,
        slogans: &[
            "SANS-ABRI ? LE SECTEUR FINANCIER A UNE SOLUTION POUR VOTRE CAPITAL.",
            "MODULE RÉSIDENTIEL 9 M² : LA PROXIMITÉ DE L'AUTRE ENFIN OPTIMISÉE.",
            "INVESTISSEZ DANS LA PIERRE : ELLE NE DEMANDE JAMAIS DE CONGÉS PAYÉS.",
            "LOGEMENT RATIONNEL : VOTRE DOSSIER EST UNE PRIORITÉ DE SECOND RANG.",
            "LE LOYER S'AJUSTE EN TEMPS RÉEL À VOS CAPACITÉS DE RÉSISTANCE.",
            "PROPRIÉTAIRE SUR TROIS GÉNÉRATIONS : TRANSMETTEZ VOTRE EMPRUNT AVEC FIERTÉ.",
            "PANORAMIQUE GUARANTI JUSQU'À L'EXTENSION DE LA PHASE 3.",
            "UN CHEZ-SOI N'A PAS DE PRIX. NOUS L'AVONS STRUCTURÉ POUR VOUS.",
            "LE BÉTON EST LA SEULE NATURE QUI NE DÉPÉRIT PAS.",
            "DORMIR EST UN DROIT. DORMIR ICI EST UN SERVICE.",
            "S'ÉLOIGNER DE LA VILLE EST UN LUXE RESSERVÉ AUX INFRASTRUCTURES.",
            "STADE MUNICIPAL : 80 000 SPECTATEURS. UNE SEULE ÉMOTION SYNCHRONISÉE.",
            "DAFT PURGES : DOUZIÈME TOURNÉE D'ADIEU. L'ÉMOTION RESTE MARCHANDE.",
            "RETRANSMISSION EN DIRECT : LE PUBLIC FAIT PARTIE DU DÉCOR LOUABLE.",
            "FESTIVAL MUNI-SECTEUR : LE CAMPEMENT EST UNE CONCESSION. LA BOUE EST EN OPTION.",
            "ACCÈS LIBRE : BILLETERIE ADOSSÉE À VOS MÉTADONNÉES COMPORTEMENTALES.",
            "SOIRÉE EXCLUSIVE SUR LE SITE DE L'ANCIEN PARC. L'ARBRE RESTANT EST SUR L'AFFICHE.",
            "ÉVÉNEMENT COMPLET : LES ALGORITHMES DE REVENTE ONT DÉJÀ DÉTERMINÉ VOTRE VALEUR.",
            "CULTURE DE SYNTHÈSE : VOS SOUVENIRS D'ÉCOUTE SERONT STOCKÉS SANS FRAIS.",
            "NE REGARDEZ PAS LE SPECTACLE. LAISSEZ-LE VOUS TRAVERSER.",
            "LA FOULE EST UN MOTEUR. VOTRE PRÉSENCE EST SON ENERGIE.",
        ],
    },
    Ad {
        kind: AdKind::Brute,
        accent: SceneColor::Alarm,
        slogans: &[
            "ACQUÉREZ. LE DÉSIR SUIVRA.",
            "DÉSIR SYNTHÉTIQUE. SATISFACTION IMMÉDIATE.",
            "POSSÉDEZ. DÉCEVEZ-VOUS. RECOMMENCEZ.",
            "L'AMNÉSIE DU DÉSIR EST NORMALE. VALIDEZ LE PANIER.",
            "-50% SUR L'ACCESSOIRE. -0% SUR VOTRE SURVIE.",
            "URGENCE ARTIFICIELLE : OFFRE LIMITÉE À L'ÉTERNITÉ.",
            "TOUT EST EN SOLDE. SAUF LE TEMPS QU'IL VOUS RESTE.",
            "ACHEMINEMENT GRATUIT. LE COÛT HUMAIN A DÉJÀ ÉTÉ AMORTI.",
            "VALIDEZ. VALIDEZ. VALIDEZ. LA LECTURE ÉTAIT UN FREIN.",
            "LE VIDE EXISTENTIEL N'EST PAS UNE FATALITÉ. C'EST UN MARCHÉ.",
            "VOUS AVEZ UNE ÂME. NOUS AVONS LE FORFAIT ADAPTÉ.",
            "VOTRE DÉSIR ÉTAIT PRÉVISIBLE. NOUS L'AVONS DÉJÀ FACTURÉ.",
            "ATTENTION. ATTACHEMENT. TRANSACTION. DANS CET ORDRE EXACT.",
            "CETTE AFFICHE VALAIT PLUS CHER QUE L'HOMME QUI L'A POSÉE.",
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
        slogans: &["OUVERT 24H/24", "DORMIR COÛTE CHER"],
    },
    Ad {
        kind: AdKind::Neon,
        accent: SceneColor::Mint,
        slogans: &["NOODLES", "SOUPE SYNTHÉTIQUE"],
    },
    Ad {
        kind: AdKind::Neon,
        accent: SceneColor::Amber,
        slogans: &["MOTEL ∞", "CHAMBRE 404"],
    },
    Ad {
        kind: AdKind::Neon,
        accent: SceneColor::Azure,
        slogans: &["CRÉDIT FACILE", "DETTE +"],
    },
    Ad {
        kind: AdKind::Neon,
        accent: SceneColor::Violet,
        slogans: &["RÊVES -70%", "SOLDES D'ÂMES"],
    },
    Ad {
        kind: AdKind::Neon,
        accent: SceneColor::Lime,
        slogans: &["OBÉIS", "SOURIEZ"],
    },
    Ad {
        kind: AdKind::Neon,
        accent: SceneColor::Alarm,
        slogans: &["PROMO", "PROMO ÉTERNELLE"],
    },
    Ad {
        kind: AdKind::Neon,
        accent: SceneColor::Rose,
        slogans: &["TRAVAILLE", "CONSOMME"],
    },
    Ad {
        kind: AdKind::Neon,
        accent: SceneColor::Mint,
        slogans: &["100% BIO*", "*PAS DU TOUT"],
    },
    Ad {
        kind: AdKind::Neon,
        accent: SceneColor::Amber,
        slogans: &["PRIX CASSÉS", "EMPLOYÉS AUSSI"],
    },
    Ad {
        kind: AdKind::Neon,
        accent: SceneColor::Azure,
        slogans: &["À QUOI BON", "ACHETEZ"],
    },
    Ad {
        kind: AdKind::Neon,
        accent: SceneColor::Violet,
        slogans: &["TOUT SE VEND", "VOUS AUSSI"],
    },
    Ad {
        kind: AdKind::Neon,
        accent: SceneColor::Lime,
        slogans: &["LIVRAISON 5 MIN", "LIVREUR 5 MIN"],
    },
    Ad {
        kind: AdKind::Neon,
        accent: SceneColor::Alarm,
        slogans: &["MERCI", "REVENEZ"],
    },
    Ad {
        kind: AdKind::Neon,
        accent: SceneColor::Rose,
        slogans: &["LIKE", "TOI AUSSI"],
    },
    Ad {
        kind: AdKind::Neon,
        accent: SceneColor::Amber,
        slogans: &["SMILE OR ELSE", "SOURIRE OBLIGATOIRE"],
    },
    Ad {
        kind: AdKind::Neon,
        accent: SceneColor::Mint,
        slogans: &["ZÉRO DÉCHET*", "*SAUF VOUS"],
    },
    Ad {
        kind: AdKind::Neon,
        accent: SceneColor::Azure,
        slogans: &["BUY NOW", "CRY LATER"],
    },
    Ad {
        kind: AdKind::Neon,
        accent: SceneColor::Violet,
        slogans: &["REVENDEZ-VOUS", "PRIX DE GROS"],
    },
    Ad {
        kind: AdKind::Neon,
        accent: SceneColor::Lime,
        slogans: &["WIFI GRATUIT", "CERVEAU EN OPTION"],
    },
    Ad {
        kind: AdKind::Neon,
        accent: SceneColor::Rose,
        slogans: &["LAST CALL", "FOREVER"],
    },
    Ad {
        kind: AdKind::Neon,
        accent: SceneColor::Alarm,
        slogans: &["AUJOURD'HUI SEULEMENT", "DEMAIN AUSSI"],
    },
];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn le_catalogue_a_la_forme_attendue() {
        assert_eq!(CATALOGUE.len(), CATALOGUE_LEN);
        assert!(
            CATALOGUE[..PRODUCT_ADS]
                .iter()
                .all(|a| !matches!(a.kind, AdKind::Watch | AdKind::Neon))
        );
        assert!(CATALOGUE[NEON_FIRST..].iter().all(|a| a.kind == AdKind::Neon));
        for id in [WATCH_AD, CAR_FRONT_AD, CAR_SIDE_AD] {
            assert_eq!(id.ad().kind, AdKind::Watch);
        }
        assert!(CATALOGUE.iter().all(|a| !a.slogans.is_empty()));
    }
}
