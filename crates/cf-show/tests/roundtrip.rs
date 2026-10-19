//! Propriété : tout événement (et tout look) écrit en RON puis relu est identique à l'original.
//!
//! Les valeurs générées ne sont pas forcément valides au sens de `validate` (plages) : l'aller-retour doit tenir pour
//! toute valeur représentable, y compris les textes avec guillemets, accents ou retours à la ligne.

#![allow(
    clippy::unwrap_used,
    clippy::panic,
    reason = "fichier de test : les aides hors #[test] peuvent échouer bruyamment"
)]

use cf_show::load::{event_to_ron, look_to_ron, parse_event, parse_look};
use cf_show::model::*;
use proptest::collection::vec;
use proptest::option;
use proptest::prelude::*;

fn slug() -> impl Strategy<Value = String> {
    "[a-z0-9]{1,8}(-[a-z0-9]{1,4}){0,2}"
}

fn id<T: std::fmt::Debug>(make: fn(String) -> Result<T, InvalidValue>) -> impl Strategy<Value = T> {
    slug().prop_filter_map("slug valide", move |s| make(s).ok())
}

fn num() -> impl Strategy<Value = f32> {
    any::<f32>().prop_filter("nombre fini", |v| v.is_finite())
}

fn text() -> impl Strategy<Value = String> {
    any::<String>()
}

fn hex() -> impl Strategy<Value = HexColor> {
    "#[0-9a-fA-F]{6}".prop_filter_map("couleur", |s| HexColor::new(s).ok())
}

fn clock() -> impl Strategy<Value = ClockTime> {
    (0u8..24, 0u8..60).prop_filter_map("heure", |(h, m)| ClockTime::from_hm(h, m))
}

fn custom_theme() -> impl Strategy<Value = CustomTheme> {
    (hex(), hex(), hex(), hex(), hex()).prop_map(|(ink, velvet, cyan, ember, ice)| CustomTheme {
        ink,
        velvet,
        cyan,
        ember,
        ice,
    })
}

fn preset() -> impl Strategy<Value = PresetTheme> {
    prop_oneof![
        Just(PresetTheme::Velvet),
        Just(PresetTheme::Synthwave),
        Just(PresetTheme::Terminal),
        Just(PresetTheme::Aurora),
        Just(PresetTheme::Gold),
        Just(PresetTheme::Frost),
    ]
}

fn theme() -> impl Strategy<Value = Theme> {
    prop_oneof![preset().prop_map(Theme::Preset), custom_theme().prop_map(Theme::Custom)]
}

fn palette() -> impl Strategy<Value = PaletteRef> {
    prop_oneof![
        Just(PaletteRef::Inherit),
        Just(PaletteRef::Dj),
        preset().prop_map(PaletteRef::Preset),
        custom_theme().prop_map(PaletteRef::Custom),
    ]
}

fn layers() -> impl Strategy<Value = LayerMix> {
    (num(), num(), num(), num()).prop_map(|(background, world, foreground, overlay)| LayerMix {
        background,
        world,
        foreground,
        overlay,
    })
}

fn fx() -> impl Strategy<Value = FxGates> {
    (num(), num(), num()).prop_map(|(rgb_split, flash, city_pulse)| FxGates {
        rgb_split,
        flash,
        city_pulse,
    })
}

fn cut_grid() -> impl Strategy<Value = CutGrid> {
    (any::<u8>(), any::<u8>(), any::<u8>()).prop_map(|(calm, groove, peak)| CutGrid { calm, groove, peak })
}

fn gates() -> impl Strategy<Value = ShowGates> {
    let lists = (
        vec(id(StuntTier::try_from), 0..3),
        vec(id(StuntKind::try_from), 0..4),
        vec(id(ShotTier::try_from), 0..3),
        vec(id(ShotId::try_from), 0..4),
        vec(id(EventName::try_from), 0..4),
    );
    let flags = (
        any::<bool>(),
        any::<bool>(),
        any::<bool>(),
        any::<bool>(),
        any::<bool>(),
    );
    let numbers = (any::<u8>(), num(), num(), num(), num(), num(), num());
    (lists, flags, numbers, cut_grid(), fx()).prop_map(
        |(
            (stunt_tiers, stunt_kinds, shot_tiers, shot_block, events),
            (flight_on_drop_only, tricks, salvo, meteor_shower, ascension),
            (max_floor, flight_rate, stunt_gap_scale, event_rate, meteor_rate, cruise, ground_traffic),
            cut_grid,
            fx,
        )| ShowGates {
            max_floor,
            flight_rate,
            flight_on_drop_only,
            tricks,
            stunt_tiers,
            stunt_kinds,
            stunt_gap_scale,
            shot_tiers,
            shot_block,
            cut_grid,
            salvo,
            events,
            event_rate,
            meteor_rate,
            meteor_shower,
            cruise,
            ground_traffic,
            fx,
            ascension,
        },
    )
}

fn param() -> impl Strategy<Value = ParamPath> {
    proptest::sample::select(ParamPath::ALL.to_vec())
}

fn macros() -> impl Strategy<Value = Vec<Macro>> {
    let drive = (param(), num(), num()).prop_map(|(param, at_zero, at_one)| MacroDrive { param, at_zero, at_one });
    vec(
        (text(), vec(drive, 0..3)).prop_map(|(name, drives)| Macro { name, drives }),
        0..3,
    )
}

fn look() -> impl Strategy<Value = Look> {
    (
        any::<u32>(),
        id(LookId::try_from),
        text(),
        id(WorldId::try_from),
        layers(),
        palette(),
        num(),
        gates(),
        macros(),
    )
        .prop_map(
            |(version, id, name, world, layers, palette, hue_shift_deg, gates, macros)| Look {
                version,
                id,
                name,
                world,
                layers,
                palette,
                hue_shift_deg,
                gates,
                macros,
            },
        )
}

fn gate_overrides() -> impl Strategy<Value = GateOverrides> {
    let flags = (
        option::of(any::<u8>()),
        option::of(num()),
        option::of(any::<bool>()),
        option::of(any::<bool>()),
    );
    let lists = (
        option::of(vec(id(StuntTier::try_from), 0..2)),
        option::of(vec(id(StuntKind::try_from), 0..2)),
        option::of(num()),
        option::of(vec(id(ShotTier::try_from), 0..2)),
        option::of(vec(id(ShotId::try_from), 0..2)),
        option::of(cut_grid()),
    );
    let traffic = (
        option::of(any::<bool>()),
        option::of(vec(id(EventName::try_from), 0..2)),
        option::of(num()),
    );
    let world = (
        option::of(num()),
        option::of(any::<bool>()),
        option::of(num()),
        option::of(num()),
    );
    let fx = (
        option::of(num()),
        option::of(num()),
        option::of(num()),
        option::of(any::<bool>()),
    );
    (flags, lists, traffic, world, fx).prop_map(
        |(
            (max_floor, flight_rate, flight_on_drop_only, tricks),
            (stunt_tiers, stunt_kinds, stunt_gap_scale, shot_tiers, shot_block, cut_grid),
            (salvo, events, event_rate),
            (meteor_rate, meteor_shower, cruise, ground_traffic),
            (rgb_split, flash, city_pulse, ascension),
        )| GateOverrides {
            max_floor,
            flight_rate,
            flight_on_drop_only,
            tricks,
            stunt_tiers,
            stunt_kinds,
            stunt_gap_scale,
            shot_tiers,
            shot_block,
            cut_grid,
            salvo,
            events,
            event_rate,
            meteor_rate,
            meteor_shower,
            cruise,
            ground_traffic,
            fx: FxOverrides {
                rgb_split,
                flash,
                city_pulse,
            },
            ascension,
        },
    )
}

fn overrides() -> impl Strategy<Value = LookOverrides> {
    (
        option::of(layers()),
        option::of(palette()),
        option::of(num()),
        gate_overrides(),
    )
        .prop_map(|(layers, palette, hue_shift_deg, gates)| LookOverrides {
            layers,
            palette,
            hue_shift_deg,
            gates,
        })
}

fn cue() -> impl Strategy<Value = Cue> {
    let entry = prop_oneof![
        Just(CueEntry::Cut),
        any::<u8>().prop_map(|bars| CueEntry::Fade { bars }),
        Just(CueEntry::OnDrop),
        Just(CueEntry::OnPhrase),
    ];
    let advance = prop_oneof![
        Just(CueAdvance::Manual),
        any::<u16>().prop_map(|bars| CueAdvance::Bars { bars }),
        Just(CueAdvance::OnDrop),
        clock().prop_map(|time| CueAdvance::At { time }),
        any::<f64>()
            .prop_filter("fini", |s| s.is_finite())
            .prop_map(|seconds| CueAdvance::After { seconds }),
    ];
    let trigger = prop_oneof![
        text().prop_map(Trigger::Title),
        Just(Trigger::ShowDj),
        Just(Trigger::DjLogo),
        Just(Trigger::Ascension),
        id(EventName::try_from).prop_map(Trigger::Event),
        Just(Trigger::ForceDrop),
    ];
    (
        id(CueId::try_from),
        text(),
        id(LookId::try_from),
        overrides(),
        entry,
        vec(trigger, 0..3),
        advance,
    )
        .prop_map(|(id, name, look, overrides, entry, triggers, advance)| Cue {
            id,
            name,
            look,
            overrides,
            entry,
            triggers,
            advance,
        })
}

fn tempo() -> impl Strategy<Value = TempoSource> {
    prop_oneof![
        Just(TempoSource::Auto),
        Just(TempoSource::Audio),
        Just(TempoSource::MidiClock),
        Just(TempoSource::Link),
        Just(TempoSource::Tap),
    ]
}

fn slot() -> impl Strategy<Value = DjSlot> {
    let logo = "[a-z]{1,6}(/[a-z0-9]{1,6}){0,2}\\.png".prop_filter_map("chemin", |s| AssetPath::new(s).ok());
    let schedule = (clock(), clock()).prop_map(|(start, end)| Schedule { start, end });
    let dj = (text(), option::of(logo), option::of(theme()), option::of(schedule)).prop_map(
        |(name, logo, theme, schedule)| DjIdentity {
            name,
            logo,
            theme,
            schedule,
        },
    );
    let audio = (num(), num(), num(), tempo()).prop_map(|(bpm_min, bpm_max, sensitivity, tempo_source)| AudioProfile {
        bpm_min,
        bpm_max,
        sensitivity,
        tempo_source,
    });
    let mode = prop_oneof![Just(SlotMode::Auto), Just(SlotMode::Semi), Just(SlotMode::Manual)];
    (
        id(SlotId::try_from),
        dj,
        audio,
        mode,
        vec(id(ControlProfileId::try_from), 0..2),
        vec(cue(), 0..3),
    )
        .prop_map(|(id, dj, audio, mode, controls, cues)| DjSlot {
            id,
            dj,
            audio,
            mode,
            controls,
            cues,
        })
}

fn settings() -> impl Strategy<Value = EventSettings> {
    let flashes = prop_oneof![Just(FlashPolicy::Limited), Just(FlashPolicy::Off)];
    let projector = prop_oneof![
        Just(MonitorSelect::Auto),
        any::<u8>().prop_map(MonitorSelect::Index),
        text().prop_map(MonitorSelect::Name)
    ];
    let flags = (any::<bool>(), any::<bool>());
    (
        text(),
        theme(),
        flashes,
        num(),
        projector,
        id(LookId::try_from),
        flags,
        vec(id(ControlProfileId::try_from), 0..2),
    )
        .prop_map(
            |(name, theme, flashes, latency_offset_ms, projector, safe_look, (dj_sign, dj_on_ads), controls)| {
                EventSettings {
                    name,
                    theme,
                    flashes,
                    latency_offset_ms,
                    projector,
                    safe_look,
                    dj_sign,
                    dj_on_ads,
                    controls,
                }
            },
        )
}

fn event() -> impl Strategy<Value = Event> {
    (any::<u32>(), settings(), vec(look(), 0..2), vec(slot(), 0..3)).prop_map(|(version, settings, looks, slots)| {
        Event {
            version,
            settings,
            looks,
            slots,
        }
    })
}

/// Relit le texte sans passer par la vérification de version (le `version` généré est quelconque).
fn reparse<T: serde::de::DeserializeOwned>(text: &str) -> T {
    ron::Options::default()
        .with_default_extension(ron::extensions::Extensions::IMPLICIT_SOME)
        .from_str(text)
        .unwrap_or_else(|e| panic!("relecture impossible : {e}\n{text}"))
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(256))]

    #[test]
    fn event_roundtrip(event in event()) {
        let text = event_to_ron(&event).unwrap();
        prop_assert_eq!(&reparse::<Event>(&text), &event);
    }

    #[test]
    fn look_roundtrip(look in look()) {
        let text = look_to_ron(&look).unwrap();
        prop_assert_eq!(&reparse::<Look>(&text), &look);
    }

    /// Avec la version courante, le chemin public complet (lecture de l'en-tête de version comprise) relit pareil.
    #[test]
    fn current_version_roundtrip(mut event in event(), mut look in look()) {
        event.version = FORMAT_VERSION;
        look.version = FORMAT_VERSION;
        prop_assert_eq!(&parse_event(&event_to_ron(&event).unwrap(), "test").unwrap(), &event);
        prop_assert_eq!(&parse_look(&look_to_ron(&look).unwrap(), "test").unwrap(), &look);
    }
}
