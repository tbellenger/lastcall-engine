//! Compile an authoring folder into a package the core can load.

use lastcall_core::cond;
use lastcall_core::geom::{self, flood, in_mask};
use lastcall_core::imageutil::{self, RgbaImage};
use lastcall_core::model::{
    ActorDef, Animation, Costume, Dir, Effect, FrameDef, GameDef, ItemDef, PriorityDef, RoomDef,
    Rule, Vocab, WalkDef, SCREEN_H, SCREEN_W,
};
use lastcall_core::pak::{self, sha256_hex, ActorPart, PackageParts, RoomPart};
use lastcall_core::parser::VocabIndex;
use lastcall_core::render;
use lastcall_core::walk::{run_walk, WalkHost};
use lastcall_core::{Engine, Error, Result};
use serde::Deserialize;
use serde_json::Value;
use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Path, PathBuf};

mod studio;
mod tree;
pub use studio::Session;
pub use tree::{DiskTree, MemTree, ReadTree};

use tree::{base_name, join_rel};

pub fn validate_dir(dir: &Path) -> Result<PackageParts> {
    let parts = load_dir(dir)?;
    let bytes = pak::build_pak(&parts)?;
    let _loaded = pak::load_pak(&bytes)?;
    Ok(parts)
}

pub fn pack_dir(dir: &Path, output: &Path) -> Result<()> {
    let parts = validate_dir(dir)?;
    let bytes = pak::build_pak(&parts)?;
    if let Some(parent) = output.parent() {
        if !parent.as_os_str().is_empty() {
            fs::create_dir_all(parent)?;
        }
    }
    fs::write(output, bytes)?;
    Ok(())
}

pub fn test_dir(dir: &Path, update_snapshots: bool) -> Result<()> {
    let parts = validate_dir(dir)?;
    let bytes = pak::build_pak(&parts)?;
    let boot = Engine::load(&bytes)?;
    check_boot(dir, &boot.frame_hash(), update_snapshots)?;
    let walks = walk_files(dir)?;
    if walks.is_empty() {
        return Err(Error::msg(format!("no .walk scripts in {}", dir.display())));
    }
    let mut failures = Vec::new();
    for walk in walks {
        let script = fs::read_to_string(&walk)?;
        let mut engine = Engine::load(&bytes)?;
        let mut host = SnapshotHost {
            root: dir.join("tests").join("snapshots"),
            update: update_snapshots,
        };
        if let Err(err) = run_walk(&mut engine, &script, &mut host) {
            failures.push(format!("{}: {err}", walk.display()));
        }
    }
    let parity = dir.join("tests").join("parity");
    if parity.is_dir() {
        if let Err(err) = run_parity(&bytes, &parity, dir, update_snapshots) {
            failures.push(err.to_string());
        }
    }
    if let Err(err) = check_pixel_look(&bytes, dir) {
        failures.push(err.to_string());
    }
    if failures.is_empty() {
        Ok(())
    } else {
        Err(Error::msg(failures.join("\n")))
    }
}

fn check_pixel_look(bytes: &[u8], dir: &Path) -> Result<()> {
    let pixel = dir.join("tests").join("pixel");
    if !pixel.is_dir() {
        return Ok(());
    }
    let mut failures = Vec::new();
    let mut names = Vec::new();
    for entry in fs::read_dir(&pixel)? {
        let path = entry?.path();
        if path.extension().and_then(|ext| ext.to_str()) == Some("png") {
            if let Some(name) = path.file_stem().and_then(|stem| stem.to_str()) {
                names.push((name.to_string(), path));
            }
        }
    }
    names.sort();
    let mut engine = Engine::load(bytes).map_err(|err| Error::msg(err.to_string()))?;
    for (room, path) in names {
        if let Err(err) = engine.preview_room(&room) {
            failures.push(format!("{room}: {err}"));
            continue;
        }
        let fb = engine.framebuffer();
        if let Err(err) = write_engine_shot(&room, fb) {
            failures.push(format!("{room} shot: {err}"));
        }
        let previous = fs::read(&path)?;
        let decoded = imageutil::decode_png(&previous)
            .map_err(|err| Error::msg(err.to_string()))?
            .into_rgba();
        if decoded.w != SCREEN_W || decoded.h != SCREEN_H {
            failures.push(format!("{room}: reference is not {SCREEN_W}x{SCREEN_H}"));
            continue;
        }
        let top = render::BAR_H;
        let bottom = SCREEN_H as i32 - render::BAR_H;
        let mut off = 0usize;
        let mut total = 0usize;
        for y in top..bottom {
            for x in 0..SCREEN_W as i32 {
                let i = ((y * SCREEN_W as i32 + x) * 4) as usize;
                let d = (fb[i] as i16 - decoded.rgba[i] as i16).unsigned_abs() as u16
                    + (fb[i + 1] as i16 - decoded.rgba[i + 1] as i16).unsigned_abs() as u16
                    + (fb[i + 2] as i16 - decoded.rgba[i + 2] as i16).unsigned_abs() as u16;
                if d > 24 {
                    off += 1;
                }
                total += 1;
            }
        }
        if total > 0 && off * 100 / total > 2 {
            let actual = pixel.join("_actual");
            let _ = fs::create_dir_all(&actual);
            let image = RgbaImage {
                w: SCREEN_W,
                h: SCREEN_H,
                rgba: fb.to_vec(),
            };
            if let Ok(encoded) = imageutil::encode_rgba(&image) {
                let _ = fs::write(actual.join(format!("{room}.png")), encoded);
            }
            failures.push(format!(
                "{room}: {off} of {total} playfield pixels differ from the pixel reference"
            ));
        }
    }
    if failures.is_empty() {
        Ok(())
    } else {
        Err(Error::msg(failures.join("\n")))
    }
}

fn write_engine_shot(room: &str, rgba: &[u8]) -> std::result::Result<(), String> {
    let dir = PathBuf::from("/tmp/charlie-shots");
    fs::create_dir_all(&dir).map_err(|err| err.to_string())?;
    let image = RgbaImage {
        w: SCREEN_W,
        h: SCREEN_H,
        rgba: rgba.to_vec(),
    };
    let encoded = imageutil::encode_rgba(&image).map_err(|err| err.to_string())?;
    fs::write(dir.join(format!("{room}.png")), encoded).map_err(|err| err.to_string())
}

fn run_parity(bytes: &[u8], dir: &Path, game_dir: &Path, update: bool) -> Result<()> {
    let mut files = Vec::new();
    for entry in fs::read_dir(dir)? {
        let path = entry?.path();
        if path.extension().and_then(|ext| ext.to_str()) == Some("json") {
            files.push(path);
        }
    }
    files.sort();
    if files.is_empty() {
        return Err(Error::msg(format!(
            "no parity transcripts in {}",
            dir.display()
        )));
    }
    let mut failures = Vec::new();
    for path in files {
        let text = fs::read_to_string(&path)?;
        let transcript: ParityFile = serde_json::from_str(&text)
            .map_err(|err| Error::msg(format!("{}: {err}", path.display())))?;
        let mut engine = Engine::load(bytes).map_err(|err| Error::msg(err.to_string()))?;
        if let Err(err) = drain(&mut engine, 80) {
            failures.push(format!("{} boot: {err}", path.display()));
            continue;
        }
        for (index, step) in transcript.steps.iter().enumerate() {
            let from = engine.said().len();
            if let Some(line) = &step.interrupt {
                engine.interrupt_line(line);
            } else if let Some(line) = &step.cmd {
                engine.submit_line(line);
            } else if let Some(target) = &step.walkto {
                let (x, y) = if let Some(id) = target.strip_prefix("exit:") {
                    engine.exit_target(id).ok_or_else(|| {
                        Error::msg(format!(
                            "{} step {}: no target for exit {id}",
                            path.display(),
                            index + 1
                        ))
                    })?
                } else {
                    let (x, y) = target.split_once(',').ok_or_else(|| {
                        Error::msg(format!(
                            "{} step {}: walkto expects x,y or exit:id",
                            path.display(),
                            index + 1
                        ))
                    })?;
                    (
                        x.trim()
                            .parse::<i32>()
                            .map_err(|err| Error::msg(err.to_string()))?,
                        y.trim()
                            .parse::<i32>()
                            .map_err(|err| Error::msg(err.to_string()))?,
                    )
                };
                engine.script_walk_to(x, y).map_err(|err| {
                    Error::msg(format!("{} step {}: {err}", path.display(), index + 1))
                })?;
                finish_motion(&mut engine);
            } else if let Some([x, y]) = step.tap {
                engine.input(lastcall_core::Input::Tap { x, y });
                finish_motion(&mut engine);
            } else if let Some(spec) = &step.dir {
                let mut parts = spec.split_whitespace();
                let name = parts.next().unwrap_or("");
                let ticks: u32 = parts.next().unwrap_or("0").parse().unwrap_or(0);
                let dir = lastcall_core::model::Dir::from_name(name).ok_or_else(|| {
                    Error::msg(format!(
                        "{} step {}: unknown direction {name}",
                        path.display(),
                        index + 1
                    ))
                })?;
                engine.force_walk(dir, ticks);
                engine.step(ticks);
            }
            let limit = step
                .dismiss
                .unwrap_or(if step.hold.unwrap_or(false) { 0 } else { 80 });
            if step.hold.unwrap_or(false) && step.dismiss.is_none() {
                finish_motion(&mut engine);
            } else if let Err(err) = dismiss_some(&mut engine, limit) {
                failures.push(format!("{} step {}: {err}", path.display(), index + 1));
                break;
            }
            let said = engine.said();
            let lines = said[from.min(said.len())..].to_vec();
            if let Err(err) = check_step(&engine, step, &lines) {
                failures.push(format!(
                    "{} step {} ({}): {err}",
                    path.display(),
                    index + 1,
                    step.cmd
                        .as_deref()
                        .or(step.interrupt.as_deref())
                        .unwrap_or("")
                ));
                break;
            }
            if let Some(name) = &step.shot {
                if let Err(err) = write_shot(game_dir, name, engine.framebuffer(), update) {
                    failures.push(format!("{} shot {name}: {err}", path.display()));
                    break;
                }
            }
        }
    }
    if failures.is_empty() {
        Ok(())
    } else {
        Err(Error::msg(failures.join("\n")))
    }
}

fn finish_motion(engine: &mut Engine) {
    for _ in 0..4_000 {
        if !engine.route_active() {
            break;
        }
        engine.step(1);
    }
}

fn dismiss_some(engine: &mut Engine, limit: u32) -> std::result::Result<(), String> {
    let mut dismissed = 0u32;
    for _ in 0..4_000 {
        finish_motion(engine);
        if !engine.presenting() {
            return Ok(());
        }
        if dismissed >= limit {
            return Ok(());
        }
        match engine.dismiss_narration() {
            Ok(()) => dismissed += 1,
            Err(_) => engine.step(1),
        }
    }
    Err("narration did not settle".into())
}

fn drain(engine: &mut Engine, limit: u32) -> std::result::Result<(), String> {
    dismiss_some(engine, limit)
}

fn check_step(
    engine: &Engine,
    step: &ParityStep,
    lines: &[String],
) -> std::result::Result<(), String> {
    if lines != step.lines.as_slice() {
        let mut msg = format!(
            "lines\n  expected:\n{}\n  actual:\n{}",
            step.lines.join("\n"),
            lines.join("\n")
        );
        if msg.len() > 1_200 {
            msg.truncate(1_200);
            msg.push_str("\n...");
        }
        return Err(msg);
    }
    if engine.score() != step.score {
        return Err(format!("score {} != {}", engine.score(), step.score));
    }
    if engine.room_id() != step.room {
        return Err(format!("room {} != {}", engine.room_id(), step.room));
    }
    if engine.is_dead() != step.dead {
        return Err(format!("dead {} != {}", engine.is_dead(), step.dead));
    }
    if engine.ending() != step.ending.as_deref() {
        return Err(format!("ending {:?} != {:?}", engine.ending(), step.ending));
    }
    let mut actual: BTreeSet<String> = engine
        .flags()
        .into_iter()
        .filter(|flag| !matches!(flag.as_str(), "live" | "vicEntered" | "quietEnter"))
        .collect();
    for flag in &step.flags {
        if flag == "live"
            || flag == "vicEntered"
            || flag == "quietEnter"
            || flag.starts_with("did:")
            || flag.starts_with("did_")
        {
            continue;
        }
        if !actual.remove(flag) {
            return Err(format!("missing flag {flag}"));
        }
    }
    actual.retain(|flag| !flag.starts_with("did_") && !flag.starts_with("did:"));
    if !actual.is_empty() {
        return Err(format!("extra flags {actual:?}"));
    }
    let mut items: BTreeMap<String, i32> = engine.inventory().into_iter().collect();
    for (id, count) in &step.items {
        let have = items.remove(id).unwrap_or(0);
        if have != *count {
            return Err(format!("item {id} count {have} != {count}"));
        }
    }
    if !items.is_empty() {
        return Err(format!("extra items {items:?}"));
    }
    for (name, value) in &step.vars {
        if engine.var(name) != *value {
            return Err(format!("var {name} {} != {value}", engine.var(name)));
        }
    }
    Ok(())
}

fn write_shot(
    game_dir: &Path,
    name: &str,
    rgba: &[u8],
    update: bool,
) -> std::result::Result<(), String> {
    if name.is_empty() || name.contains('/') || name.contains("..") {
        return Err(format!("bad shot name {name}"));
    }
    let image = RgbaImage {
        w: SCREEN_W,
        h: SCREEN_H,
        rgba: rgba.to_vec(),
    };
    let encoded = imageutil::encode_rgba(&image).map_err(|err| err.to_string())?;
    let path = game_dir
        .join("tests")
        .join("shots")
        .join(format!("{name}.png"));
    if update || !path.exists() {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).map_err(|err| err.to_string())?;
        }
        fs::write(&path, &encoded).map_err(|err| err.to_string())?;
        return Ok(());
    }
    let previous = fs::read(&path).map_err(|err| err.to_string())?;
    let decoded = imageutil::decode_png(&previous)
        .map_err(|err| err.to_string())?
        .into_rgba();
    if decoded.w != SCREEN_W || decoded.h != SCREEN_H || decoded.rgba.len() != rgba.len() {
        return Err("shot size changed".into());
    }
    let mut off = 0usize;
    for i in 0..rgba.len() / 4 {
        let d = (rgba[i * 4] as i16 - decoded.rgba[i * 4] as i16).unsigned_abs() as u16
            + (rgba[i * 4 + 1] as i16 - decoded.rgba[i * 4 + 1] as i16).unsigned_abs() as u16
            + (rgba[i * 4 + 2] as i16 - decoded.rgba[i * 4 + 2] as i16).unsigned_abs() as u16;
        if d > 24 {
            off += 1;
        }
    }
    let pixels = (SCREEN_W * SCREEN_H) as usize;
    if off * 100 / pixels > 2 {
        let actual = game_dir.join("tests").join("shots").join("_actual");
        fs::create_dir_all(&actual).map_err(|err| err.to_string())?;
        fs::write(actual.join(format!("{name}.png")), encoded).map_err(|err| err.to_string())?;
        return Err(format!("{off} pixels differ"));
    }
    Ok(())
}

#[derive(Debug, Deserialize)]
struct ParityFile {
    steps: Vec<ParityStep>,
}

#[derive(Debug, Deserialize)]
struct ParityStep {
    #[serde(default)]
    cmd: Option<String>,
    #[serde(default)]
    interrupt: Option<String>,
    #[serde(default)]
    hold: Option<bool>,
    #[serde(default)]
    dismiss: Option<u32>,
    lines: Vec<String>,
    score: i32,
    room: String,
    flags: Vec<String>,
    items: BTreeMap<String, i32>,
    vars: BTreeMap<String, i32>,
    #[serde(default)]
    dead: bool,
    #[serde(default)]
    ending: Option<String>,
    #[serde(default)]
    shot: Option<String>,
    /// Walk to `x,y` or `exit:<id>` before checking this step.
    #[serde(default)]
    walkto: Option<String>,
    /// Click the picture at view pixels.
    #[serde(default)]
    tap: Option<[i32; 2]>,
    /// Hold an arrow. `n 40` walks north for 40 ticks.
    #[serde(default)]
    dir: Option<String>,
}

fn load_dir(dir: &Path) -> Result<PackageParts> {
    load_tree(&DiskTree::new(dir))
}

pub fn load_tree(tree: &dyn ReadTree) -> Result<PackageParts> {
    let game_schema = compiled_schema("game.schema.json")?;
    let vocab_schema = compiled_schema("vocab.schema.json")?;
    let items_schema = compiled_schema("items.schema.json")?;
    let room_schema = compiled_schema("room.schema.json")?;
    let walk_schema = compiled_schema("walk.schema.json")?;
    let priority_schema = compiled_schema("priority.schema.json")?;
    let actor_schema = compiled_schema("actor.schema.json")?;

    let game_value = read_json(tree, "game.json")?;
    check_schema("game.json", &game_schema, &game_value)?;
    let source_game: SourceGame = serde_json::from_value(game_value)
        .map_err(|err| Error::msg(format!("game.json: {err}")))?;
    if source_game.resolution != [SCREEN_W, SCREEN_H] {
        return Err(Error::msg(format!(
            "resolution must be {SCREEN_W}x{SCREEN_H}"
        )));
    }
    let (major, minor, _) = pak::parse_semver(&source_game.format_version)?;
    if major != 0 {
        return Err(Error::msg(format!(
            "unsupported format major {major}; this tool writes 0.x packages"
        )));
    }
    if minor > lastcall_core::model::FORMAT_MINOR {
        eprintln!(
            "advc: warning: format {} is newer than this tool",
            source_game.format_version
        );
    }

    let vocab_value = read_json(tree, "vocab.json")?;
    check_schema("vocab.json", &vocab_schema, &vocab_value)?;
    let vocab: Vocab = serde_json::from_value(vocab_value)
        .map_err(|err| Error::msg(format!("vocab.json: {err}")))?;
    VocabIndex::build(&vocab).map_err(Error::msg)?;

    let items_value = read_json(tree, "items.json")?;
    check_schema("items.json", &items_schema, &items_value)?;
    let items: Vec<ItemDef> = serde_json::from_value(items_value)
        .map_err(|err| Error::msg(format!("items.json: {err}")))?;

    let mut actors = Vec::new();
    if tree.is_dir("actors") {
        let folders = list_dirs(tree, "actors")?;
        for folder in folders {
            let path = join_rel(&folder, "actor.json");
            if !tree.is_file(&path) {
                continue;
            }
            let value = read_json(tree, &path)?;
            check_schema(&path, &actor_schema, &value)?;
            let source: SourceActor = serde_json::from_value(value)
                .map_err(|err| Error::msg(format!("{path}: {err}")))?;
            let folder_name = base_name(&folder);
            if source.id != folder_name {
                return Err(Error::msg(format!(
                    "actor folder {folder_name} does not match id {}",
                    source.id
                )));
            }
            actors.push(compile_actor(tree, &folder, source)?);
        }
    }

    let mut rooms = Vec::new();
    let mut room_sources = Vec::new();
    let folders = list_dirs(tree, "rooms")?;
    for folder in folders {
        let path = join_rel(&folder, "room.json");
        if !tree.is_file(&path) {
            continue;
        }
        let value = read_json(tree, &path)?;
        check_schema(&path, &room_schema, &value)?;
        let source: SourceRoom =
            serde_json::from_value(value).map_err(|err| Error::msg(format!("{path}: {err}")))?;
        let folder_name = base_name(&folder);
        if source.id != folder_name {
            return Err(Error::msg(format!(
                "room folder {folder_name} does not match id {}",
                source.id
            )));
        }
        let walk_path = join_rel(&folder, "walk.json");
        let walk_value = read_json(tree, &walk_path)?;
        check_schema(&walk_path, &walk_schema, &walk_value)?;
        let walk: WalkDef = serde_json::from_value(walk_value)
            .map_err(|err| Error::msg(format!("{walk_path}: {err}")))?;
        let pri_path = join_rel(&folder, "priority.json");
        let pri_value = read_json(tree, &pri_path)?;
        check_schema(&pri_path, &priority_schema, &pri_value)?;
        let priority: PriorityDef = serde_json::from_value(pri_value)
            .map_err(|err| Error::msg(format!("{pri_path}: {err}")))?;
        room_sources.push((folder, source, walk, priority));
    }

    let mut game_rules = Vec::new();
    for (index, rule) in source_game.rules.iter().enumerate() {
        game_rules.push(compile_rule(rule, &format!("game:{index}"))?);
    }
    let game = GameDef {
        id: source_game.id,
        title: source_game.title,
        version: source_game.version,
        format_version: source_game.format_version,
        engine_min: source_game.engine_min,
        resolution: source_game.resolution,
        start_room: source_game.start_room,
        start_spawn: source_game.start_spawn,
        hero: source_game.hero,
        score_max: source_game.score_max,
        rng_seed: source_game.rng_seed,
        reach: source_game.reach,
        intro: source_game.intro,
        rules: game_rules,
        hooks: source_game.hooks,
        timers: source_game.timers,
        declarations: source_game.declarations,
        carrying: source_game.carrying,
        death_prompt: source_game.death_prompt,
    };

    for (folder, source, walk, priority) in room_sources {
        rooms.push(compile_room(tree, &folder, source, walk, priority)?);
    }

    let script = if tree.is_file("scripts/game.rhai") {
        let bytes = tree.read("scripts/game.rhai")?;
        String::from_utf8(bytes).map_err(|err| Error::msg(format!("scripts/game.rhai: {err}")))?
    } else {
        String::new()
    };
    let cutscenes = load_cutscenes(tree)?;
    let parts = PackageParts {
        game,
        vocab,
        items,
        rooms,
        actors,
        script,
        cutscenes,
    };
    check_refs(&parts)?;
    Ok(parts)
}

fn compile_room(
    tree: &dyn ReadTree,
    folder: &str,
    source: SourceRoom,
    walk_def: WalkDef,
    mut priority: PriorityDef,
) -> Result<RoomPart> {
    let picture = load_rgba(tree, &join_rel(folder, "picture.png"))?;
    if picture.w != SCREEN_W || picture.h != SCREEN_H {
        return Err(Error::msg(format!(
            "room {} picture is {}x{}, expected {SCREEN_W}x{SCREEN_H}",
            source.id, picture.w, picture.h
        )));
    }
    priority.bands.sort_by_key(|band| band.y);
    let paint_path = join_rel(folder, "priority.png");
    let paint = if tree.is_file(&paint_path) {
        let image = load_gray(tree, &paint_path)?;
        if image.w != SCREEN_W || image.h != SCREEN_H {
            return Err(Error::msg(format!(
                "room {} priority paint is not {SCREEN_W}x{SCREEN_H}",
                source.id
            )));
        }
        Some(image.pixels)
    } else {
        None
    };
    let merged = render::merge_priority(
        SCREEN_W as i32,
        SCREEN_H as i32,
        &priority.bands,
        paint.as_deref(),
    );
    let mask = geom::rasterize(SCREEN_W as i32, SCREEN_H as i32, &walk_def.polygons);
    let props = if source.props.is_empty() {
        None
    } else {
        let path = join_rel(folder, "props.png");
        if !tree.is_file(&path) {
            return Err(Error::msg(format!(
                "room {} has props but no props.png",
                source.id
            )));
        }
        let atlas = load_rgba(tree, &path)?;
        for prop in &source.props {
            imageutil::crop(
                &atlas,
                prop.frame.x,
                prop.frame.y,
                prop.frame.w,
                prop.frame.h,
                prop.frame.ax,
                prop.frame.ay,
            )
            .map_err(|err| Error::msg(format!("room {} prop {}: {err}", source.id, prop.id)))?;
        }
        Some(atlas)
    };

    let mut on_enter = Vec::new();
    for (index, rule) in source.on_enter.iter().enumerate() {
        on_enter.push(compile_rule(rule, &format!("{}:enter:{index}", source.id))?);
    }
    let mut rules = Vec::new();
    for (index, rule) in source.rules.iter().enumerate() {
        rules.push(compile_rule(rule, &format!("{}:{index}", source.id))?);
    }
    check_mask_points(&source.id, &mask, &source, &on_enter, &rules)?;
    let variant_images = load_variants(tree, folder, &source.id, &source.pictures)?;
    let occluders = load_occluders(tree, folder, &source.id, &source.actors)?;

    Ok(RoomPart {
        def: RoomDef {
            id: source.id,
            name: source.name,
            look: source.look,
            picture: String::new(),
            spawns: source.spawns,
            exits: source.exits,
            hotspots: source.hotspots,
            actors: source.actors,
            props: source.props,
            on_enter,
            rules,
            bands: priority.bands,
            pictures: source.pictures.clone(),
        },
        picture,
        walk: lastcall_core::imageutil::GrayImage {
            w: SCREEN_W,
            h: SCREEN_H,
            pixels: mask,
        },
        priority: lastcall_core::imageutil::GrayImage {
            w: SCREEN_W,
            h: SCREEN_H,
            pixels: merged,
        },
        props,
        pictures: variant_images,
        occluders,
    })
}

fn load_occluders(
    tree: &dyn ReadTree,
    folder: &str,
    room_id: &str,
    actors: &[lastcall_core::model::ActorPlace],
) -> Result<BTreeMap<String, lastcall_core::imageutil::GrayImage>> {
    let mut occluders = BTreeMap::new();
    for place in actors {
        let Some(name) = &place.occlude else {
            continue;
        };
        if occluders.contains_key(name) {
            continue;
        }
        let path = join_rel(folder, &format!("occlude-{name}.png"));
        let image = load_gray(tree, &path)
            .map_err(|err| Error::msg(format!("room {room_id} occluder {name}: {err}")))?;
        if image.w != SCREEN_W || image.h != SCREEN_H {
            return Err(Error::msg(format!(
                "room {room_id} occluder {name} is not {SCREEN_W}x{SCREEN_H}"
            )));
        }
        occluders.insert(name.clone(), image);
    }
    Ok(occluders)
}

fn extra_effects(parts: &PackageParts) -> Vec<Effect> {
    let mut effects = Vec::new();
    for timer in &parts.game.timers {
        effects.extend(timer.effects.clone());
    }
    for cut in &parts.cutscenes {
        effects.extend(cut.def.on_end.clone());
        for beat in &cut.def.beats {
            effects.extend(beat.effects.clone());
        }
        for rule in &cut.def.on_command {
            effects.extend(rule.effects.clone());
        }
    }
    effects
}

fn load_variants(
    tree: &dyn ReadTree,
    folder: &str,
    room_id: &str,
    names: &[String],
) -> Result<BTreeMap<String, lastcall_core::imageutil::RgbaImage>> {
    let mut pictures = BTreeMap::new();
    for name in names {
        let path = join_rel(folder, &format!("variant-{name}.png"));
        let image = load_rgba(tree, &path)
            .map_err(|err| Error::msg(format!("room {room_id} variant {name}: {err}")))?;
        if image.w != SCREEN_W || image.h != SCREEN_H {
            return Err(Error::msg(format!(
                "room {room_id} variant {name} is not {SCREEN_W}x{SCREEN_H}"
            )));
        }
        pictures.insert(name.clone(), image);
    }
    Ok(pictures)
}

fn load_cutscenes(tree: &dyn ReadTree) -> Result<Vec<lastcall_core::pak::CutscenePart>> {
    if !tree.is_dir("cutscenes") {
        return Ok(Vec::new());
    }
    let mut names = Vec::new();
    for name in tree.list("cutscenes")? {
        if name.ends_with(".json") {
            names.push(join_rel("cutscenes", &name));
        }
    }
    names.sort();
    let mut cutscenes = Vec::new();
    for path in names {
        let value = read_json(tree, &path)?;
        let mut def: lastcall_core::model::CutsceneDef =
            serde_json::from_value(value).map_err(|err| Error::msg(format!("{path}: {err}")))?;
        let folder = join_rel("cutscenes", &def.id);
        let mut frames = Vec::new();
        if tree.is_dir(&folder) {
            let mut pngs = Vec::new();
            for name in tree.list(&folder)? {
                if name.ends_with(".png") {
                    pngs.push(join_rel(&folder, &name));
                }
            }
            pngs.sort();
            for png in pngs {
                let image = load_rgba(tree, &png)?;
                if image.w != SCREEN_W || image.h != SCREEN_H {
                    return Err(Error::msg(format!(
                        "cutscene {} frame {png} is not {SCREEN_W}x{SCREEN_H}",
                        def.id
                    )));
                }
                frames.push(image);
            }
        }
        def.frame_count = frames.len() as u32;
        cutscenes.push(lastcall_core::pak::CutscenePart { def, frames });
    }
    Ok(cutscenes)
}

fn check_mask_points(
    room: &str,
    mask: &[u8],
    source: &SourceRoom,
    on_enter: &[Rule],
    rules: &[Rule],
) -> Result<()> {
    let w = SCREEN_W as i32;
    let h = SCREEN_H as i32;
    if source.spawns.is_empty() {
        return Err(Error::msg(format!("room {room} has no spawns")));
    }
    for (name, spot) in &source.spawns {
        if !in_mask(mask, w, h, spot[0], spot[1]) {
            return Err(Error::msg(format!(
                "room {room} spawn {name} at {},{} is not on the walk mask",
                spot[0], spot[1]
            )));
        }
    }
    let mut reached: Vec<Vec<bool>> = Vec::new();
    for (name, spot) in &source.spawns {
        let seen = flood(mask, w, h, (spot[0], spot[1]));
        for (other, other_spot) in &source.spawns {
            if !seen[(other_spot[1] * w + other_spot[0]) as usize] {
                return Err(Error::msg(format!(
                    "room {room} spawn {other} is not reachable from {name}"
                )));
            }
        }
        reached.push(seen);
    }
    let require = |seen: &[Vec<bool>], x: i32, y: i32, what: &str| -> Result<()> {
        if !in_mask(mask, w, h, x, y) {
            return Err(Error::msg(format!(
                "room {room} {what} at {x},{y} is not on the walk mask"
            )));
        }
        for seen in seen {
            if !seen[(y * w + x) as usize] {
                return Err(Error::msg(format!(
                    "room {room} {what} at {x},{y} is not reachable from every spawn"
                )));
            }
        }
        Ok(())
    };
    for hotspot in &source.hotspots {
        if let Some(stand) = hotspot.stand {
            require(
                &reached,
                stand[0],
                stand[1],
                &format!("hotspot {} stand", hotspot.id),
            )?;
        }
    }
    for exit in &source.exits {
        if let Some(approach) = exit.approach {
            require(
                &reached,
                approach[0],
                approach[1],
                &format!("exit {} approach", exit.id),
            )?;
        }
        let mut ok = false;
        for seen in &reached {
            let mut local = false;
            for y in exit.trigger[1]..exit.trigger[1] + exit.trigger[3] {
                for x in exit.trigger[0]..exit.trigger[0] + exit.trigger[2] {
                    if in_mask(mask, w, h, x, y) && seen[(y * w + x) as usize] {
                        local = true;
                    }
                }
            }
            if !local {
                return Err(Error::msg(format!(
                    "room {room} exit {} is not reachable from every spawn",
                    exit.id
                )));
            }
            ok = true;
        }
        if !ok {
            return Err(Error::msg(format!(
                "room {room} exit {} has no walkable trigger",
                exit.id
            )));
        }
    }
    for rule in on_enter.iter().chain(rules.iter()) {
        for effect in &rule.effects {
            if let Effect::WalkTo { x, y } = effect {
                require(&reached, *x, *y, &format!("rule {} walkTo", rule.id))?;
            }
        }
    }
    Ok(())
}

fn compile_actor(tree: &dyn ReadTree, folder: &str, source: SourceActor) -> Result<ActorPart> {
    let mut costumes = BTreeMap::new();
    let mut cache: BTreeMap<String, FrameDef> = BTreeMap::new();
    let mut blits: Vec<Blit> = Vec::new();
    let mut x = 0u32;
    let mut y = 0u32;
    let mut row_h = 0u32;
    let mut used_w = 1u32;
    let mut used_h = 1u32;
    const MAX_W: u32 = 1024;

    for (costume_id, costume) in &source.costumes {
        let strips = costume_strips(costume);
        if strips.is_empty() || strips.iter().any(|strip| strip.sheet.is_empty()) {
            return Err(Error::msg(format!(
                "actor {} costume {costume_id} has no sheet",
                source.id
            )));
        }
        let mut animations = BTreeMap::new();
        for strip in &strips {
            let sheet = load_rgba(tree, &join_rel(folder, &strip.sheet))?;
            if strip.frame_w == 0
                || strip.frame_h == 0
                || sheet.h != strip.frame_h
                || sheet.w % strip.frame_w != 0
            {
                return Err(Error::msg(format!(
                    "actor {} costume {costume_id} sheet {} must be a horizontal strip of {}x{} frames",
                    source.id, strip.sheet, strip.frame_w, strip.frame_h
                )));
            }
            if strip.ax < 0
                || strip.ay < 0
                || strip.ax >= strip.frame_w as i32
                || strip.ay >= strip.frame_h as i32
            {
                return Err(Error::msg(format!(
                    "actor {} costume {costume_id} anchor is outside {}",
                    source.id, strip.sheet
                )));
            }
            let count = sheet.w / strip.frame_w;
            for (anim_id, anim) in &strip.animations {
                if animations.contains_key(anim_id) {
                    return Err(Error::msg(format!(
                        "actor {} costume {costume_id} animation {anim_id} is listed twice",
                        source.id
                    )));
                }
                let mut frames = Vec::new();
                for index in &anim.frames {
                    if *index >= count {
                        return Err(Error::msg(format!(
                            "actor {} animation {anim_id} frame {index} is outside the sheet",
                            source.id
                        )));
                    }
                    let key = format!(
                        "{costume_id}|{}|{index}|{}|{}|{}|{}",
                        strip.sheet, strip.frame_w, strip.frame_h, strip.ax, strip.ay
                    );
                    let frame = if let Some(existing) = cache.get(&key) {
                        existing.clone()
                    } else {
                        if x + strip.frame_w > MAX_W {
                            x = 0;
                            y += row_h;
                            row_h = 0;
                        }
                        let frame = FrameDef {
                            x,
                            y,
                            w: strip.frame_w,
                            h: strip.frame_h,
                            ax: strip.ax,
                            ay: strip.ay,
                        };
                        blits.push(Blit {
                            sheet: sheet.clone(),
                            sx: index * strip.frame_w,
                            dx: x,
                            dy: y,
                            w: strip.frame_w,
                            h: strip.frame_h,
                        });
                        x += strip.frame_w;
                        row_h = row_h.max(strip.frame_h);
                        used_w = used_w.max(x);
                        used_h = used_h.max(y + row_h);
                        cache.insert(key, frame.clone());
                        frame
                    };
                    frames.push(frame);
                }
                animations.insert(
                    anim_id.clone(),
                    Animation {
                        fps: anim.fps.max(1),
                        looped: anim.looped,
                        frames,
                    },
                );
            }
        }
        costumes.insert(costume_id.clone(), Costume { animations });
    }

    let mut rgba = vec![0u8; (used_w * used_h * 4) as usize];
    for blit in &blits {
        for row in 0..blit.h {
            let src = (((row) * blit.sheet.w + blit.sx) * 4) as usize;
            let dst = (((blit.dy + row) * used_w + blit.dx) * 4) as usize;
            let n = (blit.w * 4) as usize;
            rgba[dst..dst + n].copy_from_slice(&blit.sheet.rgba[src..src + n]);
        }
    }
    Ok(ActorPart {
        def: ActorDef {
            id: source.id,
            costumes,
        },
        atlas: RgbaImage {
            w: used_w,
            h: used_h,
            rgba,
        },
    })
}

struct Blit {
    sheet: RgbaImage,
    sx: u32,
    dx: u32,
    dy: u32,
    w: u32,
    h: u32,
}

fn check_refs(parts: &PackageParts) -> Result<()> {
    let verbs: BTreeSet<&str> = parts
        .vocab
        .verbs
        .iter()
        .map(|verb| verb.id.as_str())
        .collect();
    let nouns: BTreeSet<&str> = parts
        .vocab
        .nouns
        .iter()
        .map(|noun| noun.id.as_str())
        .collect();
    let items: BTreeSet<&str> = parts.items.iter().map(|item| item.id.as_str()).collect();
    let room_ids: BTreeSet<&str> = parts
        .rooms
        .iter()
        .map(|room| room.def.id.as_str())
        .collect();
    let actor_ids: BTreeSet<&str> = parts
        .actors
        .iter()
        .map(|actor| actor.def.id.as_str())
        .collect();

    if !room_ids.contains(parts.game.start_room.as_str()) {
        return Err(Error::msg(format!(
            "start room {} is missing",
            parts.game.start_room
        )));
    }
    let start = parts
        .rooms
        .iter()
        .find(|room| room.def.id == parts.game.start_room)
        .unwrap();
    if !start.def.spawns.contains_key(&parts.game.start_spawn) {
        return Err(Error::msg(format!(
            "start spawn {} is missing from {}",
            parts.game.start_spawn, parts.game.start_room
        )));
    }
    if !actor_ids.contains(parts.game.hero.as_str()) {
        return Err(Error::msg(format!(
            "hero actor {} is missing",
            parts.game.hero
        )));
    }
    let hero = parts
        .actors
        .iter()
        .find(|actor| actor.def.id == parts.game.hero)
        .unwrap();
    let costume = hero.def.costumes.get("default").ok_or_else(|| {
        Error::msg(format!(
            "hero {} needs a costume named default",
            parts.game.hero
        ))
    })?;
    if !costume.animations.contains_key("stand_s") {
        return Err(Error::msg("hero costume default needs a stand_s animation"));
    }

    let mut seen_rules = BTreeSet::new();
    let mut scores: BTreeMap<String, i32> = BTreeMap::new();
    let mut all_rules: Vec<&Rule> = parts.game.rules.iter().collect();
    for room in &parts.rooms {
        if room.def.spawns.is_empty() {
            return Err(Error::msg(format!("room {} has no spawns", room.def.id)));
        }
        for exit in &room.def.exits {
            if !room_ids.contains(exit.to.as_str()) {
                return Err(Error::msg(format!(
                    "room {} exit {} leads to unknown room {}",
                    room.def.id, exit.id, exit.to
                )));
            }
            let dest = parts
                .rooms
                .iter()
                .find(|other| other.def.id == exit.to)
                .unwrap();
            if !dest.def.spawns.contains_key(&exit.spawn) {
                return Err(Error::msg(format!(
                    "room {} exit {} uses missing spawn {} in {}",
                    room.def.id, exit.id, exit.spawn, exit.to
                )));
            }
            if let Some(cond) = &exit.blocked_if {
                cond::parse_cond(cond)?;
            }
        }
        for hotspot in &room.def.hotspots {
            for noun in &hotspot.nouns {
                if !nouns.contains(noun.as_str()) {
                    return Err(Error::msg(format!(
                        "room {} hotspot {} uses unknown noun {noun}",
                        room.def.id, hotspot.id
                    )));
                }
            }
        }
        for prop in &room.def.props {
            if let Some(cond) = &prop.show_if {
                cond::parse_cond(cond)?;
            }
        }
        for place in &room.def.actors {
            if !actor_ids.contains(place.actor.as_str()) {
                return Err(Error::msg(format!(
                    "room {} places unknown actor {}",
                    room.def.id, place.actor
                )));
            }
            let actor = parts
                .actors
                .iter()
                .find(|actor| actor.def.id == place.actor)
                .unwrap();
            let costume_id = place.costume.as_deref().unwrap_or("default");
            let costume = actor.def.costumes.get(costume_id).ok_or_else(|| {
                Error::msg(format!(
                    "room {} actor {} has no costume {costume_id}",
                    room.def.id, place.id
                ))
            })?;
            if let Some(anim) = &place.anim {
                if !costume.animations.contains_key(anim) {
                    return Err(Error::msg(format!(
                        "room {} actor {} animation {anim} is missing",
                        room.def.id, place.id
                    )));
                }
            }
            for state in &place.states {
                if !costume.animations.contains_key(&state.anim) {
                    return Err(Error::msg(format!(
                        "room {} actor {} state animation {} is missing",
                        room.def.id, place.id, state.anim
                    )));
                }
                for cond in &state.if_conds {
                    cond::parse_cond(cond)?;
                }
            }
        }
        all_rules.extend(room.def.rules.iter());
        all_rules.extend(room.def.on_enter.iter());
    }

    for rule in &all_rules {
        if rule.id.is_empty() || !seen_rules.insert(rule.id.as_str()) {
            return Err(Error::msg(format!(
                "duplicate or empty rule id {}",
                rule.id
            )));
        }
        if let Some(verb) = &rule.verb {
            if !verbs.contains(verb.as_str()) {
                return Err(Error::msg(format!(
                    "rule {} uses unknown verb {verb}",
                    rule.id
                )));
            }
        }
        for noun in [&rule.noun, &rule.second].into_iter().flatten() {
            if !nouns.contains(noun.as_str()) {
                return Err(Error::msg(format!(
                    "rule {} uses unknown noun {noun}",
                    rule.id
                )));
            }
        }
        for cond in &rule.req {
            cond::parse_cond(cond)?;
        }
        note_effects(rule, &items, &room_ids, parts, &mut scores)?;
    }

    for effect in extra_effects(parts) {
        if let Effect::Score { id, points } = effect {
            if let Some(prev) = scores.insert(id.clone(), points) {
                if prev != points {
                    return Err(Error::msg(format!(
                        "score id {id} is awarded as both {prev} and {points}"
                    )));
                }
            }
        }
    }
    for decl in &parts.game.declarations.scores {
        if let Some(prev) = scores.insert(decl.id.clone(), decl.points) {
            if prev != decl.points {
                return Err(Error::msg(format!(
                    "score id {} is awarded as both {prev} and {}",
                    decl.id, decl.points
                )));
            }
        }
    }

    let total: i32 = scores.values().sum();
    if total != parts.game.score_max {
        return Err(Error::msg(format!(
            "score awards sum to {total}, scoreMax is {}",
            parts.game.score_max
        )));
    }
    Ok(())
}

fn note_effects(
    rule: &Rule,
    items: &BTreeSet<&str>,
    rooms: &BTreeSet<&str>,
    parts: &PackageParts,
    scores: &mut BTreeMap<String, i32>,
) -> Result<()> {
    for effect in &rule.effects {
        match effect {
            Effect::Take { id } | Effect::Give { id } => {
                if !items.contains(id.as_str()) {
                    return Err(Error::msg(format!(
                        "rule {} uses unknown item {id}",
                        rule.id
                    )));
                }
            }
            Effect::Score { id, points } => {
                if let Some(prev) = scores.insert(id.clone(), *points) {
                    if prev != *points {
                        return Err(Error::msg(format!(
                            "score id {id} is awarded as both {prev} and {points}"
                        )));
                    }
                }
            }
            Effect::Goto { room, spawn } => {
                if !rooms.contains(room.as_str()) {
                    return Err(Error::msg(format!(
                        "rule {} goes to unknown room {room}",
                        rule.id
                    )));
                }
                let dest = parts
                    .rooms
                    .iter()
                    .find(|other| other.def.id == *room)
                    .unwrap();
                if !dest.def.spawns.contains_key(spawn) {
                    return Err(Error::msg(format!(
                        "rule {} goes to missing spawn {spawn}",
                        rule.id
                    )));
                }
            }
            _ => {}
        }
    }
    Ok(())
}

fn compile_rule(rule: &SourceRule, auto_id: &str) -> Result<Rule> {
    let id = if rule.id.is_empty() {
        auto_id.to_string()
    } else {
        rule.id.clone()
    };
    if rule.effects.is_some() && has_compact(rule) {
        return Err(Error::msg(format!(
            "rule {id} has both effects and compact fields"
        )));
    }
    let mut effects = if let Some(effects) = &rule.effects {
        effects.clone()
    } else {
        compact_effects(rule).map_err(|err| Error::msg(format!("rule {id}: {err}")))?
    };
    let marked = effects
        .iter()
        .any(|effect| matches!(effect, Effect::MarkOnce { id: existing } if existing == &id));
    if rule.once && !marked {
        effects.insert(0, Effect::MarkOnce { id: id.clone() });
    }
    Ok(Rule {
        id,
        verb: rule.verb.clone(),
        noun: rule.noun.clone(),
        second: rule.second.clone(),
        req: rule.req.clone(),
        once: rule.once,
        any_noun: rule.any_noun,
        any_second: rule.any_second,
        effects,
    })
}

fn has_compact(rule: &SourceRule) -> bool {
    rule.text.is_some()
        || !rule.texts.is_empty()
        || rule.say.is_some()
        || rule.take.is_some()
        || rule.give.is_some()
        || rule.set.is_some()
        || rule.clear.is_some()
        || rule.inc.is_some()
        || rule.set_var.is_some()
        || rule.score.is_some()
        || rule.sfx.is_some()
        || rule.checkpoint.is_some()
        || rule.goto.is_some()
        || rule.death.is_some()
        || rule.ending.is_some()
        || rule.wait.is_some()
        || rule.face.is_some()
        || rule.walk_to.is_some()
}

fn compact_effects(rule: &SourceRule) -> std::result::Result<Vec<Effect>, String> {
    let mut effects = Vec::new();
    if let Some(point) = rule.walk_to {
        effects.push(Effect::WalkTo {
            x: point[0],
            y: point[1],
        });
    }
    if let Some(face) = &rule.face {
        let dir = Dir::from_name(face).ok_or_else(|| format!("unknown facing {face}"))?;
        effects.push(Effect::Face { dir });
    }
    if let Some(id) = &rule.take {
        effects.push(Effect::Take { id: id.clone() });
    }
    if let Some(id) = &rule.give {
        effects.push(Effect::Give { id: id.clone() });
    }
    if let Some(flags) = &rule.set {
        for flag in flags.clone().list() {
            effects.push(Effect::Set { flag });
        }
    }
    if let Some(flags) = &rule.clear {
        for flag in flags.clone().list() {
            effects.push(Effect::Clear { flag });
        }
    }
    if let Some(inc) = &rule.inc {
        effects.push(Effect::Inc {
            var: inc.var.clone(),
            by: inc.by,
        });
    }
    if let Some(set) = &rule.set_var {
        effects.push(Effect::SetVar {
            var: set.var.clone(),
            value: set.value,
        });
    }
    if let Some(score) = &rule.score {
        effects.push(Effect::Score {
            id: score.id.clone(),
            points: score.points,
        });
    }
    if let Some(id) = &rule.sfx {
        effects.push(Effect::Sfx { id: id.clone() });
    }
    if let Some(text) = &rule.say {
        effects.push(Effect::Say { text: text.clone() });
    }
    if let Some(text) = &rule.text {
        effects.push(Effect::Say { text: text.clone() });
    }
    for text in &rule.texts {
        effects.push(Effect::Say { text: text.clone() });
    }
    if let Some(name) = &rule.checkpoint {
        effects.push(Effect::Checkpoint { name: name.clone() });
    }
    if let Some(goto) = &rule.goto {
        effects.push(Effect::Goto {
            room: goto.room.clone(),
            spawn: goto.spawn.clone(),
        });
    }
    if rule.death == Some(true) {
        effects.push(Effect::SetDead {
            restore_to: rule.restore_to.clone(),
        });
    }
    if let Some(id) = &rule.ending {
        effects.push(Effect::SetEnding { id: id.clone() });
    }
    if let Some(ticks) = rule.wait {
        effects.push(Effect::Wait { ticks });
    }
    Ok(effects)
}

fn check_boot(dir: &Path, hash: &str, update: bool) -> Result<()> {
    let path = dir.join("tests").join("boot.sha256");
    if update {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }
        fs::write(&path, format!("{hash}\n"))?;
        return Ok(());
    }
    let expected = fs::read_to_string(&path).map_err(|_| {
        Error::msg(format!(
            "missing {} (run advc test --update-snapshots)",
            path.display()
        ))
    })?;
    if expected.trim() != hash {
        return Err(Error::msg(format!(
            "boot frame hash mismatch\n  expected {}\n  actual   {hash}",
            expected.trim()
        )));
    }
    Ok(())
}

fn walk_files(dir: &Path) -> Result<Vec<PathBuf>> {
    let root = dir.join("tests");
    if !root.is_dir() {
        return Ok(Vec::new());
    }
    let mut files = Vec::new();
    for entry in fs::read_dir(&root)? {
        let entry = entry?;
        let path = entry.path();
        if path.extension().and_then(|ext| ext.to_str()) == Some("walk") {
            files.push(path);
        }
    }
    files.sort();
    Ok(files)
}

struct SnapshotHost {
    root: PathBuf,
    update: bool,
}

impl WalkHost for SnapshotHost {
    fn snapshot(
        &mut self,
        name: &str,
        rgba: &[u8],
        w: u32,
        h: u32,
    ) -> std::result::Result<(), String> {
        if name.is_empty() || name.contains('/') || name.contains('\\') || name.contains("..") {
            return Err(format!("bad snapshot name {name}"));
        }
        let hash = sha256_hex(rgba);
        let hash_path = self.root.join(format!("{name}.sha256"));
        let png_path = self.root.join(format!("{name}.png"));
        let image = RgbaImage {
            w,
            h,
            rgba: rgba.to_vec(),
        };
        let encoded = imageutil::encode_rgba(&image).map_err(|err| err.to_string())?;
        if self.update {
            fs::create_dir_all(&self.root).map_err(|err| err.to_string())?;
            fs::write(&hash_path, format!("{hash}\n")).map_err(|err| err.to_string())?;
            fs::write(&png_path, encoded).map_err(|err| err.to_string())?;
            return Ok(());
        }
        let expected = fs::read_to_string(&hash_path)
            .map_err(|_| format!("missing snapshot {name} (run advc test --update-snapshots)"))?;
        if expected.trim() == hash {
            return Ok(());
        }
        let actual_dir = self.root.join("_actual");
        let diff_dir = self.root.join("_diff");
        fs::create_dir_all(&actual_dir).map_err(|err| err.to_string())?;
        fs::create_dir_all(&diff_dir).map_err(|err| err.to_string())?;
        fs::write(actual_dir.join(format!("{name}.png")), &encoded)
            .map_err(|err| err.to_string())?;
        if let Ok(bytes) = fs::read(&png_path) {
            if let Ok(decoded) = imageutil::decode_png(&bytes) {
                let golden = decoded.into_rgba();
                if let Ok(diff) = diff_image(&golden, rgba, w, h) {
                    let _ = fs::write(diff_dir.join(format!("{name}.png")), diff);
                }
            }
        }
        Err(format!("snapshot {name} mismatch"))
    }

    fn save_slot(&mut self, name: &str, data: &[u8]) -> std::result::Result<(), String> {
        let path = self.root.join(format!("_saves/{name}.json"));
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).map_err(|err| err.to_string())?;
        }
        fs::write(path, data).map_err(|err| err.to_string())
    }

    fn load_slot(&mut self, name: &str) -> std::result::Result<Vec<u8>, String> {
        fs::read(self.root.join(format!("_saves/{name}.json"))).map_err(|err| err.to_string())
    }
}

fn diff_image(golden: &RgbaImage, actual: &[u8], w: u32, h: u32) -> Result<Vec<u8>> {
    let mut rgba = vec![0u8; (w * h * 4) as usize];
    for i in 0..(w * h) as usize {
        let g = golden
            .pixel((i as u32 % w) as i32, (i as u32 / w) as i32)
            .unwrap_or([0, 0, 0, 255]);
        let base = i * 4;
        let same = actual.len() >= base + 4
            && g[0] == actual[base]
            && g[1] == actual[base + 1]
            && g[2] == actual[base + 2]
            && g[3] == actual[base + 3];
        if same {
            rgba[base] = g[0] / 3;
            rgba[base + 1] = g[1] / 3;
            rgba[base + 2] = g[2] / 3;
            rgba[base + 3] = 255;
        } else {
            rgba[base] = 255;
            rgba[base + 1] = 0;
            rgba[base + 2] = 0;
            rgba[base + 3] = 255;
        }
    }
    imageutil::encode_rgba(&RgbaImage { w, h, rgba })
}

fn load_rgba(tree: &dyn ReadTree, path: &str) -> Result<RgbaImage> {
    let bytes = tree.read(path)?;
    imageutil::decode_png(&bytes)
        .map(|decoded| decoded.into_rgba())
        .map_err(|err| Error::msg(format!("{path}: {err}")))
}

fn load_gray(tree: &dyn ReadTree, path: &str) -> Result<lastcall_core::imageutil::GrayImage> {
    let bytes = tree.read(path)?;
    imageutil::decode_png(&bytes)
        .map(|decoded| decoded.into_gray())
        .map_err(|err| Error::msg(format!("{path}: {err}")))
}

fn list_dirs(tree: &dyn ReadTree, path: &str) -> Result<Vec<String>> {
    if !tree.is_dir(path) {
        return Err(Error::msg(format!("missing directory {path}")));
    }
    let mut out = Vec::new();
    for name in tree.list(path)? {
        let child = join_rel(path, &name);
        if tree.is_dir(&child) {
            out.push(child);
        }
    }
    Ok(out)
}

fn read_json(tree: &dyn ReadTree, path: &str) -> Result<Value> {
    let bytes = tree.read(path)?;
    let text = String::from_utf8(bytes).map_err(|err| Error::msg(format!("{path}: {err}")))?;
    serde_json::from_str(&text).map_err(|err| Error::msg(format!("{path}: {err}")))
}

fn compiled_schema(name: &str) -> Result<Value> {
    let mut value: Value = serde_json::from_str(schema_source(name)?)
        .map_err(|err| Error::msg(format!("{name}: {err}")))?;
    inline_refs(&mut value, 0)?;
    Ok(value)
}

fn check_schema(label: &str, schema: &Value, instance: &Value) -> Result<()> {
    let validator =
        jsonschema::validator_for(schema).map_err(|err| Error::msg(format!("{label}: {err}")))?;
    let mut errors = Vec::new();
    for err in validator.iter_errors(instance) {
        errors.push(err.to_string());
        if errors.len() == 8 {
            break;
        }
    }
    if errors.is_empty() {
        Ok(())
    } else {
        Err(Error::msg(format!("{label}: {}", errors.join("; "))))
    }
}

fn schema_source(name: &str) -> Result<&'static str> {
    Ok(match name {
        "game.schema.json" => include_str!("../../../schemas/game.schema.json"),
        "vocab.schema.json" => include_str!("../../../schemas/vocab.schema.json"),
        "items.schema.json" => include_str!("../../../schemas/items.schema.json"),
        "room.schema.json" => include_str!("../../../schemas/room.schema.json"),
        "walk.schema.json" => include_str!("../../../schemas/walk.schema.json"),
        "priority.schema.json" => include_str!("../../../schemas/priority.schema.json"),
        "actor.schema.json" => include_str!("../../../schemas/actor.schema.json"),
        "rule.schema.json" => include_str!("../../../schemas/rule.schema.json"),
        "effect.schema.json" => include_str!("../../../schemas/effect.schema.json"),
        "inc.schema.json" => include_str!("../../../schemas/inc.schema.json"),
        "set-var.schema.json" => include_str!("../../../schemas/set-var.schema.json"),
        "score.schema.json" => include_str!("../../../schemas/score.schema.json"),
        "goto.schema.json" => include_str!("../../../schemas/goto.schema.json"),
        "point.schema.json" => include_str!("../../../schemas/point.schema.json"),
        "frame.schema.json" => include_str!("../../../schemas/frame.schema.json"),
        "manifest.schema.json" => include_str!("../../../schemas/manifest.schema.json"),
        other => return Err(Error::msg(format!("unknown schema {other}"))),
    })
}

fn inline_refs(value: &mut Value, depth: u32) -> Result<()> {
    if depth > 16 {
        return Err(Error::msg("schema $ref nested too deeply"));
    }
    match value {
        Value::Array(items) => {
            for item in items {
                inline_refs(item, depth + 1)?;
            }
        }
        Value::Object(map) => {
            if map.len() == 1 {
                if let Some(Value::String(reference)) = map.get("$ref").cloned() {
                    if !reference.starts_with('#') && !reference.contains("://") {
                        let mut inner: Value = serde_json::from_str(schema_source(&reference)?)
                            .map_err(|err| Error::msg(format!("{reference}: {err}")))?;
                        if let Some(object) = inner.as_object_mut() {
                            object.remove("$schema");
                            object.remove("$id");
                            object.remove("title");
                        }
                        inline_refs(&mut inner, depth + 1)?;
                        *value = inner;
                        return Ok(());
                    }
                }
            }
            for child in map.values_mut() {
                inline_refs(child, depth + 1)?;
            }
        }
        _ => {}
    }
    Ok(())
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct SourceGame {
    id: String,
    title: String,
    version: String,
    #[serde(default = "default_format")]
    format_version: String,
    #[serde(default = "default_engine")]
    engine_min: String,
    #[serde(default = "default_resolution")]
    resolution: [u32; 2],
    start_room: String,
    #[serde(default = "default_spawn")]
    start_spawn: String,
    hero: String,
    #[serde(default)]
    score_max: i32,
    #[serde(default = "default_seed")]
    rng_seed: u64,
    #[serde(default = "default_reach")]
    reach: i32,
    #[serde(default)]
    intro: Vec<String>,
    #[serde(default)]
    rules: Vec<SourceRule>,
    #[serde(default)]
    hooks: lastcall_core::model::Hooks,
    #[serde(default)]
    timers: Vec<lastcall_core::model::MoveTimer>,
    #[serde(default)]
    declarations: lastcall_core::model::Declarations,
    #[serde(default)]
    carrying: Vec<String>,
    #[serde(default)]
    death_prompt: Option<String>,
}

fn default_format() -> String {
    "0.1.0".into()
}
fn default_engine() -> String {
    "0.1.0".into()
}
fn default_resolution() -> [u32; 2] {
    [SCREEN_W, SCREEN_H]
}
fn default_spawn() -> String {
    "default".into()
}
fn default_seed() -> u64 {
    1
}
fn default_reach() -> i32 {
    42
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct SourceRoom {
    id: String,
    name: String,
    #[serde(default)]
    look: String,
    #[serde(default)]
    spawns: BTreeMap<String, [i32; 2]>,
    #[serde(default)]
    exits: Vec<lastcall_core::model::ExitDef>,
    #[serde(default)]
    hotspots: Vec<lastcall_core::model::HotspotDef>,
    #[serde(default)]
    actors: Vec<lastcall_core::model::ActorPlace>,
    #[serde(default)]
    props: Vec<lastcall_core::model::PropDef>,
    #[serde(default)]
    pictures: Vec<String>,
    #[serde(default)]
    on_enter: Vec<SourceRule>,
    #[serde(default)]
    rules: Vec<SourceRule>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct SourceRule {
    #[serde(default)]
    id: String,
    #[serde(default)]
    verb: Option<String>,
    #[serde(default)]
    noun: Option<String>,
    #[serde(default)]
    second: Option<String>,
    #[serde(default)]
    any_second: bool,
    #[serde(default)]
    any_noun: bool,
    #[serde(default)]
    req: Vec<String>,
    #[serde(default)]
    once: bool,
    #[serde(default)]
    effects: Option<Vec<Effect>>,
    #[serde(default)]
    text: Option<String>,
    #[serde(default)]
    texts: Vec<String>,
    #[serde(default)]
    say: Option<String>,
    #[serde(default)]
    take: Option<String>,
    #[serde(default)]
    give: Option<String>,
    #[serde(default)]
    set: Option<OneOrMany>,
    #[serde(default)]
    clear: Option<OneOrMany>,
    #[serde(default)]
    inc: Option<IncDef>,
    #[serde(default)]
    set_var: Option<SetVarDef>,
    #[serde(default)]
    score: Option<ScoreDef>,
    #[serde(default)]
    sfx: Option<String>,
    #[serde(default)]
    checkpoint: Option<String>,
    #[serde(default)]
    goto: Option<GotoDef>,
    #[serde(default)]
    death: Option<bool>,
    #[serde(default)]
    ending: Option<String>,
    #[serde(default)]
    wait: Option<u32>,
    #[serde(default)]
    face: Option<String>,
    #[serde(default)]
    walk_to: Option<[i32; 2]>,
    #[serde(default, rename = "restoreTo")]
    restore_to: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(untagged)]
enum OneOrMany {
    One(String),
    Many(Vec<String>),
}

impl OneOrMany {
    fn list(self) -> Vec<String> {
        match self {
            OneOrMany::One(value) => vec![value],
            OneOrMany::Many(values) => values,
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
struct IncDef {
    var: String,
    #[serde(default = "one_i32")]
    by: i32,
}

#[derive(Debug, Clone, Deserialize)]
struct SetVarDef {
    var: String,
    value: i32,
}

#[derive(Debug, Clone, Deserialize)]
struct ScoreDef {
    id: String,
    points: i32,
}

#[derive(Debug, Clone, Deserialize)]
struct GotoDef {
    room: String,
    spawn: String,
}

fn one_i32() -> i32 {
    1
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct SourceActor {
    id: String,
    costumes: BTreeMap<String, SourceCostume>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct SourceCostume {
    #[serde(default)]
    sheet: String,
    #[serde(default)]
    frame_w: u32,
    #[serde(default)]
    frame_h: u32,
    #[serde(default)]
    ax: i32,
    #[serde(default)]
    ay: i32,
    #[serde(default)]
    animations: BTreeMap<String, SourceAnim>,
    #[serde(default)]
    sheets: Vec<SourceStrip>,
}

fn costume_strips(costume: &SourceCostume) -> Vec<SourceStrip> {
    if !costume.sheets.is_empty() {
        return costume.sheets.clone();
    }
    vec![SourceStrip {
        sheet: costume.sheet.clone(),
        frame_w: costume.frame_w,
        frame_h: costume.frame_h,
        ax: costume.ax,
        ay: costume.ay,
        animations: costume.animations.clone(),
    }]
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
struct SourceStrip {
    sheet: String,
    frame_w: u32,
    frame_h: u32,
    ax: i32,
    ay: i32,
    animations: BTreeMap<String, SourceAnim>,
}

#[derive(Debug, Clone, Deserialize)]
struct SourceAnim {
    #[serde(default = "default_fps")]
    fps: u32,
    #[serde(rename = "loop", default = "yes")]
    looped: bool,
    frames: Vec<u32>,
}

fn default_fps() -> u32 {
    1
}
fn yes() -> bool {
    true
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn compact_once_orders_mark_take_score_say_and_death_last() {
        let rule = SourceRule {
            id: "take_key".into(),
            verb: Some("take".into()),
            noun: Some("key".into()),
            second: None,
            req: vec!["!has:key".into()],
            once: true,
            effects: None,
            text: Some("You pocket the brass key.".into()),
            texts: Vec::new(),
            say: None,
            take: Some("key".into()),
            give: None,
            set: None,
            clear: None,
            inc: None,
            set_var: None,
            score: Some(ScoreDef {
                id: "found_key".into(),
                points: 5,
            }),
            sfx: None,
            checkpoint: None,
            goto: None,
            death: None,
            ending: None,
            wait: None,
            face: None,
            walk_to: None,
            restore_to: None,
            any_noun: false,
            any_second: false,
        };
        let compiled = compile_rule(&rule, "take_key").unwrap();
        assert!(matches!(compiled.effects[0], Effect::MarkOnce { .. }));
        assert!(matches!(compiled.effects[1], Effect::Take { .. }));
        assert!(matches!(compiled.effects[2], Effect::Score { .. }));
        assert!(matches!(compiled.effects[3], Effect::Say { .. }));

        let death = SourceRule {
            id: "climb_well".into(),
            verb: None,
            noun: None,
            second: None,
            req: Vec::new(),
            once: false,
            effects: None,
            text: Some("down".into()),
            texts: Vec::new(),
            say: None,
            take: None,
            give: None,
            set: None,
            clear: None,
            inc: None,
            set_var: None,
            score: None,
            sfx: None,
            checkpoint: None,
            goto: None,
            death: Some(true),
            ending: None,
            wait: None,
            face: None,
            walk_to: None,
            restore_to: None,
            any_noun: false,
            any_second: false,
        };
        let compiled = compile_rule(&death, "climb_well").unwrap();
        assert!(matches!(compiled.effects[0], Effect::Say { .. }));
        assert!(matches!(
            compiled.effects[1],
            Effect::SetDead { restore_to: None }
        ));
    }
}
