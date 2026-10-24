//! Le travail de cuisson : quelles cellules et quels bandeaux existent, comment chacun se dessine, et la cuisson en
//! parallèle (chaque fil a son peintre ; le résultat ne dépend pas du nombre de fils).

use std::sync::atomic::{AtomicUsize, Ordering};

use cf_render::SceneColor;

use super::{CellKey, FLAG_GATE_R, FLAG_VERDICT, Secondary, StripAd, StripKey};
use crate::rng::hash;
use crate::signs::ads::{AdId, AdKind, CATALOGUE};
use crate::signs::icons::icon_for;
use crate::signs::paint::{Bitmap, Draw, Painter, Strip};
use crate::signs::show_signs::{NEON_TAGS, SlotNames};

/// Une chose à cuire.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Job {
    /// Une cellule d'image.
    Cell(CellKey),
    /// Un bandeau défilant.
    Strip(StripKey),
}

/// Le résultat d'une cuisson.
#[derive(Debug)]
pub enum Baked {
    /// Une cellule, avec la couleur de son canal `B` et ses drapeaux.
    Cell {
        /// L'image.
        bitmap: Bitmap,
        /// Couleur du canal `B`.
        secondary: Secondary,
        /// `FLAG_*`.
        flags: u32,
    },
    /// Un bandeau.
    Strip(Strip),
}

/// Le plus grand nombre de créneaux dont on cuit les panneaux « DJ » (au-delà, les derniers n'ont pas de panneau).
pub const MAX_LIVE_SLOTS: usize = 12;

/// La couleur propre au canal `B` d'une publicité (voir `paint::arts`).
const fn secondary_of(kind: AdKind) -> Secondary {
    match kind {
        AdKind::Food => Secondary::Role(SceneColor::Alarm),
        AdKind::Fashion | AdKind::Escape => Secondary::Role(SceneColor::Mint),
        AdKind::Mind | AdKind::Politics | AdKind::Estate | AdKind::Brute => Secondary::Role(SceneColor::Rose),
        AdKind::Watch => Secondary::Role(SceneColor::Ice),
        AdKind::Neon => Secondary::Accent,
    }
}

/// Toutes les cuissons de l'ouverture, dans un ordre fixe (le rangement dans l'atlas en dépend).
#[must_use]
pub fn jobs(slots: &[SlotNames]) -> Vec<Job> {
    let mut out = Vec::new();
    for (index, ad) in CATALOGUE.iter().enumerate() {
        let id = AdId(index as u8);
        for slogan in 0..ad.slogans.len() as u8 {
            match ad.kind {
                AdKind::Neon => out.push(Job::Cell(CellKey::Neon { ad: id, slogan })),
                AdKind::Watch => {}
                _ => {
                    out.push(Job::Cell(CellKey::Product { ad: id, slogan }));
                    out.push(Job::Cell(CellKey::Ghost { ad: id, slogan }));
                }
            }
            if !matches!(ad.kind, AdKind::Neon) {
                out.push(Job::Strip(StripKey {
                    ad: StripAd::Catalogue(id),
                    start: slogan,
                }));
            }
        }
    }
    out.push(Job::Cell(CellKey::Cam));
    for (slot, names) in slots.iter().take(MAX_LIVE_SLOTS).enumerate() {
        if names.name.is_empty() {
            continue;
        }
        let slot = slot as u8;
        out.push(Job::Cell(CellKey::LivePoster { slot }));
        out.push(Job::Cell(CellKey::LiveScreen { slot }));
        for tag in 0..NEON_TAGS.len() as u8 {
            out.push(Job::Cell(CellKey::LiveNeon { slot, tag }));
        }
        let ghost_tags = if names.event.is_empty() { NEON_TAGS.len() as u8 } else { 1 };
        for tag in 0..ghost_tags {
            out.push(Job::Cell(CellKey::LiveGhost { slot, tag }));
        }
        out.push(Job::Strip(StripKey {
            ad: StripAd::LivePoster { slot },
            start: 1,
        }));
        for start in 0..2 {
            out.push(Job::Strip(StripKey {
                ad: StripAd::LiveScreen { slot },
                start,
            }));
        }
    }
    out
}

/// Les slogans d'un bandeau, dans l'ordre où il les enchaîne : à partir de `start`, en boucle.
fn rotated(texts: &[String], start: usize) -> Vec<String> {
    (0..texts.len()).map(|i| texts[(start + i) % texts.len()].clone()).collect()
}

fn live_texts(key: StripAd, names: &SlotNames) -> Vec<String> {
    match key {
        StripAd::LivePoster { .. } => vec![names.name.clone(), format!("{} LIVE", names.name)],
        _ => vec![
            names.name.clone(),
            if names.event.is_empty() {
                "EN DIRECT".to_owned()
            } else {
                names.event.clone()
            },
        ],
    }
}

/// Cuit une chose. `None` si le dessin a échoué (canevas impossible) : la cellule manquera, l'enseigne restera vide.
pub fn bake(painter: &mut Painter, job: Job, slots: &[SlotNames]) -> Option<Baked> {
    match job {
        Job::Strip(key) => {
            let texts: Vec<String> = match key.ad {
                StripAd::Catalogue(id) => id.ad().slogans.iter().map(|s| (*s).to_owned()).collect(),
                StripAd::LivePoster { slot } | StripAd::LiveScreen { slot } => {
                    live_texts(key.ad, slots.get(usize::from(slot))?)
                }
            };
            painter
                .strip(&rotated(&texts, usize::from(key.start)))
                .map(Baked::Strip)
        }
        Job::Cell(key) => Some(bake_cell(painter, key, slots)?),
    }
}

fn bake_cell(painter: &mut Painter, key: CellKey, slots: &[SlotNames]) -> Option<Baked> {
    let names = |slot: u8| slots.get(usize::from(slot));
    let cell = |bitmap: Bitmap, secondary: Secondary, flags: u32| Baked::Cell {
        bitmap,
        secondary,
        flags,
    };
    Some(match key {
        CellKey::Product { ad, slogan } => {
            let a = ad.ad();
            let text = a.slogans[usize::from(slogan) % a.slogans.len()];
            let rank = usize::from(slogan);
            let flags = match a.kind {
                AdKind::Escape => FLAG_VERDICT,
                AdKind::Brute if rank % 2 == 0 => FLAG_GATE_R,
                _ => 0,
            };
            let bitmap = painter.product(&Draw {
                ad: a,
                slogan: text,
                rank,
                icon: icon_for(a.kind, text),
                live: None,
            });
            cell(bitmap, secondary_of(a.kind), flags)
        }
        CellKey::Ghost { ad, slogan } => {
            let a = ad.ad();
            cell(painter.ghost(a.slogans[usize::from(slogan) % a.slogans.len()]), Secondary::None, 0)
        }
        CellKey::Neon { ad, slogan } => {
            let a = ad.ad();
            let seed = hash(f64::from(ad.0) * 7.0 + f64::from(slogan), 3.0) as f32 * 6.28;
            let text = a.slogans[usize::from(slogan) % a.slogans.len()];
            cell(painter.neon(text, seed), Secondary::Accent, 0)
        }
        CellKey::Cam => cell(painter.cam(None), Secondary::None, 0),
        CellKey::LivePoster { slot } => {
            let n = names(slot)?;
            let estate = &CATALOGUE[estate_index()];
            let text = format!("{} LIVE", n.name);
            let bitmap = painter.product(&Draw {
                ad: estate,
                slogan: &text,
                rank: 1,
                icon: icon_for(estate.kind, &text),
                live: Some((&n.name, &n.event)),
            });
            cell(bitmap, secondary_of(estate.kind), 0)
        }
        CellKey::LiveScreen { slot } => {
            let n = names(slot)?;
            cell(painter.cam(Some((&n.name, &n.event))), Secondary::None, 0)
        }
        CellKey::LiveNeon { slot, tag } => {
            let n = names(slot)?;
            let text = format!("{} {}", NEON_TAGS[usize::from(tag) % NEON_TAGS.len()], n.name);
            cell(painter.neon(&text, hash(f64::from(slot), 3.0) as f32 * 6.28), Secondary::Accent, 0)
        }
        CellKey::LiveGhost { slot, tag } => {
            let n = names(slot)?;
            let text = if n.event.is_empty() {
                format!("{} {}", NEON_TAGS[usize::from(tag) % NEON_TAGS.len()], n.name)
            } else {
                n.event.clone()
            };
            cell(painter.ghost(&text), Secondary::None, 0)
        }
    })
}

/// Rang de l'affiche de concert dans le catalogue (la publicité « immobilier »).
fn estate_index() -> usize {
    CATALOGUE.iter().position(|a| a.kind == AdKind::Estate).unwrap_or(0)
}

/// Cuit `jobs` sur `threads` fils. Le résultat est dans l'ordre de `jobs`, quel que soit le nombre de fils.
#[must_use]
pub fn bake_all(jobs: &[Job], slots: &[SlotNames], threads: usize) -> Vec<Option<Baked>> {
    let next = AtomicUsize::new(0);
    let threads = threads.clamp(1, 16);
    let mut done: Vec<(usize, Option<Baked>)> = std::thread::scope(|scope| {
        let workers: Vec<_> = (0..threads)
            .map(|_| {
                scope.spawn(|| {
                    let mut out = Vec::new();
                    let Some(mut painter) = Painter::new() else { return out };
                    loop {
                        let i = next.fetch_add(1, Ordering::Relaxed);
                        let Some(job) = jobs.get(i) else { break };
                        out.push((i, bake(&mut painter, *job, slots)));
                    }
                    out
                })
            })
            .collect();
        workers
            .into_iter()
            .flat_map(|w| w.join().unwrap_or_default())
            .collect()
    });
    done.sort_by_key(|(i, _)| *i);
    let mut out: Vec<Option<Baked>> = (0..jobs.len()).map(|_| None).collect();
    for (i, baked) in done {
        out[i] = baked;
    }
    out
}
