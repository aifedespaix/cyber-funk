//! Les cartons-titres dans une vraie `App` Bevy, sans rendu : un titre de drop, le DJ de la passation et le niveau qui
//! monte posent chacun un carton ; un nouveau remplace l'ancien ; il disparaît de lui-même.

#![allow(clippy::expect_used)]

use std::time::Duration;

use bevy::prelude::*;
use bevy::time::TimeUpdateStrategy;
use cf_render::show::{SheetSlot, ShowSheet};
use cf_render::{ActiveTitle, CorePlugin, ShowEventMessage, TitleKind, TitlesPlugin};
use cf_show::director::ShowEvent;

const FRAME: Duration = Duration::from_millis(50);

fn app() -> App {
    let mut app = App::new();
    app.add_plugins((MinimalPlugins, CorePlugin, TitlesPlugin))
        .add_message::<ShowEventMessage>()
        .insert_resource(TimeUpdateStrategy::ManualDuration(FRAME))
        .insert_resource(ShowSheet {
            event_name: String::new(),
            dj_sign: true,
            slots: vec![
                SheetSlot {
                    dj: "Aife".to_owned(),
                    schedule: None,
                    cues: Vec::new(),
                },
                SheetSlot {
                    dj: "Nova".to_owned(),
                    schedule: None,
                    cues: Vec::new(),
                },
            ],
        });
    app
}

fn card(app: &App) -> Option<(TitleKind, String, Option<String>)> {
    app.world()
        .resource::<ActiveTitle>()
        .card()
        .map(|c| (c.kind, c.text.clone(), c.sub.clone()))
}

fn drive(app: &mut App, frames: u32) {
    for _ in 0..frames {
        app.update();
    }
}

#[test]
fn un_titre_de_drop_pose_un_carton_puis_s_efface() {
    let mut app = app();
    drive(&mut app, 2);
    assert!(card(&app).is_none());
    app.world_mut()
        .write_message(ShowEventMessage(ShowEvent::Title("DROP".into())));
    drive(&mut app, 2);
    assert_eq!(card(&app), Some((TitleKind::Drop, "DROP".to_owned(), None)));
    assert!(
        app.world_mut().query::<&Text2d>().iter(app.world()).count() >= 1,
        "le texte est à l'écran"
    );
    // 1,5 s de plateau + 0,25 s d'entrée + 0,5 s de sortie à 120 BPM par défaut : 4 s suffisent.
    drive(&mut app, 80);
    assert!(card(&app).is_none(), "le carton s'est effacé");
    assert_eq!(
        app.world_mut().query::<&Text2d>().iter(app.world()).count(),
        0,
        "plus aucun texte"
    );
}

#[test]
fn la_passation_annonce_le_dj_qui_arrive() {
    let mut app = app();
    drive(&mut app, 2);
    app.world_mut().write_message(ShowEventMessage(ShowEvent::Handover {
        from: 0,
        to: 1,
        fade_s: 4.0,
    }));
    drive(&mut app, 2);
    assert_eq!(
        card(&app),
        Some((TitleKind::Dj, "Nova".to_owned(), Some("Aux platines".to_owned())))
    );
}

#[test]
fn un_nouveau_carton_remplace_l_ancien() {
    let mut app = app();
    drive(&mut app, 2);
    app.world_mut()
        .write_message(ShowEventMessage(ShowEvent::Title("UN".into())));
    drive(&mut app, 4);
    app.world_mut()
        .write_message(ShowEventMessage(ShowEvent::Title("DEUX".into())));
    drive(&mut app, 2);
    assert_eq!(card(&app).map(|c| c.1), Some("DEUX".to_owned()));
    let titles = app.world_mut().query::<&Text2d>().iter(app.world()).count();
    assert_eq!(titles, 1, "l'ancien est retiré");
}

#[test]
fn le_niveau_qui_monte_pose_un_carton_mais_pas_le_niveau_zero() {
    use std::path::Path;

    use cf_render::CurrentShow;
    use cf_show::director::ShowDirector;
    use cf_show::drop::DropTitles;
    use cf_show::load::{LoadedShow, LookLibrary};
    use rand_pcg::Pcg32;

    let assets = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../assets");
    let looks = LookLibrary::load_dir(&assets.join("looks")).expect("looks");
    let show = LoadedShow::load(&assets.join("shows/demo.ron"), &looks).expect("show");
    let director = ShowDirector::<Pcg32>::new(&show, DropTitles::empty(), 1).expect("chef");
    let mut state = CurrentShow(director.state().clone());
    state.0.level = 0;

    let mut app = app();
    app.insert_resource(state);
    drive(&mut app, 3);
    assert!(card(&app).is_none(), "l'ouverture n'annonce rien");
    app.world_mut().resource_mut::<CurrentShow>().0.level = 2;
    drive(&mut app, 2);
    assert_eq!(
        card(&app),
        Some((TitleKind::Level, "La ville".to_owned(), Some("Niveau 2".to_owned())))
    );
    // Retomber au niveau 0 (reprise, retour en pré-intro) n'annonce rien.
    drive(&mut app, 200);
    app.world_mut().resource_mut::<CurrentShow>().0.level = 0;
    drive(&mut app, 2);
    assert!(card(&app).is_none());
}
