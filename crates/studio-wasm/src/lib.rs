//! Browser bindings for the Studio session. Parsing stays in `advc`.

use std::collections::BTreeMap;

use advc::Session;
use wasm_bindgen::prelude::*;

fn err(error: impl ToString) -> JsValue {
    JsValue::from_str(&error.to_string())
}

#[wasm_bindgen]
pub struct Studio {
    inner: Session,
}

#[wasm_bindgen]
pub struct Export {
    files: BTreeMap<String, Vec<u8>>,
    deleted: Vec<String>,
}

/// Loose files from a directory picker. `finish` opens them as a project.
#[wasm_bindgen]
pub struct ProjectFiles {
    files: BTreeMap<String, Vec<u8>>,
}

impl Default for ProjectFiles {
    fn default() -> Self {
        Self::new()
    }
}

#[wasm_bindgen]
impl ProjectFiles {
    #[wasm_bindgen(constructor)]
    pub fn new() -> Self {
        Self {
            files: BTreeMap::new(),
        }
    }

    pub fn add(&mut self, path: &str, bytes: &[u8]) {
        let path = path.replace('\\', "/");
        let path = path.trim_start_matches("./").trim_start_matches('/');
        if path.is_empty() || path.split('/').any(|part| part == "..") {
            return;
        }
        self.files.insert(path.to_string(), bytes.to_vec());
    }

    pub fn finish(&mut self) -> Result<Studio, JsValue> {
        let files = std::mem::take(&mut self.files);
        Session::open_loose(files)
            .map(|inner| Studio { inner })
            .map_err(err)
    }
}

#[wasm_bindgen]
impl Studio {
    /// Open a `.advpak` or a zipped authoring folder.
    pub fn open(bytes: &[u8]) -> Result<Studio, JsValue> {
        Session::open_bytes(bytes)
            .map(|inner| Studio { inner })
            .map_err(err)
    }

    pub fn is_pak(&self) -> bool {
        self.inner.is_pak()
    }

    pub fn title(&self) -> String {
        self.inner.title().to_string()
    }

    pub fn game_id(&self) -> String {
        self.inner.game_id().to_string()
    }

    pub fn dirty(&self) -> bool {
        self.inner.dirty()
    }

    pub fn rooms(&self) -> String {
        self.inner.rooms_json()
    }

    pub fn room(&self, id: &str) -> Result<String, JsValue> {
        self.inner.room_json(id).map_err(err)
    }

    pub fn picture(&self, id: &str) -> Result<Vec<u8>, JsValue> {
        self.inner.picture_png(id).map_err(err)
    }

    pub fn mask(&self, id: &str) -> Result<Vec<u8>, JsValue> {
        self.inner.mask(id).map_err(err)
    }

    pub fn priority(&self, id: &str) -> Result<Vec<u8>, JsValue> {
        self.inner.priority_map(id).map_err(err)
    }

    pub fn set_room(&mut self, id: &str, json: &str) -> Result<(), JsValue> {
        self.inner.set_room_json(id, json).map_err(err)
    }

    pub fn plan_path(&self, id: &str, x0: i32, y0: i32, x1: i32, y1: i32) -> String {
        serde_json::to_string(&self.inner.plan_path(id, x0, y0, x1, y1))
            .unwrap_or_else(|_| "[]".into())
    }

    pub fn hero_png(&self) -> Vec<u8> {
        self.inner.hero_png().to_vec()
    }

    pub fn hero_meta(&self) -> String {
        self.inner.hero_meta_json()
    }

    /// Full-screen RGBA. Transparent pixels are empty; priority occludes the hero.
    pub fn hero_overlay(&self, id: &str, x: i32, y: i32) -> Result<Vec<u8>, JsValue> {
        self.inner
            .hero_overlay(id, x, y)
            .map(|overlay| overlay.rgba)
            .map_err(err)
    }

    pub fn export_pak(&self) -> Result<Vec<u8>, JsValue> {
        self.inner.export_pak().map_err(err)
    }

    pub fn export_zip(&self) -> Result<Vec<u8>, JsValue> {
        self.inner.export_project_zip().map_err(err)
    }

    pub fn export_changes(&self) -> Result<Export, JsValue> {
        let dump = self.inner.export_changes().map_err(err)?;
        Ok(Export {
            files: dump.files,
            deleted: dump.deleted,
        })
    }
}

#[wasm_bindgen]
impl Export {
    pub fn paths(&self) -> String {
        let list: Vec<String> = self.files.keys().cloned().collect();
        serde_json::to_string(&list).unwrap_or_else(|_| "[]".into())
    }

    pub fn deleted(&self) -> String {
        serde_json::to_string(&self.deleted).unwrap_or_else(|_| "[]".into())
    }

    pub fn bytes(&self, path: &str) -> Vec<u8> {
        self.files.get(path).cloned().unwrap_or_default()
    }
}
