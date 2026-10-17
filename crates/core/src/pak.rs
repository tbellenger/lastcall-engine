use crate::cond::{self, Cond};
use crate::error::{Error, Result};
use crate::imageutil::{self, crop, Decoded, GrayImage, RgbaImage, SpriteFrame};
use crate::model::Vocab;
use crate::model::{ActorDef, Effect, GameDef, ItemDef, RoomDef, SCREEN_H, SCREEN_W};
use crate::parser::VocabIndex;
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet};
use std::io::{Cursor, Read, Write};
use zip::write::SimpleFileOptions;
use zip::{CompressionMethod, ZipArchive, ZipWriter};

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
struct Manifest {
    format_version: String,
    engine_min: String,
    game_id: String,
    game_version: String,
    content_hash: String,
    resources: Vec<ResourceEntry>,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
struct ResourceEntry {
    path: String,
    sha256: String,
    size: u64,
}

pub struct RoomPart {
    pub def: RoomDef,
    pub picture: RgbaImage,
    pub walk: GrayImage,
    pub priority: GrayImage,
    pub props: Option<RgbaImage>,
}

pub struct ActorPart {
    pub def: ActorDef,
    pub atlas: RgbaImage,
}

pub struct PackageParts {
    pub game: GameDef,
    pub vocab: Vocab,
    pub items: Vec<ItemDef>,
    pub rooms: Vec<RoomPart>,
    pub actors: Vec<ActorPart>,
}

#[derive(Debug)]
pub struct LoadedRoom {
    pub def: RoomDef,
    pub picture: RgbaImage,
    pub walk: Vec<u8>,
    pub priority: Vec<u8>,
    pub prop_frames: Vec<SpriteFrame>,
}

#[derive(Debug)]
pub struct LoadedActor {
    pub def: ActorDef,
    pub frames: BTreeMap<String, BTreeMap<String, Vec<SpriteFrame>>>,
}

#[derive(Debug)]
pub struct Package {
    pub game: GameDef,
    pub items: Vec<ItemDef>,
    pub item_index: BTreeMap<String, ItemDef>,
    pub rooms: BTreeMap<String, LoadedRoom>,
    pub actors: BTreeMap<String, LoadedActor>,
    pub vocab: VocabIndex,
    pub content_hash: String,
    pub pack_version: String,
    pub newer_minor: bool,
    pub score_table: BTreeMap<String, i32>,
    pub known_flags: BTreeSet<String>,
    pub known_vars: BTreeSet<String>,
    pub known_rules: BTreeSet<String>,
    pub endings: BTreeSet<String>,
}

pub fn build_pak(parts: &PackageParts) -> Result<Vec<u8>> {
    let mut files: BTreeMap<String, (Vec<u8>, bool)> = BTreeMap::new();
    files.insert(
        "game.json".into(),
        (serde_json::to_vec(&parts.game)?, false),
    );
    files.insert(
        "vocab.json".into(),
        (serde_json::to_vec(&parts.vocab)?, false),
    );
    files.insert(
        "items.json".into(),
        (serde_json::to_vec(&parts.items)?, false),
    );
    for room in &parts.rooms {
        let id = &room.def.id;
        files.insert(
            format!("rooms/{id}/room.json"),
            (serde_json::to_vec(&room.def)?, false),
        );
        files.insert(
            format!("rooms/{id}/picture.png"),
            (imageutil::encode_rgba(&room.picture)?, true),
        );
        files.insert(
            format!("rooms/{id}/walk.png"),
            (imageutil::encode_gray(&room.walk)?, true),
        );
        files.insert(
            format!("rooms/{id}/priority.png"),
            (imageutil::encode_gray(&room.priority)?, true),
        );
        if let Some(props) = &room.props {
            files.insert(
                format!("rooms/{id}/props.png"),
                (imageutil::encode_rgba(props)?, true),
            );
        }
    }
    for actor in &parts.actors {
        let id = &actor.def.id;
        files.insert(
            format!("actors/{id}/actor.json"),
            (serde_json::to_vec(&actor.def)?, false),
        );
        files.insert(
            format!("actors/{id}/atlas.png"),
            (imageutil::encode_rgba(&actor.atlas)?, true),
        );
    }

    let mut resources = Vec::new();
    let mut hasher = Sha256::new();
    for (path, (bytes, _)) in &files {
        let digest = sha256_hex(bytes);
        hasher.update(path.as_bytes());
        hasher.update([0]);
        hasher.update(digest.as_bytes());
        hasher.update([0]);
        hasher.update(bytes.len().to_string().as_bytes());
        hasher.update(b"\n");
        resources.push(ResourceEntry {
            path: path.clone(),
            sha256: digest,
            size: bytes.len() as u64,
        });
    }
    let manifest = Manifest {
        format_version: parts.game.format_version.clone(),
        engine_min: parts.game.engine_min.clone(),
        game_id: parts.game.id.clone(),
        game_version: parts.game.version.clone(),
        content_hash: hex_bytes(&hasher.finalize()),
        resources,
    };
    let manifest_bytes = serde_json::to_vec(&manifest)?;

    let mut zip = ZipWriter::new(Cursor::new(Vec::new()));
    let stored = SimpleFileOptions::default().compression_method(CompressionMethod::Stored);
    let deflated = SimpleFileOptions::default().compression_method(CompressionMethod::Deflated);
    let mut names: Vec<String> = files.keys().cloned().collect();
    names.push("manifest.json".into());
    names.sort();
    for name in names {
        if name == "manifest.json" {
            zip.start_file(&name, deflated)?;
            zip.write_all(&manifest_bytes)?;
        } else {
            let (bytes, is_stored) = &files[&name];
            zip.start_file(&name, if *is_stored { stored } else { deflated })?;
            zip.write_all(bytes)?;
        }
    }
    Ok(zip.finish()?.into_inner())
}

pub fn load_pak(bytes: &[u8]) -> Result<Package> {
    let mut archive = ZipArchive::new(Cursor::new(bytes))?;
    let mut files: BTreeMap<String, Vec<u8>> = BTreeMap::new();
    for i in 0..archive.len() {
        let mut file = archive.by_index(i)?;
        if file.is_dir() {
            continue;
        }
        let name = file.name().replace('\\', "/");
        let mut buf = Vec::new();
        file.read_to_end(&mut buf)?;
        files.insert(name, buf);
    }
    let manifest_bytes = files
        .get("manifest.json")
        .ok_or_else(|| Error::msg("package is missing manifest.json"))?;
    let manifest: Manifest = serde_json::from_slice(manifest_bytes)?;
    let (major, minor, _) = parse_semver(&manifest.format_version)?;
    if major != crate::model::FORMAT_MAJOR {
        return Err(Error::msg(format!(
            "unsupported format major version {major} (this engine understands {FORMAT_MAJOR}.x)",
            FORMAT_MAJOR = crate::model::FORMAT_MAJOR
        )));
    }
    let newer_minor = minor > crate::model::FORMAT_MINOR;

    let listed: BTreeMap<&str, &ResourceEntry> = manifest
        .resources
        .iter()
        .map(|entry| (entry.path.as_str(), entry))
        .collect();
    let mut hasher = Sha256::new();
    let mut ordered: Vec<&String> = files
        .keys()
        .filter(|k| k.as_str() != "manifest.json")
        .collect();
    ordered.sort();
    for path in &ordered {
        let bytes = &files[*path];
        let Some(entry) = listed.get(path.as_str()) else {
            return Err(Error::msg(format!("unexpected resource {path}")));
        };
        if entry.size != bytes.len() as u64 {
            return Err(Error::msg(format!("size mismatch for {path}")));
        }
        let digest = sha256_hex(bytes);
        if digest != entry.sha256 {
            return Err(Error::msg(format!("hash mismatch for {path}")));
        }
        hasher.update(path.as_bytes());
        hasher.update([0]);
        hasher.update(digest.as_bytes());
        hasher.update([0]);
        hasher.update(bytes.len().to_string().as_bytes());
        hasher.update(b"\n");
    }
    for entry in &manifest.resources {
        if !files.contains_key(&entry.path) {
            return Err(Error::msg(format!("missing resource {}", entry.path)));
        }
    }
    let content_hash = hex_bytes(&hasher.finalize());
    if content_hash != manifest.content_hash {
        return Err(Error::msg(
            "package content hash does not match the manifest",
        ));
    }

    let game: GameDef = serde_json::from_slice(
        files
            .get("game.json")
            .ok_or_else(|| Error::msg("missing game.json"))?,
    )?;
    if game.resolution != [SCREEN_W, SCREEN_H] {
        return Err(Error::msg(format!(
            "resolution must be {SCREEN_W}x{SCREEN_H} (game.json has {}x{})",
            game.resolution[0], game.resolution[1]
        )));
    }
    let vocab: Vocab = serde_json::from_slice(
        files
            .get("vocab.json")
            .ok_or_else(|| Error::msg("missing vocab.json"))?,
    )?;
    let items: Vec<ItemDef> = serde_json::from_slice(
        files
            .get("items.json")
            .ok_or_else(|| Error::msg("missing items.json"))?,
    )?;
    let vocab_index = VocabIndex::build(&vocab).map_err(Error::msg)?;

    let mut rooms = BTreeMap::new();
    let room_ids: Vec<String> = files
        .keys()
        .filter_map(|path| {
            let rest = path.strip_prefix("rooms/")?;
            let (id, tail) = rest.split_once('/')?;
            if tail == "room.json" {
                Some(id.to_string())
            } else {
                None
            }
        })
        .collect();
    for id in room_ids {
        let def: RoomDef = serde_json::from_slice(&files[&format!("rooms/{id}/room.json")])?;
        if def.id != id {
            return Err(Error::msg(format!(
                "room folder {id} does not match id {}",
                def.id
            )));
        }
        let picture = decode_rgba(&files[&format!("rooms/{id}/picture.png")], "picture")?;
        if picture.w != SCREEN_W || picture.h != SCREEN_H {
            return Err(Error::msg(format!(
                "room {id} picture is not {SCREEN_W}x{SCREEN_H}"
            )));
        }
        let walk = decode_gray(&files[&format!("rooms/{id}/walk.png")], "walk")?;
        let priority = decode_gray(&files[&format!("rooms/{id}/priority.png")], "priority")?;
        if walk.w != SCREEN_W
            || walk.h != SCREEN_H
            || priority.w != SCREEN_W
            || priority.h != SCREEN_H
        {
            return Err(Error::msg(format!("room {id} mask size mismatch")));
        }
        let atlas = match files.get(&format!("rooms/{id}/props.png")) {
            Some(bytes) => Some(decode_rgba(bytes, "props")?),
            None => None,
        };
        let mut prop_frames = Vec::new();
        for prop in &def.props {
            let atlas = atlas
                .as_ref()
                .ok_or_else(|| Error::msg(format!("room {id} has props but no props atlas")))?;
            prop_frames.push(crop(
                atlas,
                prop.frame.x,
                prop.frame.y,
                prop.frame.w,
                prop.frame.h,
                prop.frame.ax,
                prop.frame.ay,
            )?);
        }
        rooms.insert(
            id,
            LoadedRoom {
                def,
                picture,
                walk: walk.pixels,
                priority: priority.pixels,
                prop_frames,
            },
        );
    }

    let mut actors = BTreeMap::new();
    let actor_ids: Vec<String> = files
        .keys()
        .filter_map(|path| {
            let rest = path.strip_prefix("actors/")?;
            let (id, tail) = rest.split_once('/')?;
            if tail == "actor.json" {
                Some(id.to_string())
            } else {
                None
            }
        })
        .collect();
    for id in actor_ids {
        let def: ActorDef = serde_json::from_slice(&files[&format!("actors/{id}/actor.json")])?;
        if def.id != id {
            return Err(Error::msg(format!(
                "actor folder {id} does not match {}",
                def.id
            )));
        }
        let atlas = decode_rgba(
            files
                .get(&format!("actors/{id}/atlas.png"))
                .ok_or_else(|| Error::msg(format!("actor {id} is missing an atlas")))?,
            "atlas",
        )?;
        let mut frames = BTreeMap::new();
        for (costume_id, costume) in &def.costumes {
            let mut anims = BTreeMap::new();
            for (anim_id, anim) in &costume.animations {
                let mut cropped = Vec::new();
                for frame in &anim.frames {
                    cropped.push(crop(
                        &atlas, frame.x, frame.y, frame.w, frame.h, frame.ax, frame.ay,
                    )?);
                }
                anims.insert(anim_id.clone(), cropped);
            }
            frames.insert(costume_id.clone(), anims);
        }
        actors.insert(id, LoadedActor { def, frames });
    }

    let mut item_index = BTreeMap::new();
    for item in &items {
        if item_index.insert(item.id.clone(), item.clone()).is_some() {
            return Err(Error::msg(format!("duplicate item {}", item.id)));
        }
    }

    let mut package = Package {
        game,
        items,
        item_index,
        rooms,
        actors,
        vocab: vocab_index,
        content_hash,
        pack_version: manifest.game_version,
        newer_minor,
        score_table: BTreeMap::new(),
        known_flags: BTreeSet::new(),
        known_vars: BTreeSet::new(),
        known_rules: BTreeSet::new(),
        endings: BTreeSet::new(),
    };
    index_package(&mut package)?;
    if !package.rooms.contains_key(&package.game.start_room) {
        return Err(Error::msg(format!(
            "start room {} is missing",
            package.game.start_room
        )));
    }
    if !package.actors.contains_key(&package.game.hero) {
        return Err(Error::msg(format!(
            "hero actor {} is missing",
            package.game.hero
        )));
    }
    Ok(package)
}

fn index_package(package: &mut Package) -> Result<()> {
    let mut rules = Vec::new();
    rules.extend(package.game.rules.clone());
    for room in package.rooms.values() {
        rules.extend(room.def.rules.clone());
        rules.extend(room.def.on_enter.clone());
        for actor in &room.def.actors {
            for state in &actor.states {
                for cond in &state.if_conds {
                    note_cond(&mut package.known_flags, &mut package.known_vars, cond)?;
                }
            }
        }
        for prop in &room.def.props {
            if let Some(cond) = &prop.show_if {
                note_cond(&mut package.known_flags, &mut package.known_vars, cond)?;
            }
        }
        for exit in &room.def.exits {
            if let Some(cond) = &exit.blocked_if {
                note_cond(&mut package.known_flags, &mut package.known_vars, cond)?;
            }
        }
    }
    for rule in &rules {
        if !package.known_rules.insert(rule.id.clone()) {
            return Err(Error::msg(format!("duplicate rule id {}", rule.id)));
        }
        for cond in &rule.req {
            note_cond(&mut package.known_flags, &mut package.known_vars, cond)?;
        }
        for effect in &rule.effects {
            match effect {
                Effect::Set { flag } | Effect::Clear { flag } => {
                    package.known_flags.insert(flag.clone());
                }
                Effect::Inc { var, .. } | Effect::SetVar { var, .. } => {
                    package.known_vars.insert(var.clone());
                }
                Effect::Score { id, points } => {
                    if let Some(prev) = package.score_table.insert(id.clone(), *points) {
                        if prev != *points {
                            return Err(Error::msg(format!(
                                "score id {id} is awarded as both {prev} and {points}"
                            )));
                        }
                    }
                }
                Effect::SetEnding { id } => {
                    package.endings.insert(id.clone());
                }
                _ => {}
            }
        }
    }
    Ok(())
}

fn note_cond(flags: &mut BTreeSet<String>, vars: &mut BTreeSet<String>, text: &str) -> Result<()> {
    let cond = cond::parse_cond(text)?;
    collect_cond(flags, vars, &cond);
    Ok(())
}

fn collect_cond(flags: &mut BTreeSet<String>, vars: &mut BTreeSet<String>, cond: &Cond) {
    match cond {
        Cond::Flag(id) => {
            flags.insert(id.clone());
        }
        Cond::Var { name, .. } => {
            vars.insert(name.clone());
        }
        Cond::Not(inner) => collect_cond(flags, vars, inner),
        _ => {}
    }
}

fn decode_rgba(bytes: &[u8], what: &str) -> Result<RgbaImage> {
    match imageutil::decode_png(bytes) {
        Ok(decoded) => Ok(decoded.into_rgba()),
        Err(err) => Err(Error::msg(format!("{what} png: {err}"))),
    }
}

fn decode_gray(bytes: &[u8], what: &str) -> Result<GrayImage> {
    match imageutil::decode_png(bytes) {
        Ok(Decoded::Gray(img)) => Ok(img),
        Ok(other) => Ok(other.into_gray()),
        Err(err) => Err(Error::msg(format!("{what} png: {err}"))),
    }
}

pub fn parse_semver(text: &str) -> Result<(u32, u32, u32)> {
    let text = text.trim().trim_start_matches('v');
    let mut parts = text.split('.');
    let major = parts
        .next()
        .and_then(|p| p.parse().ok())
        .ok_or_else(|| Error::msg(format!("bad version '{text}'")))?;
    let minor = parts
        .next()
        .and_then(|p| p.parse().ok())
        .ok_or_else(|| Error::msg(format!("bad version '{text}'")))?;
    let patch = parts
        .next()
        .and_then(|p| p.parse().ok())
        .ok_or_else(|| Error::msg(format!("bad version '{text}'")))?;
    if parts.next().is_some() {
        return Err(Error::msg(format!("bad version '{text}'")));
    }
    Ok((major, minor, patch))
}

pub fn sha256_hex(bytes: &[u8]) -> String {
    hex_bytes(&Sha256::digest(bytes))
}

fn hex_bytes(bytes: &[u8]) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut out = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        out.push(HEX[(byte >> 4) as usize] as char);
        out.push(HEX[(byte & 0xf) as usize] as char);
    }
    out
}
