use crate::error::{Error, Result};
use crate::model::{Dir, SCREEN_H, SCREEN_W};
use crate::pak::Package;
use crate::rng::Pcg32;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

pub const SAVE_FORMAT: u32 = 1;
pub const SAVE_LIMIT: usize = 32 * 1024;

#[derive(Debug, Clone)]
pub struct World {
    pub room: String,
    pub hero_x: i32,
    pub hero_y: i32,
    pub facing: Dir,
    pub costume: String,
    pub inventory: BTreeMap<String, i32>,
    pub flags: BTreeSet<String>,
    pub vars: BTreeMap<String, i32>,
    pub timers: BTreeMap<String, i32>,
    pub scored: BTreeSet<String>,
    pub fired: BTreeSet<String>,
    pub actors: BTreeMap<String, String>,
    pub dead: bool,
    pub ending: Option<String>,
    pub tick: u64,
    /// Named checkpoint a death should restore, or none for the autosave.
    pub death_restore: Option<String>,
    pub held: bool,
    pub picture: String,
    pub cutscene: Option<String>,
    pub cut_beat: u32,
    pub cut_frame: u32,
    pub cut_flip: u32,
}

impl World {
    pub fn new(seed_tick: u64) -> Self {
        Self {
            room: String::new(),
            hero_x: 0,
            hero_y: 0,
            facing: Dir::S,
            costume: "default".into(),
            inventory: BTreeMap::new(),
            flags: BTreeSet::new(),
            vars: BTreeMap::new(),
            timers: BTreeMap::new(),
            scored: BTreeSet::new(),
            fired: BTreeSet::new(),
            actors: BTreeMap::new(),
            dead: false,
            ending: None,
            tick: seed_tick,
            death_restore: None,
            held: false,
            picture: String::new(),
            cutscene: None,
            cut_beat: 0,
            cut_frame: 0,
            cut_flip: 0,
        }
    }
}

#[derive(Debug, Clone)]
pub struct Checkpoint {
    pub world: World,
    pub rng_state: u64,
    pub rng_inc: u64,
}

#[derive(Debug)]
pub struct DecodedSave {
    pub world: World,
    pub rng: Pcg32,
    pub checkpoints: BTreeMap<String, Checkpoint>,
    pub stack: Vec<String>,
    pub pack_mismatch: bool,
    pub autosave_locked: bool,
}

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct SaveFile {
    format: u32,
    game: String,
    pack_version: String,
    pack_hash: String,
    tick: u64,
    rng_seed: u64,
    rng_inc: u64,
    state: SaveState,
}

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct SaveState {
    room: String,
    hero: HeroSave,
    inventory: BTreeMap<String, i32>,
    flags: BTreeSet<String>,
    vars: BTreeMap<String, i32>,
    timers: BTreeMap<String, i32>,
    scored: BTreeSet<String>,
    fired: BTreeSet<String>,
    actors: BTreeMap<String, String>,
    checkpoints: BTreeMap<String, CheckpointSave>,
    checkpoint_stack: Vec<String>,
    dead: bool,
    ending: Option<String>,
    /// When set, room changes no longer replace the autosave checkpoint.
    #[serde(default)]
    autosave_locked: bool,
    #[serde(default)]
    death_restore: Option<String>,
    #[serde(default)]
    held: bool,
    #[serde(default)]
    picture: String,
    #[serde(default)]
    cutscene: Option<String>,
    #[serde(default)]
    cut_beat: u32,
    #[serde(default)]
    cut_frame: u32,
    #[serde(default)]
    cut_flip: u32,
}

#[derive(Serialize, Deserialize)]
struct HeroSave {
    pos: [i32; 2],
    facing: Dir,
    costume: String,
}

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct CheckpointSave {
    tick: u64,
    rng_seed: u64,
    rng_inc: u64,
    state: InnerState,
}

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct InnerState {
    room: String,
    hero: HeroSave,
    inventory: BTreeMap<String, i32>,
    flags: BTreeSet<String>,
    vars: BTreeMap<String, i32>,
    timers: BTreeMap<String, i32>,
    scored: BTreeSet<String>,
    fired: BTreeSet<String>,
    actors: BTreeMap<String, String>,
    dead: bool,
    ending: Option<String>,
    #[serde(default)]
    death_restore: Option<String>,
    #[serde(default)]
    held: bool,
    #[serde(default)]
    picture: String,
    #[serde(default)]
    cutscene: Option<String>,
    #[serde(default)]
    cut_beat: u32,
    #[serde(default)]
    cut_frame: u32,
    #[serde(default)]
    cut_flip: u32,
}

pub fn encode_save(
    package: &Package,
    world: &World,
    rng: Pcg32,
    checkpoints: &BTreeMap<String, Checkpoint>,
    stack: &[String],
    autosave_locked: bool,
) -> Result<Vec<u8>> {
    let file = SaveFile {
        format: SAVE_FORMAT,
        game: package.game.id.clone(),
        pack_version: package.pack_version.clone(),
        pack_hash: package.content_hash.clone(),
        tick: world.tick,
        rng_seed: rng.state(),
        rng_inc: rng.inc(),
        state: SaveState {
            room: world.room.clone(),
            hero: hero_save(world),
            inventory: world.inventory.clone(),
            flags: world.flags.clone(),
            vars: world.vars.clone(),
            timers: world.timers.clone(),
            scored: world.scored.clone(),
            fired: world.fired.clone(),
            actors: world.actors.clone(),
            checkpoints: checkpoints
                .iter()
                .map(|(name, cp)| {
                    (
                        name.clone(),
                        CheckpointSave {
                            tick: cp.world.tick,
                            rng_seed: cp.rng_state,
                            rng_inc: cp.rng_inc,
                            state: inner_from(&cp.world),
                        },
                    )
                })
                .collect(),
            checkpoint_stack: stack.to_vec(),
            dead: world.dead,
            ending: world.ending.clone(),
            death_restore: world.death_restore.clone(),
            held: world.held,
            picture: world.picture.clone(),
            cutscene: world.cutscene.clone(),
            cut_beat: world.cut_beat,
            cut_frame: world.cut_frame,
            cut_flip: world.cut_flip,
            autosave_locked,
        },
    };
    let bytes = serde_json::to_vec(&file)?;
    if bytes.len() > SAVE_LIMIT {
        return Err(Error::msg(format!(
            "save is {} bytes; the limit is {SAVE_LIMIT}",
            bytes.len()
        )));
    }
    Ok(bytes)
}

pub fn decode_save(package: &Package, bytes: &[u8]) -> Result<DecodedSave> {
    if bytes.len() > SAVE_LIMIT {
        return Err(Error::msg(format!(
            "save is {} bytes; the limit is {SAVE_LIMIT}",
            bytes.len()
        )));
    }
    let file: SaveFile = serde_json::from_slice(bytes)?;
    if file.format != SAVE_FORMAT {
        return Err(Error::msg(format!(
            "unsupported save format {}",
            file.format
        )));
    }
    if file.game != package.game.id {
        return Err(Error::msg(format!(
            "save is for game '{}', this package is '{}'",
            file.game, package.game.id
        )));
    }
    let world = world_from_state(package, &file.state, file.tick)?;
    let mut checkpoints = BTreeMap::new();
    for (name, cp) in &file.state.checkpoints {
        if !is_checkpoint_name(name) {
            return Err(Error::msg(format!("bad checkpoint name '{name}'")));
        }
        let mut cp_world = world_from_inner(package, &cp.state, cp.tick)?;
        cp_world.tick = cp.tick;
        checkpoints.insert(
            name.clone(),
            Checkpoint {
                world: cp_world,
                rng_state: cp.rng_seed,
                rng_inc: cp.rng_inc,
            },
        );
    }
    for name in &file.state.checkpoint_stack {
        if !checkpoints.contains_key(name) {
            return Err(Error::msg(format!(
                "checkpoint stack names missing '{name}'"
            )));
        }
    }
    Ok(DecodedSave {
        world,
        rng: Pcg32::from_parts(file.rng_seed, file.rng_inc),
        checkpoints,
        stack: file.state.checkpoint_stack,
        pack_mismatch: file.pack_hash != package.content_hash,
        autosave_locked: file.state.autosave_locked,
    })
}

fn world_from_state(package: &Package, state: &SaveState, tick: u64) -> Result<World> {
    let inner = InnerState {
        room: state.room.clone(),
        hero: HeroSave {
            pos: state.hero.pos,
            facing: state.hero.facing,
            costume: state.hero.costume.clone(),
        },
        inventory: state.inventory.clone(),
        flags: state.flags.clone(),
        vars: state.vars.clone(),
        timers: state.timers.clone(),
        scored: state.scored.clone(),
        fired: state.fired.clone(),
        actors: state.actors.clone(),
        dead: state.dead,
        ending: state.ending.clone(),
        death_restore: state.death_restore.clone(),
        held: state.held,
        picture: state.picture.clone(),
        cutscene: state.cutscene.clone(),
        cut_beat: state.cut_beat,
        cut_frame: state.cut_frame,
        cut_flip: state.cut_flip,
    };
    world_from_inner(package, &inner, tick)
}

fn world_from_inner(package: &Package, state: &InnerState, tick: u64) -> Result<World> {
    if !package.rooms.contains_key(&state.room) {
        return Err(Error::msg(format!(
            "save room '{}' is not in this game",
            state.room
        )));
    }
    let [x, y] = state.hero.pos;
    if !(0..SCREEN_W as i32 * 256).contains(&x) || !(0..SCREEN_H as i32 * 256).contains(&y) {
        return Err(Error::msg("save hero position is off the screen"));
    }
    let hero = package
        .actors
        .get(&package.game.hero)
        .ok_or_else(|| Error::msg("package has no hero"))?;
    if !hero.def.costumes.contains_key(&state.hero.costume) {
        return Err(Error::msg(format!(
            "save costume '{}' is not on the hero",
            state.hero.costume
        )));
    }
    for (id, count) in &state.inventory {
        if !package.item_index.contains_key(id) {
            return Err(Error::msg(format!("save item '{id}' is not in this game")));
        }
        if !(0..=99).contains(count) {
            return Err(Error::msg(format!("save item '{id}' has a bad count")));
        }
    }
    for flag in &state.flags {
        if !package.known_flags.contains(flag) {
            return Err(Error::msg(format!(
                "save flag '{flag}' is not in this game"
            )));
        }
    }
    for var in state.vars.keys() {
        if !package.known_vars.contains(var) {
            return Err(Error::msg(format!("save var '{var}' is not in this game")));
        }
    }
    for (var, value) in &state.vars {
        if !(-1_000_000..=1_000_000).contains(value) {
            return Err(Error::msg(format!("save var '{var}' is out of range")));
        }
    }
    if !state.timers.is_empty() {
        return Err(Error::msg("this package does not define timers"));
    }
    for id in &state.scored {
        if !package.score_table.contains_key(id) {
            return Err(Error::msg(format!(
                "save score id '{id}' is not in this game"
            )));
        }
    }
    for id in &state.fired {
        if !package.known_rules.contains(id) {
            return Err(Error::msg(format!(
                "save rule id '{id}' is not in this game"
            )));
        }
    }
    let places = known_places(package);
    for id in state.actors.keys() {
        if !places.contains(id) {
            return Err(Error::msg(format!("save actor '{id}' is not in this game")));
        }
    }
    if let Some(ending) = &state.ending {
        if !package.endings.contains(ending) {
            return Err(Error::msg(format!(
                "save ending '{ending}' is not in this game"
            )));
        }
    }
    Ok(World {
        room: state.room.clone(),
        hero_x: x,
        hero_y: y,
        facing: state.hero.facing,
        costume: state.hero.costume.clone(),
        inventory: state.inventory.clone(),
        flags: state.flags.clone(),
        vars: state.vars.clone(),
        timers: state.timers.clone(),
        scored: state.scored.clone(),
        fired: state.fired.clone(),
        actors: state.actors.clone(),
        dead: state.dead,
        ending: state.ending.clone(),
        tick,
        death_restore: state.death_restore.clone(),
        held: state.held,
        picture: state.picture.clone(),
        cutscene: state.cutscene.clone(),
        cut_beat: state.cut_beat,
        cut_frame: state.cut_frame,
        cut_flip: state.cut_flip,
    })
}

/// Checkpoint names come from the game. Charlie uses `alley-chase` and `club-chase`.
fn is_checkpoint_name(text: &str) -> bool {
    let mut chars = text.chars();
    match chars.next() {
        Some(c) if c.is_ascii_alphabetic() || c == '_' => {}
        _ => return false,
    }
    chars.all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-') && text.len() <= 64
}

fn known_places(package: &Package) -> BTreeSet<String> {
    let mut ids = BTreeSet::new();
    ids.insert(package.game.hero.clone());
    for room in package.rooms.values() {
        for actor in &room.def.actors {
            ids.insert(actor.id.clone());
        }
    }
    ids
}

fn hero_save(world: &World) -> HeroSave {
    HeroSave {
        pos: [world.hero_x, world.hero_y],
        facing: world.facing,
        costume: world.costume.clone(),
    }
}

fn inner_from(world: &World) -> InnerState {
    InnerState {
        room: world.room.clone(),
        hero: hero_save(world),
        inventory: world.inventory.clone(),
        flags: world.flags.clone(),
        vars: world.vars.clone(),
        timers: world.timers.clone(),
        scored: world.scored.clone(),
        fired: world.fired.clone(),
        actors: world.actors.clone(),
        dead: world.dead,
        ending: world.ending.clone(),
        death_restore: world.death_restore.clone(),
        held: world.held,
        picture: world.picture.clone(),
        cutscene: world.cutscene.clone(),
        cut_beat: world.cut_beat,
        cut_frame: world.cut_frame,
        cut_flip: world.cut_flip,
    }
}
