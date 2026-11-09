//! Les panneaux de l'onglet Debug : le catalogue que le monde publie, et l'ordre « pose-moi celui-là devant la voiture ».
//!
//! La fenêtre opérateur ne dépend d'aucun monde : elle lit [`SignCatalog`] (les formats de panneau et les textes de toutes
//! les publicités, publiés au démarrage par `cf-world-neon::signs`) et écrit des [`SignOrder`] que le monde exécute.
//! Les formats et les publicités sont désignés par leur rang dans le catalogue.

use bevy::prelude::*;

/// Où un panneau se pose : de quoi grouper la liste.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum SignGroup {
    /// Au-dessus ou au bord des voies : portique, potence, portique-caméra.
    Road,
    /// Sur les côtés de la route : hologramme, enseigne en néon, projection dans le ciel.
    Side,
    /// Sur les immeubles : panneau de toit, écran géant.
    Roof,
}

impl SignGroup {
    /// Les groupes, dans l'ordre de la liste.
    pub const ALL: [Self; 3] = [Self::Road, Self::Side, Self::Roof];

    /// Titre du groupe.
    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::Road => "Sur la route",
            Self::Side => "Sur les côtés",
            Self::Roof => "Sur les immeubles",
        }
    }
}

/// Un format de panneau.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SignFormatInfo {
    /// Identifiant stable (`"gantry"`, `"holo"`…).
    pub key: &'static str,
    /// Nom affiché.
    pub label: &'static str,
    /// Une phrase pour l'infobulle.
    pub hint: &'static str,
    /// Où il se pose.
    pub group: SignGroup,
    /// Les publicités qu'il sait afficher (rangs dans [`SignCatalog::ads`]).
    pub ads: Vec<u8>,
}

/// Une publicité : un visuel et ses textes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SignAdInfo {
    /// Nom du visuel (« Viande », « Néon 3 »…).
    pub label: String,
    /// Les textes, un par rotation.
    pub slogans: Vec<&'static str>,
}

/// Ce que le monde sait poser comme panneaux.
#[derive(Resource, Debug, Clone, Default, PartialEq, Eq)]
pub struct SignCatalog {
    /// Les formats, dans l'ordre de la liste.
    pub formats: Vec<SignFormatInfo>,
    /// Les publicités, par rang.
    pub ads: Vec<SignAdInfo>,
}

impl SignCatalog {
    /// Le catalogue est vide (monde absent ou pas encore publié).
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.formats.is_empty()
    }

    /// Le premier format qui sait afficher cette publicité, en préférant `preferred` s'il le sait aussi.
    #[must_use]
    pub fn format_for_ad(&self, ad: u8, preferred: Option<usize>) -> Option<usize> {
        let knows = |i: usize| self.formats.get(i).is_some_and(|f| f.ads.contains(&ad));
        preferred
            .filter(|&i| knows(i))
            .or_else(|| (0..self.formats.len()).find(|&i| knows(i)))
    }
}

/// Pose ce panneau devant la voiture, dans le champ de la caméra.
#[derive(Message, Debug, Clone, Copy, PartialEq, Eq)]
pub struct SignOrder {
    /// Rang du format dans [`SignCatalog::formats`].
    pub format: u8,
    /// Rang de la publicité dans [`SignCatalog::ads`] ; `None` : celle que la route aurait tirée.
    pub ad: Option<u8>,
    /// Rang du texte ; `None` : celui que la route aurait tiré.
    pub slogan: Option<u8>,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn catalog() -> SignCatalog {
        let format = |key, group, ads: &[u8]| SignFormatInfo {
            key,
            label: key,
            hint: "",
            group,
            ads: ads.to_vec(),
        };
        SignCatalog {
            formats: vec![
                format("gantry", SignGroup::Road, &[0, 1]),
                format("holo", SignGroup::Side, &[0, 1]),
                format("neon", SignGroup::Side, &[2]),
            ],
            ads: Vec::new(),
        }
    }

    #[test]
    fn le_format_choisi_est_garde_quand_il_sait_afficher_la_publicite() {
        let catalog = catalog();
        assert_eq!(catalog.format_for_ad(1, Some(1)), Some(1));
        assert_eq!(catalog.format_for_ad(1, None), Some(0));
        assert_eq!(
            catalog.format_for_ad(2, Some(0)),
            Some(2),
            "le néon est le seul à la connaître"
        );
        assert_eq!(catalog.format_for_ad(9, Some(0)), None);
    }
}
