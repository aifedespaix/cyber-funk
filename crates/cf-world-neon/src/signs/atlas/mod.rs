//! L'atlas des enseignes : toutes les textures 2D, cuites une fois à l'ouverture et rangées en pages.
//!
//! - **Cellules** : 512 × 256 pixels, quatre canaux de couverture (produits, néons, caméras, panneaux « DJ ») ou un seul
//!   (projections). Huit par rangée, jusqu'à 4 096 pixels de haut par page.
//! - **Bandeaux** : une bande d'un canal par (publicité, slogan de départ), une par rangée de 40 pixels.
//!
//! Rien ici ne touche à Bevy : [`BakedAtlas::build`] rend des octets, que `systems` envoie au GPU. C'est ce qui permet de
//! mesurer le temps de cuisson (critère : moins de 2 s) sans fenêtre.

mod jobs;

use std::collections::HashMap;

use cf_render::SceneColor;

pub use jobs::{Baked, Job, MAX_LIVE_SLOTS, bake, bake_all, bake_all_measured, jobs, thread_cpu_s};

use super::ads::AdId;
use super::formats::{CANVAS_H, CANVAS_W, TICKER_H};
use super::paint::{Bitmap, Pixels, Strip};

/// Largeur d'une page, en pixels.
pub const PAGE_W: u32 = 4096;
/// Hauteur maximale d'une page, en pixels.
pub const PAGE_MAX_H: u32 = 4096;
/// Cellules par rangée.
const COLS: u32 = PAGE_W / CANVAS_W;
/// Rangées de cellules par page.
const CELL_ROWS: u32 = PAGE_MAX_H / CANVAS_H;
/// Rangées de bandeaux par page.
const STRIP_ROWS: u32 = PAGE_MAX_H / TICKER_H;

/// Le canal `R` de cette cellule, soumis au limiteur de flashs (stroboscope de « ACHÈTE MOI »).
pub const FLAG_GATE_R: u32 = 1;
/// Cette cellule porte un verdict (« GAME OVER ») dans son canal `K` : il n'apparaît que sous le flash du limiteur.
pub const FLAG_VERDICT: u32 = 2;

/// La couleur du canal `B` d'une cellule.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Secondary {
    /// Un rôle de la palette.
    Role(SceneColor),
    /// La couleur d'accent du panneau (néons : les tubes morts).
    Accent,
    /// Le canal est vide.
    None,
}

/// Une cellule de l'atlas.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum CellKey {
    /// Le visuel « produit » d'une publicité (hologramme, caisson).
    Product {
        /// La publicité.
        ad: AdId,
        /// Le rang du slogan affiché.
        slogan: u8,
    },
    /// La projection du slogan (un canal).
    Ghost {
        /// La publicité.
        ad: AdId,
        /// Le rang du slogan.
        slogan: u8,
    },
    /// L'enseigne en tubes d'une publicité néon.
    Neon {
        /// La publicité.
        ad: AdId,
        /// Le rang du slogan.
        slogan: u8,
    },
    /// L'habillage commun des écrans-caméra.
    Cam,
    /// L'affiche de concert du DJ d'un créneau.
    LivePoster {
        /// Rang du créneau.
        slot: u8,
    },
    /// L'écran géant du DJ d'un créneau.
    LiveScreen {
        /// Rang du créneau.
        slot: u8,
    },
    /// L'enseigne néon du DJ (un mot d'accompagnement à choisir).
    LiveNeon {
        /// Rang du créneau.
        slot: u8,
        /// Rang du mot (`NEON_TAGS`).
        tag: u8,
    },
    /// La projection de l'événement (ou du DJ, si l'événement n'a pas de nom).
    LiveGhost {
        /// Rang du créneau.
        slot: u8,
        /// Rang du mot, 0 si l'événement a un nom.
        tag: u8,
    },
}

/// À quelle publicité appartient un bandeau.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum StripAd {
    /// Une publicité du catalogue.
    Catalogue(AdId),
    /// L'affiche de concert d'un créneau.
    LivePoster {
        /// Rang du créneau.
        slot: u8,
    },
    /// L'écran géant d'un créneau.
    LiveScreen {
        /// Rang du créneau.
        slot: u8,
    },
}

/// Un bandeau : sa publicité et le slogan par lequel il commence.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct StripKey {
    /// La publicité.
    pub ad: StripAd,
    /// Rang du premier slogan.
    pub start: u8,
}

/// Où est une cellule.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CellInfo {
    /// Page (indice dans [`BakedAtlas::pages`]).
    pub page: u16,
    /// Coin haut gauche, en pixels.
    pub x: u16,
    /// Coin haut gauche, en pixels.
    pub y: u16,
    /// Couleur du canal `B`.
    pub secondary: Secondary,
    /// `FLAG_*`.
    pub flags: u32,
}

/// Où est un bandeau.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct StripInfo {
    /// Page (indice dans [`BakedAtlas::pages`]).
    pub page: u16,
    /// Haut de la bande, en pixels (le bord gauche est à 0).
    pub y: u16,
    /// Largeur de la bande, en pixels.
    pub width: u16,
    /// Longueur exacte de la boucle, en pixels.
    pub period: f32,
}

/// Format des pixels d'une page.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PageFormat {
    /// Quatre canaux.
    Rgba,
    /// Un canal.
    R8,
}

/// Une page de l'atlas, prête à partir au GPU.
#[derive(Debug)]
pub struct PageData {
    /// Format des pixels.
    pub format: PageFormat,
    /// Largeur, en pixels.
    pub w: u32,
    /// Hauteur, en pixels.
    pub h: u32,
    /// Les pixels.
    pub data: Vec<u8>,
}

impl PageData {
    fn new(format: PageFormat, rows: u32, row_h: u32) -> Self {
        let bpp = match format {
            PageFormat::Rgba => 4,
            PageFormat::R8 => 1,
        };
        let h = rows * row_h;
        Self {
            format,
            w: PAGE_W,
            h,
            data: vec![0; (PAGE_W * h * bpp) as usize],
        }
    }

    const fn bpp(&self) -> usize {
        match self.format {
            PageFormat::Rgba => 4,
            PageFormat::R8 => 1,
        }
    }

    /// Recopie `bitmap` (format identique à la page) en (x, y).
    fn blit(&mut self, x: u32, y: u32, bitmap: &Bitmap) {
        let src = match &bitmap.pixels {
            Pixels::Rgba(d) | Pixels::R8(d) => d,
        };
        let bpp = self.bpp();
        let row_bytes = bitmap.w as usize * bpp;
        for row in 0..bitmap.h as usize {
            let from = row * row_bytes;
            let to = ((y as usize + row) * self.w as usize + x as usize) * bpp;
            if let (Some(s), Some(d)) = (src.get(from..from + row_bytes), self.data.get_mut(to..to + row_bytes)) {
                d.copy_from_slice(s);
            }
        }
    }
}

/// L'atlas cuit : les pages et l'index des cellules et des bandeaux.
#[derive(Debug, Default)]
pub struct BakedAtlas {
    /// Les pages.
    pub pages: Vec<PageData>,
    /// Les cellules.
    pub cells: HashMap<CellKey, CellInfo>,
    /// Les bandeaux.
    pub strips: HashMap<StripKey, StripInfo>,
    /// Cuissons qui ont échoué (canevas impossible) : leurs enseignes resteront vides.
    pub failed: usize,
}

/// Remplit des pages ligne après ligne.
struct Shelf {
    format: PageFormat,
    row_h: u32,
    rows_per_page: u32,
    cols: u32,
    /// Nombre d'éléments par page déjà posés.
    placed: Vec<u32>,
    pages: Vec<Vec<(u32, u32, Bitmap)>>,
}

impl Shelf {
    fn new(format: PageFormat, row_h: u32, rows_per_page: u32, cols: u32) -> Self {
        Self {
            format,
            row_h,
            rows_per_page,
            cols,
            placed: Vec::new(),
            pages: Vec::new(),
        }
    }

    /// Réserve une place ; rend (page locale, x, y).
    fn put(&mut self, bitmap: Bitmap) -> (usize, u32, u32) {
        let per_page = self.cols * self.rows_per_page;
        if self.placed.last().is_none_or(|n| *n >= per_page) {
            self.placed.push(0);
            self.pages.push(Vec::new());
        }
        let page = self.pages.len() - 1;
        let n = self.placed[page];
        self.placed[page] += 1;
        let (x, y) = ((n % self.cols) * (PAGE_W / self.cols), (n / self.cols) * self.row_h);
        self.pages[page].push((x, y, bitmap));
        (page, x, y)
    }

    fn finish(self, out: &mut Vec<PageData>) -> usize {
        let base = out.len();
        for (index, items) in self.pages.into_iter().enumerate() {
            let rows = self.placed[index].div_ceil(self.cols);
            let mut page = PageData::new(self.format, rows.max(1), self.row_h);
            for (x, y, bitmap) in &items {
                page.blit(*x, *y, bitmap);
            }
            out.push(page);
        }
        base
    }
}

/// Ce que la cuisson a coûté, en secondes de temps CPU (voir [`thread_cpu_s`]).
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct BuildCost {
    /// Temps CPU de tous les fils de cuisson ensemble ; `None` si le noyau ne le mesure pas.
    pub workers_cpu_s: Option<f64>,
    /// Temps CPU du fil qui range les pages (le fil appelant : les fils de cuisson sont d'autres fils).
    pub assembly_cpu_s: Option<f64>,
}

impl BuildCost {
    /// La durée qu'aurait la cuisson sur `threads` fils sur une machine au repos : le travail réparti également (les
    /// fils prennent leur prochaine tâche dans une file commune, l'écart est d'une tâche) plus le rangement, qui est
    /// sériel. `None` si le temps CPU n'est pas mesurable.
    #[must_use]
    pub fn idle_wall_s(&self, threads: usize) -> Option<f64> {
        Some(self.workers_cpu_s? / threads.max(1) as f64 + self.assembly_cpu_s?)
    }
}

impl BakedAtlas {
    /// Cuit tout l'atlas pour les créneaux donnés, sur `threads` fils.
    #[must_use]
    pub fn build(slots: &[super::show_signs::SlotNames], threads: usize) -> Self {
        Self::build_measured(slots, threads).0
    }

    /// Comme [`BakedAtlas::build`], et rend aussi ce que la cuisson a coûté en temps CPU : le budget de chargement se
    /// tient sur ce coût, que la charge de la machine ne fausse pas, et non sur le temps mural.
    #[must_use]
    pub fn build_measured(slots: &[super::show_signs::SlotNames], threads: usize) -> (Self, BuildCost) {
        let assembly_start = thread_cpu_s();
        let list = jobs(slots);
        let (baked, workers_cpu_s) = bake_all_measured(&list, slots, threads);
        let mut rgba = Shelf::new(PageFormat::Rgba, CANVAS_H, CELL_ROWS, COLS);
        let mut r8 = Shelf::new(PageFormat::R8, CANVAS_H, CELL_ROWS, COLS);
        let mut strips = Shelf::new(PageFormat::R8, TICKER_H, STRIP_ROWS, 1);
        // Premier passage : on range, en gardant la page locale ; le décalage des pages s'ajoute au second.
        let mut cell_places: Vec<(CellKey, bool, usize, u32, u32, Secondary, u32)> = Vec::new();
        let mut strip_places: Vec<(StripKey, usize, u32, u32, f32)> = Vec::new();
        let mut failed = 0;
        for (job, result) in list.iter().zip(baked) {
            match (job, result) {
                (
                    Job::Cell(key),
                    Some(Baked::Cell {
                        bitmap,
                        secondary,
                        flags,
                    }),
                ) => {
                    let is_rgba = matches!(bitmap.pixels, Pixels::Rgba(_));
                    let (page, x, y) = if is_rgba { rgba.put(bitmap) } else { r8.put(bitmap) };
                    cell_places.push((*key, is_rgba, page, x, y, secondary, flags));
                }
                (Job::Strip(key), Some(Baked::Strip(Strip { bitmap, period }))) => {
                    let width = bitmap.w;
                    let (page, _, y) = strips.put(bitmap);
                    strip_places.push((*key, page, y, width, period));
                }
                _ => failed += 1,
            }
        }
        let mut pages = Vec::new();
        let rgba_base = rgba.finish(&mut pages);
        let r8_base = r8.finish(&mut pages);
        let strip_base = strips.finish(&mut pages);
        let mut atlas = Self {
            pages,
            cells: HashMap::new(),
            strips: HashMap::new(),
            failed,
        };
        for (key, is_rgba, page, x, y, secondary, flags) in cell_places {
            let base = if is_rgba { rgba_base } else { r8_base };
            atlas.cells.insert(
                key,
                CellInfo {
                    page: (base + page) as u16,
                    x: x as u16,
                    y: y as u16,
                    secondary,
                    flags,
                },
            );
        }
        for (key, page, y, width, period) in strip_places {
            atlas.strips.insert(
                key,
                StripInfo {
                    page: (strip_base + page) as u16,
                    y: y as u16,
                    width: width as u16,
                    period,
                },
            );
        }
        let cost = BuildCost {
            workers_cpu_s,
            assembly_cpu_s: thread_cpu_s().zip(assembly_start).map(|(end, start)| end - start),
        };
        (atlas, cost)
    }

    /// Mémoire des pages, en octets.
    #[must_use]
    pub fn bytes(&self) -> usize {
        self.pages.iter().map(|p| p.data.len()).sum()
    }
}

#[cfg(test)]
mod tests;
