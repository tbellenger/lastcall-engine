//! Studio session: open a package or an authoring folder, edit room geometry, save.
//!
//! Parsing and pathfinding go through the same crates as `advc validate` and the engine.

use crate::load_tree;
use crate::tree::{join_rel, DiskTree, MemTree, ReadTree};
use lastcall_core::geom::{self, astar};
use lastcall_core::imageutil::{self, RgbaImage};
use lastcall_core::model::{
    Band, ExitDef, HotspotDef, Polygon, PriorityDef, WalkDef, SCREEN_H, SCREEN_W,
};
use lastcall_core::pak::{self};
use lastcall_core::render::{self, Layer};
use lastcall_core::{Error, Result};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::{BTreeMap, HashMap};
use std::path::Path;

const PIXELS: usize = (SCREEN_W * SCREEN_H) as usize;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RoomDoc {
    pub id: String,
    pub name: String,
    pub polygons: Vec<Polygon>,
    pub bands: Vec<Band>,
    pub hotspots: Vec<HotspotDef>,
    pub exits: Vec<ExitDef>,
    pub spawns: BTreeMap<String, [i32; 2]>,
    /// Polygons rasterise to the walk mask the ghost path uses.
    pub polygons_live: bool,
}

struct RoomRec {
    origin_id: String,
    doc: RoomDoc,
    baseline: RoomDoc,
    room_json: Value,
    mask: Vec<u8>,
    base_mask: Vec<u8>,
    priority: Vec<u8>,
    base_priority: Vec<u8>,
    /// Pixels whose packed priority is not explained by the band lines.
    paint: Vec<u8>,
}

struct Hero {
    frame: lastcall_core::imageutil::SpriteFrame,
    png: Vec<u8>,
}

enum Kind {
    Source,
    Pak,
}

pub struct Session {
    kind: Kind,
    title: String,
    game_id: String,
    format_version: String,
    engine_min: String,
    game_version: String,
    /// Original project or package entries, including `manifest.json` for a pak.
    files: BTreeMap<String, Vec<u8>>,
    /// Set when the session was opened from a `.advpak` blob.
    original_pak: Option<Vec<u8>>,
    rooms: BTreeMap<String, RoomRec>,
    hero: Hero,
}

#[derive(Debug, Clone)]
pub struct HeroOverlay {
    pub priority: u8,
    pub rgba: Vec<u8>,
}

pub struct ChangeSet {
    pub files: BTreeMap<String, Vec<u8>>,
    pub deleted: Vec<String>,
}

impl Session {
    pub fn open_dir(dir: &Path) -> Result<Self> {
        let tree = DiskTree::new(dir);
        let files = slurp(&tree)?;
        Self::open_source_files(files)
    }

    pub fn open_bytes(bytes: &[u8]) -> Result<Self> {
        let files = strip_wrapper(pak::read_entries(bytes)?);
        if files.keys().any(|path| path.ends_with("/walk.json")) {
            return Self::open_source_files(files);
        }
        if files.contains_key("manifest.json") {
            return Self::open_pak_files(files, bytes);
        }
        if files.contains_key("game.json") {
            return Self::open_source_files(files);
        }
        Err(Error::msg(
            "not a game package or project (need manifest.json or game.json)",
        ))
    }

    /// A directory listing: an authoring folder, or an unpacked package (has `manifest.json`).
    pub fn open_loose(files: BTreeMap<String, Vec<u8>>) -> Result<Self> {
        let files = strip_wrapper(files);
        if files.keys().any(|path| path.ends_with("/walk.json")) {
            return Self::open_source_files(files);
        }
        if files.contains_key("manifest.json") {
            let zipped = pak::zip_files(&files)?;
            return Self::open_bytes(&zipped);
        }
        Self::open_source_files(files)
    }

    pub fn open_source_files(files: BTreeMap<String, Vec<u8>>) -> Result<Self> {
        let files = strip_wrapper(files);
        let tree = MemTree::from_map(files.clone());
        let parts = load_tree(&tree)?;
        let hero = hero_from_parts(&parts)?;
        let mut rooms = BTreeMap::new();
        for room in &parts.rooms {
            let id = room.def.id.clone();
            let folder = format!("rooms/{id}");
            let room_bytes = files
                .get(&format!("{folder}/room.json"))
                .ok_or_else(|| Error::msg(format!("missing {folder}/room.json")))?;
            let room_json: Value = serde_json::from_slice(room_bytes)?;
            let walk: WalkDef = serde_json::from_slice(
                files
                    .get(&format!("{folder}/walk.json"))
                    .ok_or_else(|| Error::msg(format!("missing {folder}/walk.json")))?,
            )?;
            let priority_def: PriorityDef = serde_json::from_slice(
                files
                    .get(&format!("{folder}/priority.json"))
                    .ok_or_else(|| Error::msg(format!("missing {folder}/priority.json")))?,
            )?;
            let mut bands = priority_def.bands;
            bands.sort_by_key(|band| band.y);
            let mask = geom::rasterize(SCREEN_W as i32, SCREEN_H as i32, &walk.polygons);
            let (priority, paint) = priority_and_paint(
                &bands,
                files
                    .get(&format!("{folder}/priority.png"))
                    .map(Vec::as_slice),
            )?;
            let doc = RoomDoc {
                id: id.clone(),
                name: room.def.name.clone(),
                polygons: walk.polygons,
                bands,
                hotspots: room.def.hotspots.clone(),
                exits: room.def.exits.clone(),
                spawns: room.def.spawns.clone(),
                polygons_live: true,
            };
            rooms.insert(
                id.clone(),
                RoomRec {
                    origin_id: id,
                    baseline: doc.clone(),
                    doc,
                    room_json,
                    mask: mask.clone(),
                    base_mask: mask,
                    priority: priority.clone(),
                    base_priority: priority,
                    paint,
                },
            );
        }
        Ok(Self {
            kind: Kind::Source,
            title: parts.game.title.clone(),
            game_id: parts.game.id.clone(),
            format_version: parts.game.format_version.clone(),
            engine_min: parts.game.engine_min.clone(),
            game_version: parts.game.version.clone(),
            files,
            original_pak: None,
            rooms,
            hero,
        })
    }

    fn open_pak_files(files: BTreeMap<String, Vec<u8>>, pak_bytes: &[u8]) -> Result<Self> {
        let package = pak::load_pak(pak_bytes)?;
        let hero_id = package.game.hero.clone();
        let actor = package
            .actors
            .get(&hero_id)
            .ok_or_else(|| Error::msg(format!("hero actor {hero_id} is missing")))?;
        let frame = actor
            .frames
            .get("default")
            .and_then(|anims| anims.get("stand_s"))
            .and_then(|frames| frames.first())
            .ok_or_else(|| Error::msg("hero costume default needs a stand_s animation"))?
            .clone();
        let hero = Hero {
            png: encode_sprite(&frame)?,
            frame,
        };
        let mut rooms = BTreeMap::new();
        for (id, room) in &package.rooms {
            let path = format!("rooms/{id}/room.json");
            let room_json: Value = serde_json::from_slice(
                files
                    .get(&path)
                    .ok_or_else(|| Error::msg(format!("missing {path}")))?,
            )?;
            if !files.contains_key(&format!("rooms/{id}/picture.png")) {
                return Err(Error::msg(format!("missing rooms/{id}/picture.png")));
            }
            let traced = trace_polygons(&room.walk);
            let polygons_live = traced.is_some();
            let polygons = traced.unwrap_or_default();
            let mut bands = room.def.bands.clone();
            bands.sort_by_key(|band| band.y);
            let paint = paint_from_priority(&bands, &room.priority);
            let priority = room.priority.clone();
            let doc = RoomDoc {
                id: id.clone(),
                name: room.def.name.clone(),
                polygons,
                bands,
                hotspots: room.def.hotspots.clone(),
                exits: room.def.exits.clone(),
                spawns: room.def.spawns.clone(),
                polygons_live,
            };
            rooms.insert(
                id.clone(),
                RoomRec {
                    origin_id: id.clone(),
                    baseline: doc.clone(),
                    doc,
                    room_json,
                    mask: room.walk.clone(),
                    base_mask: room.walk.clone(),
                    priority: priority.clone(),
                    base_priority: room.priority.clone(),
                    paint,
                },
            );
        }
        Ok(Self {
            kind: Kind::Pak,
            title: package.game.title.clone(),
            game_id: package.game.id.clone(),
            format_version: package.game.format_version.clone(),
            engine_min: package.game.engine_min.clone(),
            game_version: package.game.version.clone(),
            files,
            original_pak: Some(pak_bytes.to_vec()),
            rooms,
            hero,
        })
    }

    pub fn is_pak(&self) -> bool {
        matches!(self.kind, Kind::Pak)
    }

    pub fn title(&self) -> &str {
        &self.title
    }

    pub fn game_id(&self) -> &str {
        &self.game_id
    }

    pub fn dirty(&self) -> bool {
        self.rooms.values().any(room_dirty)
    }

    pub fn rooms_json(&self) -> String {
        let list: Vec<Value> = self
            .rooms
            .values()
            .map(|room| {
                serde_json::json!({
                    "id": room.doc.id,
                    "name": room.doc.name,
                })
            })
            .collect();
        serde_json::to_string(&list).unwrap_or_else(|_| "[]".into())
    }

    pub fn room_json(&self, id: &str) -> Result<String> {
        let room = self
            .rooms
            .get(id)
            .ok_or_else(|| Error::msg(format!("unknown room {id}")))?;
        Ok(serde_json::to_string(&room.doc)?)
    }

    pub fn picture_png(&self, id: &str) -> Result<Vec<u8>> {
        let room = self.room(id)?;
        let path = format!("rooms/{}/picture.png", room.origin_id);
        self.files
            .get(&path)
            .cloned()
            .ok_or_else(|| Error::msg(format!("missing {path}")))
    }

    pub fn mask(&self, id: &str) -> Result<Vec<u8>> {
        Ok(self.room(id)?.mask.clone())
    }

    pub fn priority_map(&self, id: &str) -> Result<Vec<u8>> {
        Ok(self.room(id)?.priority.clone())
    }

    pub fn set_room_json(&mut self, id: &str, json: &str) -> Result<()> {
        let mut doc: RoomDoc = serde_json::from_str(json)?;
        if doc.id.is_empty() || doc.id.contains('/') || doc.id.contains("..") {
            return Err(Error::msg("room id must be a single path segment"));
        }
        if doc.id != id && self.rooms.contains_key(&doc.id) {
            return Err(Error::msg(format!("room id {} is already used", doc.id)));
        }
        doc.bands.sort_by_key(|band| band.y);
        for band in &doc.bands {
            if band.y < 0 || band.y > SCREEN_H as i32 {
                return Err(Error::msg("priority band y is outside the picture"));
            }
        }
        let mut room = self
            .rooms
            .remove(id)
            .ok_or_else(|| Error::msg(format!("unknown room {id}")))?;
        if doc.polygons.is_empty() {
            if room.baseline.polygons_live {
                room.mask = vec![0; PIXELS];
                doc.polygons_live = true;
            } else {
                room.mask = room.base_mask.clone();
                doc.polygons_live = false;
            }
        } else {
            room.mask = geom::rasterize(SCREEN_W as i32, SCREEN_H as i32, &doc.polygons);
            doc.polygons_live = true;
        }
        if doc.bands == room.baseline.bands {
            room.priority = room.base_priority.clone();
        } else {
            let paint = if room.paint.iter().any(|pixel| *pixel != 0) {
                Some(room.paint.as_slice())
            } else {
                None
            };
            room.priority =
                render::merge_priority(SCREEN_W as i32, SCREEN_H as i32, &doc.bands, paint);
        }
        room.doc = doc;
        self.rooms.insert(room.doc.id.clone(), room);
        Ok(())
    }

    pub fn plan_path(&self, id: &str, x0: i32, y0: i32, x1: i32, y1: i32) -> Vec<[i32; 2]> {
        let Ok(room) = self.room(id) else {
            return Vec::new();
        };
        let Some(path) = astar(
            &room.mask,
            SCREEN_W as i32,
            SCREEN_H as i32,
            (x0, y0),
            (x1, y1),
        ) else {
            return Vec::new();
        };
        simplify_path(&path)
    }

    pub fn hero_png(&self) -> &[u8] {
        &self.hero.png
    }

    pub fn hero_meta_json(&self) -> String {
        serde_json::json!({
            "w": self.hero.frame.w,
            "h": self.hero.frame.h,
            "ax": self.hero.frame.ax,
            "ay": self.hero.frame.ay,
        })
        .to_string()
    }

    /// Full-screen RGBA with only the hero. Occluded pixels stay transparent.
    pub fn hero_overlay(&self, id: &str, feet_x: i32, feet_y: i32) -> Result<HeroOverlay> {
        let room = self.room(id)?;
        let priority = render::band_for_y(&room.doc.bands, feet_y);
        let mut fb = vec![0u8; PIXELS * 4];
        let layer = Layer {
            id: "hero".into(),
            priority,
            sort_y: feet_y,
            dest_x: feet_x - self.hero.frame.ax,
            dest_y: feet_y - self.hero.frame.ay,
            sprite: self.hero.frame.clone(),
            occlude: None,
        };
        render::paint_sprite(
            &mut fb,
            SCREEN_W as i32,
            SCREEN_H as i32,
            &room.priority,
            &layer,
        );
        Ok(HeroOverlay { priority, rgba: fb })
    }

    pub fn export_files(&self) -> Result<BTreeMap<String, Vec<u8>>> {
        let files = self.materialize()?;
        if self.dirty() {
            match self.kind {
                Kind::Source => {
                    let parts = load_tree(&MemTree::from_map(files.clone()))?;
                    let packed = pak::build_pak(&parts)?;
                    pak::load_pak(&packed)?;
                }
                Kind::Pak => {
                    let packed = self.pack_files(&files)?;
                    pak::load_pak(&packed)?;
                }
            }
        }
        Ok(files)
    }

    pub fn export_pak(&self) -> Result<Vec<u8>> {
        if !self.dirty() {
            if let Some(bytes) = &self.original_pak {
                return Ok(bytes.clone());
            }
        }
        let files = self.export_files()?;
        match self.kind {
            Kind::Pak => self.pack_files(&files),
            Kind::Source => {
                let parts = load_tree(&MemTree::from_map(files))?;
                pak::build_pak(&parts)
            }
        }
    }

    pub fn export_changes(&self) -> Result<ChangeSet> {
        let next = self.export_files()?;
        let mut files = BTreeMap::new();
        let mut deleted = Vec::new();
        for (path, bytes) in &next {
            match self.files.get(path) {
                Some(old) if old == bytes => {}
                _ => {
                    files.insert(path.clone(), bytes.clone());
                }
            }
        }
        for path in self.files.keys() {
            if !next.contains_key(path) {
                deleted.push(path.clone());
            }
        }
        Ok(ChangeSet { files, deleted })
    }

    pub fn export_project_zip(&self) -> Result<Vec<u8>> {
        let files = self.export_files()?;
        match self.kind {
            Kind::Pak => self.export_pak(),
            Kind::Source => pak::zip_files(&files),
        }
    }

    /// Paths whose bytes changed, plus paths to delete after a rename.
    pub fn changes_json(&self) -> Result<String> {
        let next = self.materialize()?;
        let mut changes = Vec::new();
        for (path, bytes) in &next {
            match self.files.get(path) {
                Some(old) if old == bytes => {}
                _ => changes.push(serde_json::json!({"path": path, "deleted": false})),
            }
        }
        for path in self.files.keys() {
            if !next.contains_key(path) {
                changes.push(serde_json::json!({"path": path, "deleted": true}));
            }
        }
        Ok(serde_json::to_string(&changes)?)
    }

    pub fn file_bytes(&self, path: &str) -> Result<Vec<u8>> {
        let files = self.export_files()?;
        files
            .get(path)
            .cloned()
            .ok_or_else(|| Error::msg(format!("missing {path}")))
    }

    fn room(&self, id: &str) -> Result<&RoomRec> {
        self.rooms
            .get(id)
            .ok_or_else(|| Error::msg(format!("unknown room {id}")))
    }

    fn materialize(&self) -> Result<BTreeMap<String, Vec<u8>>> {
        let mut files = self.files.clone();
        let mut renames: Vec<(String, String)> = Vec::new();
        for room in self.rooms.values() {
            if room.doc.id != room.origin_id {
                renames.push((room.origin_id.clone(), room.doc.id.clone()));
            }
            if !room_dirty(room) && room.doc.id == room.origin_id {
                continue;
            }
            self.write_room(&mut files, room)?;
        }
        for (from, to) in &renames {
            rename_prefix(
                &mut files,
                &format!("rooms/{from}/"),
                &format!("rooms/{to}/"),
            );
            retarget_id(&mut files, from, to)?;
        }
        Ok(files)
    }

    fn write_room(&self, files: &mut BTreeMap<String, Vec<u8>>, room: &RoomRec) -> Result<()> {
        let folder = format!("rooms/{}", room.origin_id);
        if room.doc.polygons != room.baseline.polygons {
            match self.kind {
                Kind::Source => {
                    let walk = WalkDef {
                        polygons: room.doc.polygons.clone(),
                    };
                    files.insert(format!("{folder}/walk.json"), pretty_json(&walk)?);
                }
                Kind::Pak => {
                    let image = lastcall_core::imageutil::GrayImage {
                        w: SCREEN_W,
                        h: SCREEN_H,
                        pixels: room.mask.clone(),
                    };
                    files.insert(
                        format!("{folder}/walk.png"),
                        imageutil::encode_gray(&image)?,
                    );
                }
            }
        } else if matches!(self.kind, Kind::Pak) && room.mask != room.base_mask {
            let image = lastcall_core::imageutil::GrayImage {
                w: SCREEN_W,
                h: SCREEN_H,
                pixels: room.mask.clone(),
            };
            files.insert(
                format!("{folder}/walk.png"),
                imageutil::encode_gray(&image)?,
            );
        }
        if room.doc.bands != room.baseline.bands {
            match self.kind {
                Kind::Source => {
                    let priority = PriorityDef {
                        bands: room.doc.bands.clone(),
                    };
                    files.insert(format!("{folder}/priority.json"), pretty_json(&priority)?);
                }
                Kind::Pak => {}
            }
        }
        if room.priority != room.base_priority {
            let image = lastcall_core::imageutil::GrayImage {
                w: SCREEN_W,
                h: SCREEN_H,
                pixels: room.priority.clone(),
            };
            if matches!(self.kind, Kind::Pak)
                || files.contains_key(&format!("{folder}/priority.png"))
            {
                files.insert(
                    format!("{folder}/priority.png"),
                    imageutil::encode_gray(&image)?,
                );
            }
        }
        if geometry_changed(room)
            || room.doc.id != room.origin_id
            || room.doc.name != room.baseline.name
        {
            let mut value = room.room_json.clone();
            patch_room_value(&mut value, room, matches!(self.kind, Kind::Pak))?;
            let bytes = match self.kind {
                Kind::Source => pretty_json(&value)?,
                Kind::Pak => serde_json::to_vec(&value)?,
            };
            files.insert(format!("{folder}/room.json"), bytes);
        }
        Ok(())
    }

    fn pack_files(&self, files: &BTreeMap<String, Vec<u8>>) -> Result<Vec<u8>> {
        let plain = files
            .iter()
            .filter(|(path, _)| path.as_str() != "manifest.json")
            .map(|(path, bytes)| (path.clone(), bytes.clone()))
            .collect();
        pak::write_package(
            &plain,
            &self.format_version,
            &self.engine_min,
            &self.game_id,
            &self.game_version,
        )
    }
}

fn room_dirty(room: &RoomRec) -> bool {
    room.doc.id != room.baseline.id
        || room.doc.name != room.baseline.name
        || room.doc.polygons != room.baseline.polygons
        || room.doc.bands != room.baseline.bands
        || room.doc.hotspots != room.baseline.hotspots
        || room.doc.exits != room.baseline.exits
        || room.doc.spawns != room.baseline.spawns
}

fn geometry_changed(room: &RoomRec) -> bool {
    room.doc.hotspots != room.baseline.hotspots
        || room.doc.exits != room.baseline.exits
        || room.doc.spawns != room.baseline.spawns
        || room.doc.bands != room.baseline.bands
}

fn patch_room_value(value: &mut Value, room: &RoomRec, include_bands: bool) -> Result<()> {
    let Some(object) = value.as_object_mut() else {
        return Err(Error::msg("room.json is not an object"));
    };
    object.insert("id".into(), Value::String(room.doc.id.clone()));
    object.insert("name".into(), Value::String(room.doc.name.clone()));
    object.insert("spawns".into(), serde_json::to_value(&room.doc.spawns)?);
    object.insert("exits".into(), serde_json::to_value(&room.doc.exits)?);
    object.insert("hotspots".into(), serde_json::to_value(&room.doc.hotspots)?);
    if include_bands {
        object.insert("bands".into(), serde_json::to_value(&room.doc.bands)?);
    }
    Ok(())
}

fn rename_prefix(files: &mut BTreeMap<String, Vec<u8>>, from: &str, to: &str) {
    let keys: Vec<String> = files
        .keys()
        .filter(|path| path.starts_with(from))
        .cloned()
        .collect();
    for key in keys {
        if let Some(bytes) = files.remove(&key) {
            let next = format!("{to}{}", &key[from.len()..]);
            files.insert(next, bytes);
        }
    }
}

fn retarget_id(files: &mut BTreeMap<String, Vec<u8>>, from: &str, to: &str) -> Result<()> {
    if let Some(bytes) = files.get("game.json").cloned() {
        let mut value: Value = serde_json::from_slice(&bytes)?;
        let mut changed = false;
        if let Some(object) = value.as_object_mut() {
            for key in ["startRoom", "start_room"] {
                if object.get(key).and_then(|v| v.as_str()) == Some(from) {
                    object.insert(key.into(), Value::String(to.into()));
                    changed = true;
                }
            }
        }
        if changed {
            let pretty = bytes.contains(&b'\n');
            let encoded = if pretty {
                pretty_json(&value)?
            } else {
                serde_json::to_vec(&value)?
            };
            files.insert("game.json".into(), encoded);
        }
    }
    let room_paths: Vec<String> = files
        .keys()
        .filter(|path| path.starts_with("rooms/") && path.ends_with("/room.json"))
        .cloned()
        .collect();
    for path in room_paths {
        let bytes = files[&path].clone();
        let mut value: Value = serde_json::from_slice(&bytes)?;
        if !retarget_exits(&mut value, from, to) {
            continue;
        }
        let pretty = bytes.contains(&b'\n');
        let encoded = if pretty {
            pretty_json(&value)?
        } else {
            serde_json::to_vec(&value)?
        };
        files.insert(path, encoded);
    }
    Ok(())
}

fn retarget_exits(value: &mut Value, from: &str, to: &str) -> bool {
    let mut changed = false;
    let Some(exits) = value.get_mut("exits").and_then(|v| v.as_array_mut()) else {
        return false;
    };
    for exit in exits {
        if exit.get("to").and_then(|v| v.as_str()) == Some(from) {
            if let Some(object) = exit.as_object_mut() {
                object.insert("to".into(), Value::String(to.into()));
                changed = true;
            }
        }
    }
    changed
}

fn pretty_json(value: &impl Serialize) -> Result<Vec<u8>> {
    let mut bytes = serde_json::to_vec_pretty(value)?;
    bytes.push(b'\n');
    Ok(bytes)
}

fn paint_from_priority(bands: &[Band], priority: &[u8]) -> Vec<u8> {
    let band_only = render::merge_priority(SCREEN_W as i32, SCREEN_H as i32, bands, None);
    let mut paint = vec![0u8; PIXELS];
    if priority.len() != PIXELS || band_only.len() != PIXELS {
        return paint;
    }
    for i in 0..PIXELS {
        if priority[i] != band_only[i] {
            paint[i] = priority[i];
        }
    }
    paint
}

fn priority_and_paint(bands: &[Band], png: Option<&[u8]>) -> Result<(Vec<u8>, Vec<u8>)> {
    let band_only = render::merge_priority(SCREEN_W as i32, SCREEN_H as i32, bands, None);
    let mut paint = vec![0u8; PIXELS];
    let Some(bytes) = png else {
        return Ok((band_only, paint));
    };
    let image = imageutil::decode_png(bytes)
        .map_err(|err| Error::msg(format!("priority.png: {err}")))?
        .into_gray();
    if image.w != SCREEN_W || image.h != SCREEN_H || image.pixels.len() != PIXELS {
        return Err(Error::msg("priority.png is not 384x216"));
    }
    for i in 0..PIXELS {
        if image.pixels[i] != band_only[i] {
            paint[i] = image.pixels[i];
        }
    }
    Ok((image.pixels, paint))
}

fn encode_sprite(frame: &lastcall_core::imageutil::SpriteFrame) -> Result<Vec<u8>> {
    imageutil::encode_rgba(&RgbaImage {
        w: frame.w,
        h: frame.h,
        rgba: frame.rgba.clone(),
    })
}

fn hero_from_parts(parts: &lastcall_core::pak::PackageParts) -> Result<Hero> {
    let actor = parts
        .actors
        .iter()
        .find(|actor| actor.def.id == parts.game.hero)
        .ok_or_else(|| Error::msg(format!("hero actor {} is missing", parts.game.hero)))?;
    let frame_def = actor
        .def
        .costumes
        .get("default")
        .and_then(|costume| costume.animations.get("stand_s"))
        .and_then(|anim| anim.frames.first())
        .ok_or_else(|| Error::msg("hero costume default needs a stand_s animation"))?;
    let frame = imageutil::crop(
        &actor.atlas,
        frame_def.x,
        frame_def.y,
        frame_def.w,
        frame_def.h,
        frame_def.ax,
        frame_def.ay,
    )?;
    let png = encode_sprite(&frame)?;
    Ok(Hero { frame, png })
}

fn strip_wrapper(files: BTreeMap<String, Vec<u8>>) -> BTreeMap<String, Vec<u8>> {
    let rooted = files.contains_key("game.json")
        || files.contains_key("manifest.json")
        || files.keys().any(|path| path.starts_with("rooms/"));
    if rooted {
        return files;
    }
    let Some(first) = files.keys().next() else {
        return files;
    };
    let Some(prefix) = first.split('/').next() else {
        return files;
    };
    if prefix.is_empty() {
        return files;
    }
    let root = format!("{prefix}/");
    if !files.keys().all(|path| path.starts_with(&root)) {
        return files;
    }
    files
        .into_iter()
        .map(|(path, bytes)| (path[root.len()..].to_string(), bytes))
        .collect()
}

fn slurp(tree: &dyn ReadTree) -> Result<BTreeMap<String, Vec<u8>>> {
    let mut out = BTreeMap::new();
    fn walk(tree: &dyn ReadTree, prefix: &str, out: &mut BTreeMap<String, Vec<u8>>) -> Result<()> {
        for name in tree.list(prefix)? {
            let path = join_rel(prefix, &name);
            if tree.is_dir(&path) {
                walk(tree, &path, out)?;
            } else if tree.is_file(&path) {
                out.insert(path.clone(), tree.read(&path)?);
            }
        }
        Ok(())
    }
    walk(tree, "", &mut out)?;
    Ok(out)
}

fn simplify_path(path: &[(i32, i32)]) -> Vec<[i32; 2]> {
    if path.len() <= 2 {
        return path.iter().map(|p| [p.0, p.1]).collect();
    }
    let mut out = Vec::new();
    for i in 0..path.len() {
        if i == 0 || i + 1 == path.len() {
            out.push([path[i].0, path[i].1]);
            continue;
        }
        let (x0, y0) = path[i - 1];
        let (x1, y1) = path[i];
        let (x2, y2) = path[i + 1];
        let colinear = (x1 - x0) * (y2 - y1) == (y1 - y0) * (x2 - x1);
        if !colinear {
            out.push([x1, y1]);
        }
    }
    out
}

fn trace_polygons(mask: &[u8]) -> Option<Vec<Polygon>> {
    if mask.len() != PIXELS {
        return None;
    }
    let loops = boundary_loops(mask)?;
    if loops.is_empty() {
        return if mask.iter().all(|p| *p == 0) {
            Some(Vec::new())
        } else {
            None
        };
    }
    let polys = nest_loops(loops)?;
    let raster = geom::rasterize(SCREEN_W as i32, SCREEN_H as i32, &polys);
    if raster == mask {
        Some(polys)
    } else {
        None
    }
}

fn boundary_loops(mask: &[u8]) -> Option<Vec<Vec<[i32; 2]>>> {
    let w = SCREEN_W as i32;
    let h = SCREEN_H as i32;
    let filled = |x: i32, y: i32| -> bool {
        x >= 0 && y >= 0 && x < w && y < h && mask[(y * w + x) as usize] != 0
    };
    let mut edges: HashMap<(i32, i32), Vec<(i32, i32)>> = HashMap::new();
    let mut add = |a: (i32, i32), b: (i32, i32)| {
        edges.entry(a).or_default().push(b);
    };
    for y in 0..h {
        for x in 0..w {
            if !filled(x, y) {
                continue;
            }
            if !filled(x, y - 1) {
                add((x, y), (x + 1, y));
            }
            if !filled(x + 1, y) {
                add((x + 1, y), (x + 1, y + 1));
            }
            if !filled(x, y + 1) {
                add((x + 1, y + 1), (x, y + 1));
            }
            if !filled(x - 1, y) {
                add((x, y + 1), (x, y));
            }
        }
    }
    let mut loops = Vec::new();
    while let Some(start) = edges.keys().next().copied() {
        let mut cur = start;
        let mut pts = vec![[start.0, start.1]];
        for _ in 0..PIXELS * 4 {
            let outs = edges.get_mut(&cur)?;
            if outs.is_empty() {
                return None;
            }
            let next = outs.pop().unwrap();
            if outs.is_empty() {
                edges.remove(&cur);
            }
            if next == start {
                break;
            }
            pts.push([next.0, next.1]);
            cur = next;
        }
        if pts.len() >= 3 {
            loops.push(simplify_loop(pts));
        }
    }
    if edges.values().any(|outs| !outs.is_empty()) {
        return None;
    }
    Some(loops)
}

fn simplify_loop(pts: Vec<[i32; 2]>) -> Vec<[i32; 2]> {
    if pts.len() < 3 {
        return pts;
    }
    let n = pts.len();
    let mut out = Vec::new();
    for i in 0..n {
        let prev = pts[(i + n - 1) % n];
        let cur = pts[i];
        let next = pts[(i + 1) % n];
        let dx1 = cur[0] - prev[0];
        let dy1 = cur[1] - prev[1];
        let dx2 = next[0] - cur[0];
        let dy2 = next[1] - cur[1];
        if dx1 * dy2 != dy1 * dx2 {
            out.push(cur);
        }
    }
    out
}

fn nest_loops(loops: Vec<Vec<[i32; 2]>>) -> Option<Vec<Polygon>> {
    let mut depth = vec![0usize; loops.len()];
    for (i, ring) in loops.iter().enumerate() {
        let sample = right_sample(ring)?;
        for (j, other) in loops.iter().enumerate() {
            if i != j && point_in_poly(sample, other) {
                depth[i] += 1;
            }
        }
    }
    let mut polygons: Vec<Polygon> = Vec::new();
    let mut outer_index: Vec<Option<usize>> = vec![None; loops.len()];
    for (i, ring) in loops.iter().enumerate() {
        if depth[i].is_multiple_of(2) {
            outer_index[i] = Some(polygons.len());
            polygons.push(Polygon {
                points: ring.clone(),
                holes: Vec::new(),
            });
        }
    }
    for (i, ring) in loops.iter().enumerate() {
        if depth[i].is_multiple_of(2) {
            continue;
        }
        let sample = left_sample(ring).or_else(|| right_sample(ring))?;
        let mut host: Option<usize> = None;
        for (j, other) in loops.iter().enumerate() {
            if !depth[j].is_multiple_of(2) || i == j {
                continue;
            }
            if point_in_poly(sample, other)
                && (host.is_none() || depth[j] > depth[host.unwrap_or(j)])
            {
                // Prefer the deepest outer that contains the hole.
                if host.is_none() || depth[j] >= depth[host.unwrap()] {
                    host = Some(j);
                }
            }
        }
        let host = host?;
        let index = outer_index[host]?;
        polygons[index].holes.push(ring.clone());
    }
    Some(polygons)
}

fn right_sample(ring: &[[i32; 2]]) -> Option<(f32, f32)> {
    let a = ring.first().copied()?;
    let b = ring.get(1).copied()?;
    let (x, y) = right_pixel(a, b)?;
    Some((x as f32 + 0.5, y as f32 + 0.5))
}

fn left_sample(ring: &[[i32; 2]]) -> Option<(f32, f32)> {
    let a = ring.first().copied()?;
    let b = ring.get(1).copied()?;
    let (x, y) = left_pixel(a, b)?;
    Some((x as f32 + 0.5, y as f32 + 0.5))
}

fn right_pixel(a: [i32; 2], b: [i32; 2]) -> Option<(i32, i32)> {
    let dx = b[0] - a[0];
    let dy = b[1] - a[1];
    Some(match (dx.signum(), dy.signum()) {
        (1, 0) => (a[0], a[1]),
        (0, 1) => (a[0] - 1, a[1]),
        (-1, 0) => (a[0] - 1, a[1] - 1),
        (0, -1) => (a[0], a[1] - 1),
        _ => return None,
    })
}

fn left_pixel(a: [i32; 2], b: [i32; 2]) -> Option<(i32, i32)> {
    let (x, y) = right_pixel(a, b)?;
    let dx = b[0] - a[0];
    let dy = b[1] - a[1];
    Some(match (dx.signum(), dy.signum()) {
        (1, 0) => (x, y - 1),
        (0, 1) => (x + 1, y),
        (-1, 0) => (x, y + 1),
        (0, -1) => (x - 1, y),
        _ => return None,
    })
}

fn point_in_poly(point: (f32, f32), poly: &[[i32; 2]]) -> bool {
    let mut inside = false;
    let (x, y) = point;
    let n = poly.len();
    if n < 3 {
        return false;
    }
    let mut j = n - 1;
    for i in 0..n {
        let xi = poly[i][0] as f32;
        let yi = poly[i][1] as f32;
        let xj = poly[j][0] as f32;
        let yj = poly[j][1] as f32;
        let intersect = ((yi > y) != (yj > y)) && (x < (xj - xi) * (y - yi) / (yj - yi + 0.0) + xi);
        if intersect {
            inside = !inside;
        }
        j = i;
    }
    inside
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::validate_dir;
    use lastcall_core::pak::build_pak;
    use std::path::PathBuf;

    fn repo(rel: &str) -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../..")
            .join(rel)
    }

    fn roundtrip_walk(path: &str) {
        let bytes = std::fs::read(repo(path)).unwrap();
        let walk: WalkDef = serde_json::from_slice(&bytes).unwrap();
        let mask = geom::rasterize(SCREEN_W as i32, SCREEN_H as i32, &walk.polygons);
        let traced = trace_polygons(&mask).expect(path);
        let again = geom::rasterize(SCREEN_W as i32, SCREEN_H as i32, &traced);
        assert_eq!(again, mask, "{path}");
    }

    #[test]
    fn trace_sample_and_charlie_bar() {
        roundtrip_walk("fixtures/sample/rooms/porch/walk.json");
        roundtrip_walk("fixtures/sample/rooms/yard/walk.json");
        roundtrip_walk("fixtures/charlie/rooms/bar/walk.json");
        roundtrip_walk("fixtures/charlie/rooms/alley/walk.json");
    }

    #[test]
    fn sample_source_roundtrip_is_byte_identical() {
        let session = Session::open_dir(&repo("fixtures/sample")).unwrap();
        assert!(!session.dirty());
        let exported = session.export_files().unwrap();
        assert_eq!(exported, session.files);
    }

    #[test]
    fn sample_pak_roundtrip_is_byte_identical() {
        let parts = validate_dir(&repo("fixtures/sample")).unwrap();
        let bytes = build_pak(&parts).unwrap();
        let session = Session::open_bytes(&bytes).unwrap();
        assert!(session.is_pak());
        assert!(!session.dirty());
        assert_eq!(session.export_pak().unwrap(), bytes);
    }

    #[test]
    fn edited_walk_changes_the_path_and_still_validates() {
        let mut session = Session::open_dir(&repo("fixtures/sample")).unwrap();
        let mut doc: RoomDoc = serde_json::from_str(&session.room_json("porch").unwrap()).unwrap();
        let before = session.plan_path("porch", 40, 190, 300, 190);
        assert!(before.len() >= 2);
        doc.polygons[0].points[0][1] = 165;
        doc.polygons[0].points[1][1] = 165;
        session
            .set_room_json("porch", &serde_json::to_string(&doc).unwrap())
            .unwrap();
        assert!(session.dirty());
        let files = session.export_files().unwrap();
        let walk = std::fs::read(repo("fixtures/sample/rooms/porch/walk.json")).unwrap();
        assert_ne!(files["rooms/porch/walk.json"], walk);
        assert_eq!(
            files["rooms/porch/room.json"],
            session.files["rooms/porch/room.json"]
        );
        let blocked = session.plan_path("porch", 40, 155, 300, 155);
        assert!(blocked.is_empty());
    }

    #[test]
    fn charlie_source_roundtrip_is_byte_identical() {
        let session = Session::open_dir(&repo("fixtures/charlie")).unwrap();
        assert_eq!(session.game_id(), "charlie");
        assert!(session.rooms.contains_key("bar"));
        let exported = session.export_files().unwrap();
        assert_eq!(exported.len(), session.files.len());
        for (path, bytes) in &session.files {
            assert_eq!(exported.get(path).unwrap(), bytes, "{path}");
        }
    }
}
