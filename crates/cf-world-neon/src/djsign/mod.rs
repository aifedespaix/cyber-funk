//! L'enseigne géante du DJ (J7-02) : le nom du DJ en lettres de néon de vingt mètres sur un immeuble au bord de la route
//! (`show/dj-sign*.ts` et `show/stage.ts` de Sillage).
//!
//! - [`layout`], [`anim`], [`frame`], [`sign`] : la logique de Sillage, pure et testée (mise en page des lettres, courbes
//!   d'allumage, ossature, machine à états des lettres) ;
//! - [`director`] : le chef qui pose l'immeuble devant la voiture, tire la teinte et recycle l'enseigne ;
//! - [`glyphs`] : les lettres, cuites une fois à l'ouverture dans un atlas (règle 8) ;
//! - [`material`], [`mesh`], [`rig`] : le rendu (néon additif, ossature en un maillage, pool d'entités) ;
//! - [`plugin`] : le branchement dans l'application et le sujet filmable `dj-sign` (publié par `NeonWorld`).
//!
//! # Photosensibilité (règle 7)
//!
//! Sillage faisait bégayer l'allumage (trois flashs par lettre en 0,36 s) et grésiller une lettre à 26 bascules par
//! seconde. Ici : un flash par lettre et au moins 0,34 s entre deux allumages, grésillement à 3 bascules par seconde, une
//! lettre à la fois ; la lumière d'une lettre ne varie jamais plus vite que `MIN_FADE` ; le pouls des kicks passe par le
//! limiteur de `cf-show` ([`DjSignPulse`]) ; flashs coupés, respiration ou mode calme : allumage en fondu, sans vague,
//! sans grésillement ni pouls.

pub mod anim;
pub mod director;
pub mod frame;
pub mod glyphs;
pub mod layout;
pub mod material;
pub mod mesh;
pub mod plugin;
pub mod rig;
pub mod sign;

pub use director::{DjSignDirector, ShowView, SignContext, SitePlan};
pub use layout::{SIGN_LAYOUT, SignLayout, layout_sign};
pub use material::{LetterMaterial, LetterParams};
pub(crate) use plugin::step_sign;
pub use plugin::{DjSignIntro, DjSignPlugin, DjSignPulse, DjSignRes, DjSignSettings};
pub use rig::{DjSignGlyphs, DjSignRig};
pub use sign::{DjSign, LetterPose, SignInput};
