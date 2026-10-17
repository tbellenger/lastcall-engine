use lastcall_core::geom::rasterize;
use lastcall_core::imageutil::{GrayImage, RgbaImage};
use lastcall_core::model::*;
use lastcall_core::render::merge_priority;
use lastcall_core::{
    build_pak, run_walk, ActorPart, Engine, HostEvent, PackageParts, RoomPart, WalkHost,
};
use std::collections::BTreeMap;

struct MemHost {
    slots: BTreeMap<String, Vec<u8>>,
}

impl WalkHost for MemHost {
    fn snapshot(&mut self, _name: &str, _rgba: &[u8], _w: u32, _h: u32) -> Result<(), String> {
        Ok(())
    }
    fn save_slot(&mut self, name: &str, data: &[u8]) -> Result<(), String> {
        self.slots.insert(name.to_string(), data.to_vec());
        Ok(())
    }
    fn load_slot(&mut self, name: &str) -> Result<Vec<u8>, String> {
        self.slots
            .get(name)
            .cloned()
            .ok_or_else(|| format!("missing slot {name}"))
    }
}

fn hero_atlas() -> RgbaImage {
    let mut img = RgbaImage::solid(32, 24, [0, 0, 0, 0]);
    for frame in 0..2 {
        for y in 0..24 {
            for x in 0..16 {
                let i = ((y * 32 + frame * 16 + x) * 4) as usize;
                img.rgba[i] = 40;
                img.rgba[i + 1] = 90;
                img.rgba[i + 2] = 220;
                img.rgba[i + 3] = 255;
            }
        }
    }
    img
}

fn anim(x: u32) -> Animation {
    Animation {
        fps: 8,
        looped: true,
        frames: vec![FrameDef {
            x,
            y: 0,
            w: 16,
            h: 24,
            ax: 8,
            ay: 23,
        }],
    }
}

fn parts(format_version: &str) -> PackageParts {
    let mut paint = vec![0u8; (SCREEN_W * SCREEN_H) as usize];
    for y in 0..185 {
        for x in 20..80 {
            paint[(y * SCREEN_W as i32 + x) as usize] = 12;
        }
    }
    let bands = vec![
        Band { y: 0, priority: 1 },
        Band {
            y: 150,
            priority: 4,
        },
        Band {
            y: 185,
            priority: 14,
        },
    ];
    let priority = merge_priority(SCREEN_W as i32, SCREEN_H as i32, &bands, Some(&paint));
    let walk = rasterize(
        SCREEN_W as i32,
        SCREEN_H as i32,
        &[Polygon {
            points: vec![[10, 150], [370, 150], [370, 211], [10, 211]],
            holes: vec![],
        }],
    );
    let mut costumes = BTreeMap::new();
    let mut animations = BTreeMap::new();
    for dir in ["n", "e", "s", "w"] {
        animations.insert(format!("stand_{dir}"), anim(0));
        animations.insert(format!("walk_{dir}"), anim(16));
    }
    costumes.insert("default".into(), Costume { animations });
    PackageParts {
        game: GameDef {
            id: "tiny".into(),
            title: "Tiny".into(),
            version: "0.1.0".into(),
            format_version: format_version.into(),
            engine_min: "0.1.0".into(),
            resolution: [SCREEN_W, SCREEN_H],
            start_room: "porch".into(),
            start_spawn: "default".into(),
            hero: "hero".into(),
            score_max: 5,
            rng_seed: 7,
            reach: 12,
            intro: vec![],
            rules: vec![],
        },
        vocab: Vocab {
            ignore: vec!["the".into(), "a".into()],
            prepositions: vec!["at".into(), "on".into(), "with".into()],
            phrases: vec![Phrase {
                from: "pick up".into(),
                to: "get".into(),
            }],
            verbs: vec![
                VerbDef {
                    id: "get".into(),
                    words: vec!["get".into()],
                    ranged: false,
                    meta: false,
                    default_reply: Some("You can't take that.".into()),
                },
                VerbDef {
                    id: "look".into(),
                    words: vec!["look".into()],
                    ranged: true,
                    meta: false,
                    default_reply: Some("You see nothing special.".into()),
                },
                VerbDef {
                    id: "climb".into(),
                    words: vec!["climb".into()],
                    ranged: false,
                    meta: false,
                    default_reply: Some("You can't climb that.".into()),
                },
                VerbDef {
                    id: "score".into(),
                    words: vec!["score".into()],
                    ranged: true,
                    meta: true,
                    default_reply: None,
                },
                VerbDef {
                    id: "restore".into(),
                    words: vec!["restore".into()],
                    ranged: true,
                    meta: true,
                    default_reply: None,
                },
            ],
            nouns: vec![
                NounDef {
                    id: "key".into(),
                    words: vec!["key".into()],
                },
                NounDef {
                    id: "well".into(),
                    words: vec!["well".into()],
                },
            ],
            rooms: BTreeMap::from([(
                "porch".into(),
                BTreeMap::from([("rim".into(), "well".into())]),
            )]),
            unknown_word: "I don't know the word '{word}'.".into(),
            ..Vocab::default()
        },
        items: vec![ItemDef {
            id: "key".into(),
            name: "brass key".into(),
            plural: "brass keys".into(),
            look: "A small brass key.".into(),
        }],
        rooms: vec![RoomPart {
            def: RoomDef {
                id: "porch".into(),
                name: "Night Porch".into(),
                look: "A stone porch.".into(),
                picture: "picture.png".into(),
                spawns: BTreeMap::from([("default".into(), [40, 190])]),
                exits: vec![],
                hotspots: vec![
                    HotspotDef {
                        id: "key".into(),
                        rect: [290, 160, 20, 20],
                        nouns: vec!["key".into()],
                        look: "The key glints.".into(),
                        stand: Some([300, 190]),
                        face: Some(Dir::N),
                    },
                    HotspotDef {
                        id: "well".into(),
                        rect: [180, 150, 30, 20],
                        nouns: vec!["well".into()],
                        look: "A slick stone well.".into(),
                        stand: Some([200, 170]),
                        face: Some(Dir::N),
                    },
                ],
                actors: vec![],
                props: vec![],
                on_enter: vec![Rule {
                    id: "enter_porch".into(),
                    verb: None,
                    noun: None,
                    second: None,
                    req: vec!["!flag:seen".into()],
                    once: false,
                    effects: vec![
                        Effect::Set {
                            flag: "seen".into(),
                        },
                        Effect::Say {
                            text: "Night settles on the porch.".into(),
                        },
                    ],
                }],
                rules: vec![
                    Rule {
                        id: "take_key".into(),
                        verb: Some("get".into()),
                        noun: Some("key".into()),
                        second: None,
                        req: vec!["!has:key".into()],
                        once: true,
                        effects: vec![
                            Effect::MarkOnce {
                                id: "take_key".into(),
                            },
                            Effect::Take { id: "key".into() },
                            Effect::Score {
                                id: "found_key".into(),
                                points: 5,
                            },
                            Effect::Say {
                                text: "You pocket the brass key.".into(),
                            },
                        ],
                    },
                    Rule {
                        id: "climb_well".into(),
                        verb: Some("climb".into()),
                        noun: Some("well".into()),
                        second: None,
                        req: vec![],
                        once: false,
                        effects: vec![
                            Effect::SetDead,
                            Effect::Say {
                                text: "You slip on the wet stones.".into(),
                            },
                        ],
                    },
                ],
                bands,
            },
            picture: RgbaImage::solid(SCREEN_W, SCREEN_H, [180, 30, 40, 255]),
            walk: GrayImage {
                w: SCREEN_W,
                h: SCREEN_H,
                pixels: walk,
            },
            priority: GrayImage {
                w: SCREEN_W,
                h: SCREEN_H,
                pixels: priority,
            },
            props: None,
        }],
        actors: vec![ActorPart {
            def: ActorDef {
                id: "hero".into(),
                costumes,
            },
            atlas: hero_atlas(),
        }],
    }
}

#[test]
fn plays_scores_dies_and_restores() {
    let bytes = build_pak(&parts("0.1.0")).unwrap();
    let mut engine = Engine::load(&bytes).unwrap();
    let mut host = MemHost {
        slots: BTreeMap::new(),
    };
    let script = r#"
@expect room porch
@expect text Night settles
@dismiss
look at the rim
@expect text slick stone
@dismiss
get flurb
@expect text don't know the word 'flurb'
@dismiss
@walkto hotspot:key
get key
@expect text pocket
@dismiss
@expect item key
@expect score 5
@expect flag seen
@save slot
@walkto hotspot:well
climb well
@expect text slip
@expect death
@restore
@expect alive
@expect room porch
@expect score 0
@expect !item key
@restore slot
@expect item key
@expect score 5
"#;
    run_walk(&mut engine, script, &mut host).unwrap();
}

#[test]
fn occludes_sprite_behind_priority_paint() {
    let bytes = build_pak(&parts("0.1.0")).unwrap();
    let mut engine = Engine::load(&bytes).unwrap();
    let mut host = MemHost {
        slots: BTreeMap::new(),
    };
    engine.dismiss_narration().unwrap();
    run_walk(&mut engine, "@walkto 50,160\n", &mut host).unwrap();
    assert_eq!(engine.hero_px(), (50, 160));
    // Foot priority is the low band, so the painted column hides the hero.
    assert_eq!(pixel(&engine, 50, 150), [180, 30, 40, 255]);
    run_walk(&mut engine, "@walkto 50,190\n", &mut host).unwrap();
    // In front of the paint, the hero's blue shirt shows.
    assert_eq!(pixel(&engine, 50, 180)[2], 220);
}

#[test]
fn rejects_unknown_major_and_warns_on_newer_minor() {
    let bytes = build_pak(&parts("1.2.0")).unwrap();
    match Engine::load(&bytes) {
        Err(err) => assert!(err.to_string().contains("major"), "{err}"),
        Ok(_) => panic!("major version 1 should be rejected"),
    }

    let bytes = build_pak(&parts("0.9.0")).unwrap();
    let mut engine = Engine::load(&bytes).unwrap();
    let events = engine.drain_events();
    assert!(events
        .iter()
        .any(|event| matches!(event, HostEvent::Warning { .. })));
}

fn pixel(engine: &Engine, x: i32, y: i32) -> [u8; 4] {
    let i = ((y as u32 * SCREEN_W + x as u32) * 4) as usize;
    let fb = engine.framebuffer();
    [fb[i], fb[i + 1], fb[i + 2], fb[i + 3]]
}
