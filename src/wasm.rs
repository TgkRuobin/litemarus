//! wasm-bindgen entry points for Node / browser.

use std::collections::BTreeMap;

use js_sys::{Array, Object, Reflect, JSON};
use wasm_bindgen::prelude::*;

use crate::minecraft::BlockState;
use crate::region::Region;
use crate::schematic::{SaveMeta, Schematic};

#[wasm_bindgen(start)]
pub fn wasm_init() {
    // 不安装 panic hook：release 使用 panic = "abort"，panic 直接 trap，
    // 避免把内部错误信息/源码路径泄漏到浏览器控制台。
}

#[wasm_bindgen(js_name = litemarusVersion)]
pub fn wasm_litemarus_version() -> String {
    format!(
        "{} {}",
        crate::constants::LITEMAPY_NAME,
        crate::constants::LITEMAPY_VERSION
    )
}

/// Build a tiny valid gzip-NBT `.litematic` (3×3×3 region, center stone).
#[wasm_bindgen(js_name = buildDemoLitematic)]
pub fn wasm_build_demo_litematic() -> Result<Vec<u8>, JsError> {
    let mut reg = Region::new(0, 0, 0, 3, 3, 3).map_err(to_js)?;
    let stone = BlockState::new("minecraft:stone").map_err(to_js)?;
    reg.set_block_at(1, 1, 1, stone).map_err(to_js)?;
    let mut sch = Schematic::new(
        Some("wasm-demo"),
        Some("LitemaRust"),
        Some("Test projection from WASM"),
        crate::constants::MC_DATA_VERSION,
    );
    sch.regions_mut().insert(String::from("main"), reg);
    sch.save_bytes(SaveMeta::default()).map_err(to_js)
}

/// Parse gzip-NBT `.litematic` bytes and return metadata JSON (no full block payload).
///
/// Fields: `name`, `author`, `description`, `width`, `height`, `length`,
/// `lmVersion`, `lmSubversion`, `mcVersion`, `created`, `modified`, `regionNames`.
#[wasm_bindgen(js_name = decodeLitematicMetadata)]
pub fn wasm_decode_litematic_metadata(bytes: &[u8]) -> Result<String, JsError> {
    let sch = Schematic::from_bytes(bytes).map_err(to_js)?;
    Ok(schematic_metadata_json(&sch))
}

/// One Litematica region for WASM callers (create → edit blocks → attach to [`WasmSchematic`]).
#[wasm_bindgen]
pub struct WasmRegion {
    inner: Region,
}

#[wasm_bindgen]
impl WasmRegion {
    /// Same semantics as Python `Region(x, y, z, width, height, length)`.
    #[wasm_bindgen(constructor)]
    pub fn new(
        x: i32,
        y: i32,
        z: i32,
        width: i32,
        height: i32,
        length: i32,
    ) -> Result<WasmRegion, JsError> {
        Ok(WasmRegion {
            inner: Region::new(x, y, z, width, height, length).map_err(to_js)?,
        })
    }

    /// Set a block by resource id (no block state properties). Invalid ids return `JsError`.
    pub fn set_block_id(&mut self, x: i32, y: i32, z: i32, block_id: &str) -> Result<(), JsError> {
        let b = BlockState::new(block_id).map_err(to_js)?;
        self.inner.set_block_at(x, y, z, b).map_err(to_js)
    }

    /// Set a block with optional state properties (`null` / `undefined` = none).
    ///
    /// `properties_json` may be a plain object (`{ facing: "north", delay: "1" }`), a JSON string,
    /// or omitted. Property values are coerced to strings (booleans → `"true"` / `"false"`).
    pub fn set_block_state(
        &mut self,
        x: i32,
        y: i32,
        z: i32,
        block_id: &str,
        properties_json: Option<JsValue>,
    ) -> Result<(), JsError> {
        let props = properties_from_js(properties_json)?;
        let b = if props.is_empty() {
            BlockState::new(block_id).map_err(to_js)?
        } else {
            BlockState::with_properties(block_id, props).map_err(to_js)?
        };
        self.inner.set_block_at(x, y, z, b).map_err(to_js)
    }

    /// Bulk block write: one call instead of one per block.
    ///
    /// - `positions`: flat `[x0, y0, z0, x1, y1, z1, ...]` (length `3 * n`).
    /// - `palette_indices`: palette slot per block (length `n`).
    /// - `states`: distinct block states, each a resource id string or
    ///   `{ id, properties }`; slot `i` refers to `states[i]`.
    ///
    /// Keeping the palette separate means the (potentially huge) per-block payload
    /// is just two typed arrays, and each state is parsed once instead of per block.
    #[wasm_bindgen(js_name = applyBlocks)]
    pub fn apply_blocks(
        &mut self,
        positions: &[i32],
        palette_indices: &[u32],
        states: Array,
    ) -> Result<(), JsError> {
        if positions.len() != palette_indices.len().saturating_mul(3) {
            return Err(JsError::new(
                "positions must hold 3 coordinates per palette index",
            ));
        }

        // Intern every distinct state once.
        let mut slots: Vec<u32> = Vec::with_capacity(states.length() as usize);
        for state in states.iter() {
            let block = block_state_from_js(&state)?;
            slots.push(self.inner.intern_block_state(block).map_err(to_js)?);
        }

        for (i, &palette_index) in palette_indices.iter().enumerate() {
            let slot = slots.get(palette_index as usize).ok_or_else(|| {
                JsError::new(&format!(
                    "palette index {palette_index} is out of range (states holds {})",
                    slots.len()
                ))
            })?;
            let base = i * 3;
            self.inner
                .set_palette_index_at(
                    positions[base],
                    positions[base + 1],
                    positions[base + 2],
                    *slot,
                )
                .map_err(to_js)?;
        }
        Ok(())
    }

    pub fn position_x(&self) -> i32 {
        self.inner.x()
    }

    pub fn position_y(&self) -> i32 {
        self.inner.y()
    }

    pub fn position_z(&self) -> i32 {
        self.inner.z()
    }

    pub fn size_width(&self) -> i32 {
        self.inner.width()
    }

    pub fn size_height(&self) -> i32 {
        self.inner.height()
    }

    pub fn size_length(&self) -> i32 {
        self.inner.length()
    }

    pub fn volume(&self) -> u32 {
        self.inner.volume() as u32
    }

    pub fn count_blocks(&self) -> u32 {
        self.inner.count_blocks() as u32
    }

    /// Block resource id at region-local coordinates.
    pub fn get_block_id(&self, x: i32, y: i32, z: i32) -> String {
        self.inner.get_block_at(x, y, z).id().to_string()
    }

    /// Block state at region-local coordinates: `{ id, properties }`.
    pub fn get_block_state(&self, x: i32, y: i32, z: i32) -> Object {
        block_state_to_js(self.inner.get_block_at(x, y, z))
    }

    /// All blocks as JSON array: `[{ x, y, z, id, properties }, ...]`.
    ///
    /// Coordinates are region-local (same as `set_block_state`). Set `skip_air` to omit `minecraft:air`.
    #[wasm_bindgen(js_name = collectBlocksJson)]
    pub fn collect_blocks_json(&self, skip_air: bool) -> String {
        region_blocks_json(&self.inner, skip_air)
    }
}

/// Litematica schematic with metadata + regions, serializable to gzip-NBT bytes.
#[wasm_bindgen]
pub struct WasmSchematic {
    inner: Schematic,
}

#[wasm_bindgen]
impl WasmSchematic {
    #[wasm_bindgen(constructor)]
    pub fn new(name: &str, author: &str, description: &str, mc_data_version: i32) -> WasmSchematic {
        WasmSchematic {
            inner: Schematic::new(Some(name), Some(author), Some(description), mc_data_version),
        }
    }

    /// Load a schematic from gzip-NBT `.litematic` bytes.
    #[wasm_bindgen(js_name = fromBytes)]
    pub fn from_bytes(bytes: &[u8]) -> Result<WasmSchematic, JsError> {
        Ok(WasmSchematic {
            inner: Schematic::from_bytes(bytes).map_err(to_js)?,
        })
    }

    pub fn name(&self) -> String {
        self.inner.name().to_string()
    }

    pub fn author(&self) -> String {
        self.inner.author().to_string()
    }

    pub fn description(&self) -> String {
        self.inner.description().to_string()
    }

    pub fn width(&self) -> i32 {
        self.inner.width()
    }

    pub fn height(&self) -> i32 {
        self.inner.height()
    }

    pub fn length(&self) -> i32 {
        self.inner.length()
    }

    pub fn lm_version(&self) -> i32 {
        self.inner.lm_version()
    }

    pub fn lm_subversion(&self) -> i32 {
        self.inner.lm_subversion()
    }

    pub fn mc_version(&self) -> i32 {
        self.inner.mc_version()
    }

    pub fn created(&self) -> i64 {
        self.inner.created()
    }

    pub fn modified(&self) -> i64 {
        self.inner.modified()
    }

    /// Region keys in file order.
    #[wasm_bindgen(js_name = regionNames)]
    pub fn region_names(&self) -> Array {
        let arr = Array::new();
        for key in self.inner.regions().keys() {
            arr.push(&JsValue::from_str(key));
        }
        arr
    }

    /// Clone a named region for reading or further edits.
    #[wasm_bindgen(js_name = getRegion)]
    pub fn get_region(&self, key: &str) -> Result<WasmRegion, JsError> {
        let region = self
            .inner
            .regions()
            .get(key)
            .ok_or_else(|| JsError::new(&format!("region \"{key}\" not found")))?;
        Ok(WasmRegion {
            inner: region.clone(),
        })
    }

    pub fn set_name(&mut self, name: &str) {
        self.inner.set_name(name.into());
    }

    pub fn set_author(&mut self, author: &str) {
        self.inner.set_author(author.into());
    }

    pub fn set_description(&mut self, description: &str) {
        self.inner.set_description(description.into());
    }

    /// Litematic format major version (Python default `LITEMATIC_VERSION` = 6).
    pub fn set_lm_version(&mut self, v: i32) {
        self.inner.set_lm_version(v);
    }

    pub fn set_lm_subversion(&mut self, v: i32) {
        self.inner.set_lm_subversion(v);
    }

    /// Minecraft `DataVersion` written to the file.
    pub fn set_mc_version(&mut self, v: i32) {
        self.inner.set_mc_version(v);
    }

    /// Insert or replace a named region (consumes the [`WasmRegion`]).
    pub fn insert_region(&mut self, key: &str, region: WasmRegion) {
        self.inner.regions_mut().insert(key.into(), region.inner);
    }

    /// Gzip-compressed `.litematic` bytes (updates modified time when `SaveMeta` default).
    pub fn save_bytes(&mut self) -> Result<Vec<u8>, JsError> {
        self.inner.save_bytes(SaveMeta::default()).map_err(to_js)
    }
}

fn to_js(e: crate::Error) -> JsError {
    JsError::new(&e.to_string())
}

fn escape_json_str(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for ch in s.chars() {
        match ch {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if c.is_control() => {
                out.push_str(&format!("\\u{:04x}", c as u32));
            }
            c => out.push(c),
        }
    }
    out
}

fn properties_to_json(block: &BlockState) -> String {
    if block.is_empty() {
        return "{}".into();
    }
    let mut out = String::from("{");
    let mut first = true;
    for (k, v) in block.properties_iter() {
        if !first {
            out.push(',');
        }
        first = false;
        out.push_str(&format!(
            "\"{}\":\"{}\"",
            escape_json_str(k),
            escape_json_str(v)
        ));
    }
    out.push('}');
    out
}

fn block_state_to_js(block: &BlockState) -> Object {
    let obj = Object::new();
    let _ = Reflect::set(
        &obj,
        &JsValue::from_str("id"),
        &JsValue::from_str(block.id()),
    );
    let props = Object::new();
    for (k, v) in block.properties_iter() {
        let _ = Reflect::set(&props, &JsValue::from_str(k), &JsValue::from_str(v));
    }
    let _ = Reflect::set(&obj, &JsValue::from_str("properties"), &props.into());
    obj
}

fn schematic_metadata_json(sch: &Schematic) -> String {
    let region_names: Vec<String> = sch.regions().keys().cloned().collect();
    let names_json: String = region_names
        .iter()
        .map(|n| format!("\"{}\"", escape_json_str(n)))
        .collect::<Vec<_>>()
        .join(",");
    format!(
        concat!(
            "{{\"name\":\"{name}\",\"author\":\"{author}\",\"description\":\"{description}\",",
            "\"width\":{width},\"height\":{height},\"length\":{length},",
            "\"lmVersion\":{lm_version},\"lmSubversion\":{lm_subversion},\"mcVersion\":{mc_version},",
            "\"created\":{created},\"modified\":{modified},\"regionNames\":[{region_names}]}}"
        ),
        name = escape_json_str(sch.name()),
        author = escape_json_str(sch.author()),
        description = escape_json_str(sch.description()),
        width = sch.width(),
        height = sch.height(),
        length = sch.length(),
        lm_version = sch.lm_version(),
        lm_subversion = sch.lm_subversion(),
        mc_version = sch.mc_version(),
        created = sch.created(),
        modified = sch.modified(),
        region_names = names_json,
    )
}

fn region_blocks_json(region: &Region, skip_air: bool) -> String {
    let mut out = String::from("[");
    let mut first = true;
    let mut push = |x: i32, y: i32, z: i32, block: &BlockState| {
        if !first {
            out.push(',');
        }
        first = false;
        out.push_str(&format!(
            "{{\"x\":{},\"y\":{},\"z\":{},\"id\":\"{}\",\"properties\":{}}}",
            x,
            y,
            z,
            escape_json_str(block.id()),
            properties_to_json(block)
        ));
    };
    if skip_air {
        // Sparse regions only store placed blocks, so this skips the empty box.
        region.for_each_set_block(|x, y, z, block| push(x, y, z, block));
    } else {
        for (x, y, z) in region.block_positions() {
            let block = region.get_block_at(x, y, z);
            push(x, y, z, block);
        }
    }
    out.push(']');
    out
}

fn properties_from_js(value: Option<JsValue>) -> Result<BTreeMap<String, String>, JsError> {
    let Some(v) = value else {
        return Ok(BTreeMap::new());
    };
    if v.is_undefined() || v.is_null() {
        return Ok(BTreeMap::new());
    }

    let obj = if v.is_string() {
        let s = v
            .as_string()
            .ok_or_else(|| JsError::new("propertiesJson: expected string"))?;
        if s.is_empty() {
            return Ok(BTreeMap::new());
        }
        JSON::parse(&s).map_err(|_| JsError::new("propertiesJson: invalid JSON"))?
    } else {
        v
    };

    if !obj.is_object() || obj.is_null() {
        return Err(JsError::new(
            "propertiesJson must be a JSON object, object literal, or JSON string",
        ));
    }

    let obj = Object::from(obj);
    let keys = Object::keys(&obj);
    let mut map = BTreeMap::new();
    for i in 0..keys.length() {
        let key = keys
            .get(i)
            .as_string()
            .ok_or_else(|| JsError::new("propertiesJson: invalid property key"))?;
        let val = Reflect::get(&obj, &JsValue::from_str(&key))
            .map_err(|_| JsError::new(&format!("propertiesJson: failed to read \"{key}\"")))?;
        let val_str = js_value_to_property_string(&key, &val)?;
        map.insert(key, val_str);
    }
    Ok(map)
}

fn js_value_to_property_string(key: &str, val: &JsValue) -> Result<String, JsError> {
    if let Some(s) = val.as_string() {
        return Ok(s);
    }
    if let Some(b) = val.as_bool() {
        return Ok(if b { "true" } else { "false" }.into());
    }
    if let Some(n) = val.as_f64() {
        return Ok(n.to_string());
    }
    Err(JsError::new(&format!(
        "property \"{key}\" must be a string, boolean, or number"
    )))
}

/// Parses a block state spec: either a resource id string or an object with
/// `id` / `name` and optional `properties`.
fn block_state_from_js(value: &JsValue) -> Result<BlockState, JsError> {
    if let Some(id) = value.as_string() {
        return BlockState::new(&id).map_err(to_js);
    }
    if !value.is_object() {
        return Err(JsError::new(
            "block state must be a resource id string or { id | name, properties }",
        ));
    }
    let id = ["id", "name"]
        .iter()
        .find_map(|field| {
            Reflect::get(value, &JsValue::from_str(field))
                .ok()
                .and_then(|v| v.as_string())
        })
        .ok_or_else(|| JsError::new("block state is missing a string `id` or `name`"))?;
    let properties = Reflect::get(value, &JsValue::from_str("properties")).unwrap_or(JsValue::UNDEFINED);
    let properties = properties_from_js(Some(properties))?;
    if properties.is_empty() {
        BlockState::new(&id).map_err(to_js)
    } else {
        BlockState::with_properties(&id, properties).map_err(to_js)
    }
}
