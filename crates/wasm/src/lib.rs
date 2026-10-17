use lastcall_core::{Engine as CoreEngine, Input};
use wasm_bindgen::prelude::*;

#[wasm_bindgen]
pub struct Engine {
    inner: CoreEngine,
}

#[wasm_bindgen]
impl Engine {
    pub fn load(bytes: &[u8]) -> Result<Engine, JsValue> {
        CoreEngine::load(bytes)
            .map(|inner| Engine { inner })
            .map_err(|err| JsValue::from_str(&err.to_string()))
    }

    pub fn input(&mut self, event_json: &str) -> Result<(), JsValue> {
        let event: Input =
            serde_json::from_str(event_json).map_err(|err| JsValue::from_str(&err.to_string()))?;
        self.inner.input(event);
        Ok(())
    }

    pub fn step(&mut self, ticks: u32) {
        self.inner.step(ticks);
    }

    pub fn framebuffer(&self) -> *const u8 {
        self.inner.framebuffer().as_ptr()
    }

    pub fn framebuffer_len(&self) -> u32 {
        self.inner.framebuffer().len() as u32
    }

    pub fn width(&self) -> u32 {
        self.inner.width()
    }

    pub fn height(&self) -> u32 {
        self.inner.height()
    }

    pub fn frame_hash(&self) -> String {
        self.inner.frame_hash()
    }

    pub fn drain_events(&mut self) -> String {
        let events = self.inner.drain_events();
        serde_json::to_string(&events).unwrap_or_else(|_| "[]".into())
    }

    pub fn save(&self) -> Result<Vec<u8>, JsValue> {
        self.inner
            .save()
            .map_err(|err| JsValue::from_str(&err.to_string()))
    }

    pub fn restore(&mut self, bytes: &[u8]) -> Result<(), JsValue> {
        self.inner
            .restore(bytes)
            .map_err(|err| JsValue::from_str(&err.to_string()))
    }
}
