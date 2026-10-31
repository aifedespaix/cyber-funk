//! Le plugin des cartons-titres : écoute le spectacle, pose les `Text2d` et les anime.

use bevy::asset::RenderAssetUsages;
use bevy::prelude::*;
use bevy::render::render_resource::{Extent3d, TextureDimension, TextureFormat};
use bevy::text::FontSource;
use bevy::window::PrimaryWindow;
use cf_show::director::ShowEvent;

use super::card::{Timing, TitleCard, TitleKind, font_sizes, look_at};
use crate::audio::AudioFeatures;
use crate::layers::OverlayLayer;
use crate::palette::{SceneColor, ScenePalette};
use crate::schedule::CfSet;
use crate::show::{CurrentShow, ShowEventMessage, ShowSheet};

/// Noms des niveaux 0 à 5 (`show.levels` de Sillage, en français).
const LEVEL_NAMES: [&str; 6] = ["Avant-scène", "Contact", "La ville", "Envol", "Démesure", "Cosmos"];

/// Ligne au-dessus du nom du DJ (`titleDj`).
const SUB_DJ: &str = "Aux platines";

/// Largeur supposée quand aucune fenêtre n'existe (tests).
const DEFAULT_WIDTH_PX: f32 = 1920.0;

/// Côté du dégradé radial du voile (px) : petit, il est étiré.
const VEIL_SIZE: u32 = 64;

/// Opacité maximale du voile derrière le texte (78 % au centre dans Sillage, ramené à 60 % : le texte a déjà du corps).
const VEIL_ALPHA: f32 = 0.6;

/// Réglages des cartons.
#[derive(Resource, Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct TitleSettings {
    /// « Réduire les animations » : seulement des fondus, sans échelle ni filet qui se trace.
    pub reduce_motion: bool,
}

/// Le carton à l'écran, s'il y en a un.
#[derive(Resource, Debug, Default)]
pub struct ActiveTitle {
    card: Option<TitleCard>,
    timing: Option<Timing>,
    elapsed_s: f32,
    next_id: u32,
}

impl ActiveTitle {
    /// Le carton affiché.
    #[must_use]
    pub fn card(&self) -> Option<&TitleCard> {
        self.card.as_ref()
    }

    /// Montre `card` à la place du précédent.
    fn show(&mut self, kind: TitleKind, text: String, sub: Option<String>, bpm: Option<f32>) {
        self.next_id += 1;
        self.timing = Some(Timing::of(kind, bpm));
        self.elapsed_s = 0.0;
        self.card = Some(TitleCard {
            id: self.next_id,
            kind,
            text,
            sub,
            bpm,
        });
    }
}

/// Police et voile, cuits à l'ouverture (règle 8).
#[derive(Resource, Debug, Default)]
struct TitleAssets {
    font: Option<Handle<Font>>,
    veil: Option<Handle<Image>>,
}

/// Quel élément d'un carton une entité est.
#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
enum Part {
    Title,
    Sub,
    Rule,
    Veil,
}

/// Ce que l'animation règle sur un élément : sa pose, et sa couleur (texte ou sprite).
type AnimatedPart = (
    &'static Part,
    &'static mut Transform,
    Option<&'static mut TextColor>,
    Option<&'static mut Sprite>,
);

/// Les entités d'un carton, retirées ensemble.
#[derive(Component, Debug)]
struct TitleEntity;

/// Installe les cartons-titres. À ajouter avec les couches (`LayersPlugin`) : les `Text2d` sont rendus par la caméra de
/// la surcouche.
#[derive(Debug, Default, Clone, Copy)]
pub struct TitlesPlugin;

impl Plugin for TitlesPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<TitleSettings>()
            .init_resource::<ActiveTitle>()
            .init_resource::<TitleAssets>()
            .add_systems(Startup, bake_assets)
            .add_systems(
                Update,
                (listen, spawn_card.run_if(card_changed), animate)
                    .chain()
                    .in_set(CfSet::Present),
            );
    }
}

/// `Startup` : charge la police et cuit le voile.
fn bake_assets(
    mut assets: ResMut<TitleAssets>,
    server: Option<Res<AssetServer>>,
    images: Option<ResMut<Assets<Image>>>,
) {
    if let Some(server) = server {
        assets.font = Some(server.load("fonts/DejaVuSans-Bold.ttf"));
    }
    if let Some(mut images) = images {
        assets.veil = Some(images.add(radial_veil()));
    }
}

/// Dégradé radial noir : opaque au centre, transparent aux bords (`radial-gradient(ellipse …)`).
fn radial_veil() -> Image {
    let size = VEIL_SIZE as usize;
    let mut data = Vec::with_capacity(size * size * 4);
    for y in 0..size {
        for x in 0..size {
            let dx = (x as f32 + 0.5) / size as f32 * 2.0 - 1.0;
            let dy = (y as f32 + 0.5) / size as f32 * 2.0 - 1.0;
            let r = (dx * dx + dy * dy).sqrt().min(1.0);
            // 100 % au centre, ~65 % à mi-rayon, 0 au bord.
            let alpha = (1.0 - r).powf(0.9) * 255.0;
            data.extend_from_slice(&[0, 0, 0, alpha as u8]);
        }
    }
    Image::new(
        Extent3d {
            width: VEIL_SIZE,
            height: VEIL_SIZE,
            depth_or_array_layers: 1,
        },
        TextureDimension::D2,
        data,
        TextureFormat::Rgba8UnormSrgb,
        RenderAssetUsages::RENDER_WORLD,
    )
}

/// Le nom du DJ du créneau `slot`, ou « DJ n » s'il n'en a pas.
fn dj_name(sheet: Option<&ShowSheet>, slot: usize) -> String {
    let name = sheet.and_then(|s| s.slots.get(slot)).map_or("", |s| s.dj.trim());
    if name.is_empty() {
        format!("DJ {}", slot + 1)
    } else {
        name.to_owned()
    }
}

/// `CfSet::Present` : transforme les faits du spectacle en cartons.
fn listen(
    mut messages: MessageReader<ShowEventMessage>,
    show: Option<Res<CurrentShow>>,
    sheet: Option<Res<ShowSheet>>,
    features: Option<Res<AudioFeatures>>,
    mut active: ResMut<ActiveTitle>,
    mut last_level: Local<Option<u8>>,
) {
    let bpm = features.and_then(|f| f.0.bpm);
    for message in messages.read() {
        match &message.0 {
            ShowEvent::Title(text) => active.show(TitleKind::Drop, text.to_string(), None, bpm),
            ShowEvent::Handover { to: slot, .. } | ShowEvent::ShowDj { slot } => {
                active.show(
                    TitleKind::Dj,
                    dj_name(sheet.as_deref(), *slot),
                    Some(SUB_DJ.to_owned()),
                    bpm,
                );
            }
            _ => {}
        }
    }
    // Le niveau n'est pas un message : on regarde l'état. Le premier niveau vu n'annonce rien (ouverture, reprise).
    let Some(level) = show.map(|s| s.0.level) else { return };
    let previous = last_level.replace(level);
    if previous.is_some_and(|p| p != level) && level > 0 {
        let name = LEVEL_NAMES.get(usize::from(level)).copied().unwrap_or("");
        active.show(TitleKind::Level, name.to_owned(), Some(format!("Niveau {level}")), bpm);
    }
}

fn card_changed(active: Res<ActiveTitle>, shown: Query<&TitleEntity>) -> bool {
    // Un carton neuf (temps écoulé à zéro), ou des restes sans carton.
    (active.is_changed() && active.elapsed_s == 0.0 && active.card.is_some())
        || (active.card.is_none() && !shown.is_empty())
}

fn tone(palette: Option<&ScenePalette>, color: SceneColor, fallback: Color) -> Color {
    palette.map_or(fallback, |p| Color::LinearRgba(p.get(color).to_bevy()))
}

/// Pose les entités du carton courant (et retire celles de l'ancien).
fn spawn_card(
    mut commands: Commands,
    active: Res<ActiveTitle>,
    assets: Res<TitleAssets>,
    palette: Option<Res<ScenePalette>>,
    window: Query<&Window, With<PrimaryWindow>>,
    old: Query<Entity, With<TitleEntity>>,
) {
    for entity in &old {
        commands.entity(entity).despawn();
    }
    let Some(card) = active.card.as_ref() else { return };
    let width = window.iter().next().map_or(DEFAULT_WIDTH_PX, Window::width);
    let (title_px, sub_px) = font_sizes(card.kind, width);
    let palette = palette.as_deref();
    let ice = tone(palette, SceneColor::Ice, Color::WHITE);
    let mint = tone(palette, SceneColor::Mint, Color::WHITE);
    let title_color = if card.kind == TitleKind::Dj { mint } else { ice };
    let sub_color = if card.kind == TitleKind::Level { mint } else { ice };
    let font = assets.font.clone().map_or_else(FontSource::default, FontSource::Handle);
    let font_of = |px: f32| TextFont {
        font: font.clone(),
        font_size: FontSize::Px(px),
        ..default()
    };

    if let Some(veil) = assets.veil.clone() {
        let ink = tone(palette, SceneColor::Ink, Color::BLACK);
        let mut sprite = Sprite::from_image(veil);
        sprite.color = ink.with_alpha(0.0);
        sprite.custom_size = Some(Vec2::new(
            title_px * card.text.chars().count().max(6) as f32 * 0.9,
            title_px * 3.2,
        ));
        commands.spawn((
            OverlayLayer,
            TitleEntity,
            Part::Veil,
            sprite,
            Transform::from_xyz(0.0, 0.0, 0.0),
        ));
    }
    commands.spawn((
        OverlayLayer,
        TitleEntity,
        Part::Title,
        Text2d::new(card.text.to_uppercase()),
        font_of(title_px),
        TextColor(title_color.with_alpha(0.0)),
        Transform::from_xyz(0.0, 0.0, 2.0),
    ));
    if let Some(sub) = &card.sub {
        commands.spawn((
            OverlayLayer,
            TitleEntity,
            Part::Sub,
            Text2d::new(sub.to_uppercase()),
            font_of(sub_px),
            TextColor(sub_color.with_alpha(0.0)),
            Transform::from_xyz(0.0, title_px * 0.62 + sub_px, 2.0),
        ));
    }
    if card.kind == TitleKind::Level {
        let mut rule = Sprite::from_color(mint.with_alpha(0.0), Vec2::new((width * 0.6).min(384.0), 2.0));
        rule.color = mint.with_alpha(0.0);
        commands.spawn((
            OverlayLayer,
            TitleEntity,
            Part::Rule,
            rule,
            Transform::from_xyz(0.0, -title_px * 0.62, 2.0),
        ));
    }
}

/// Avance le carton et règle opacité et échelle de chaque élément ; le retire à la fin.
fn animate(
    time: Res<Time>,
    settings: Res<TitleSettings>,
    mut active: ResMut<ActiveTitle>,
    mut parts: Query<AnimatedPart, With<TitleEntity>>,
) {
    let (Some(card), Some(timing)) = (active.card.as_ref().map(|c| c.kind), active.timing) else {
        return;
    };
    // Le premier tick d'un carton neuf n'avance pas : il se montre à `t = 0`, jamais déjà entamé.
    let first = active.elapsed_s == 0.0;
    let t = if first {
        0.0
    } else {
        active.elapsed_s + time.delta_secs()
    };
    let look = look_at(card, &timing, t, settings.reduce_motion);
    for (part, mut transform, text, sprite) in &mut parts {
        let (alpha, scale) = match part {
            Part::Veil => (look.alpha * VEIL_ALPHA, Vec3::splat(1.0)),
            Part::Rule => (look.alpha, Vec3::new(look.rule.max(1e-3), 1.0, 1.0)),
            Part::Title | Part::Sub => (look.alpha, Vec3::splat(look.scale)),
        };
        if transform.scale != scale {
            transform.scale = scale;
        }
        if let Some(mut text) = text {
            text.0 = text.0.with_alpha(alpha);
        }
        if let Some(mut sprite) = sprite {
            sprite.color = sprite.color.with_alpha(alpha);
        }
    }
    if t >= timing.total_s() {
        active.card = None;
        active.timing = None;
        active.elapsed_s = 0.0;
    } else if !first {
        active.elapsed_s = t;
    } else {
        // Marque le carton comme commencé sans le faire passer pour neuf à l'image suivante.
        active.elapsed_s = f32::EPSILON;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::show::SheetSlot;

    fn sheet(names: &[&str]) -> ShowSheet {
        ShowSheet {
            event_name: String::new(),
            dj_sign: true,
            slots: names
                .iter()
                .map(|n| SheetSlot {
                    dj: (*n).to_owned(),
                    schedule: None,
                    cues: Vec::new(),
                })
                .collect(),
        }
    }

    #[test]
    fn the_dj_name_comes_from_the_sheet_with_a_numbered_fallback() {
        let s = sheet(&["  Aife ", ""]);
        assert_eq!(dj_name(Some(&s), 0), "Aife");
        assert_eq!(dj_name(Some(&s), 1), "DJ 2");
        assert_eq!(dj_name(Some(&s), 9), "DJ 10");
        assert_eq!(dj_name(None, 0), "DJ 1");
    }

    #[test]
    fn the_veil_is_opaque_at_the_centre_and_clear_at_the_edge() {
        let image = radial_veil();
        let data = image.data.as_ref().expect("pixels");
        let size = VEIL_SIZE as usize;
        let alpha = |x: usize, y: usize| data[(y * size + x) * 4 + 3];
        assert!(alpha(size / 2, size / 2) > 230);
        assert!(alpha(0, 0) < 5 && alpha(size - 1, size / 2) < 30);
    }
}
