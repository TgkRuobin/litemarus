//! Block states, entities, tile entities — matches `litemapy.minecraft`.

use std::collections::BTreeMap;
use std::fmt;

use valence_nbt::{Compound, List, Value};

use crate::error::{RequiredKeyMissingException, Result};
use crate::nbt_helpers::{expect_i32, required};

pub fn is_valid_identifier(identifier: &str) -> bool {
    let mut allowed_chars = "_-abcdefghijklmnopqrstuvwxyz0123456789.:";
    let mut separator = false;
    for ch in identifier.chars() {
        if !allowed_chars.contains(ch) {
            return false;
        }
        if ch == ':' {
            separator = true;
            allowed_chars = "_-abcdefghijklmnopqrstuvwxyz0123456789./";
        }
    }
    separator
}

pub fn assert_valid_identifier(identifier: &str) -> Result<&str> {
    if !is_valid_identifier(identifier) {
        return Err(crate::error::Error::Corrupted(format!(
            "Invalid identifier \"{}\"",
            identifier
        )));
    }
    Ok(identifier)
}

#[derive(Clone, Debug, Eq, PartialEq, Hash)]
pub struct BlockState {
    block_id: String,
    properties: BTreeMap<String, String>,
}

impl BlockState {
    pub fn new(block_id: &str) -> Result<Self> {
        assert_valid_identifier(block_id)?;
        Ok(Self {
            block_id: block_id.to_string(),
            properties: BTreeMap::new(),
        })
    }

    pub fn with_properties<K: Into<String>, V: Into<String>>(
        block_id: &str,
        props: impl IntoIterator<Item = (K, V)>,
    ) -> Result<Self> {
        assert_valid_identifier(block_id)?;
        let mut properties = BTreeMap::new();
        for (k, v) in props {
            properties.insert(k.into(), v.into());
        }
        Ok(Self {
            block_id: block_id.to_string(),
            properties,
        })
    }

    pub fn air() -> Self {
        Self::new("minecraft:air").expect("minecraft:air is valid")
    }

    pub fn id(&self) -> &str {
        &self.block_id
    }

    pub fn with_id(&self, block_id: &str) -> Result<Self> {
        assert_valid_identifier(block_id)?;
        Ok(Self {
            block_id: block_id.to_string(),
            properties: self.properties.clone(),
        })
    }

    pub fn with_properties_updates(
        &self,
        updates: &[(&str, Option<&str>)],
    ) -> Result<Self> {
        let mut other = Self {
            block_id: self.block_id.clone(),
            properties: self.properties.clone(),
        };
        let mut removals: Vec<&str> = Vec::new();
        let mut adds: Vec<(&str, &str)> = Vec::new();
        for &(name, oval) in updates {
            match oval {
                None => removals.push(name),
                Some(val) => adds.push((name, val)),
            }
        }
        for n in removals {
            other.properties.remove(n);
        }
        for (k, v) in adds {
            other.properties.insert(k.into(), v.into());
        }
        Ok(other)
    }

    pub fn properties_iter(&self) -> impl Iterator<Item = (&String, &String)> {
        self.properties.iter()
    }

    pub fn len(&self) -> usize {
        self.properties.len()
    }

    pub fn is_empty(&self) -> bool {
        self.properties.is_empty()
    }

    pub fn get(&self, key: &str) -> Option<&String> {
        self.properties.get(key)
    }

    pub fn contains_key(&self, key: &str) -> bool {
        self.properties.contains_key(key)
    }

    pub fn to_block_state_identifier(&self, skip_empty: bool) -> String {
        if skip_empty && self.properties.is_empty() {
            return self.block_id.clone();
        }
        if self.properties.is_empty() {
            return self.block_id.clone();
        }
        let inner = self
            .properties
            .iter()
            .map(|(k, v)| format!("{}={}", k, v))
            .collect::<Vec<_>>()
            .join(",");
        format!("{}[{}]", self.block_id, inner)
    }

    pub fn to_nbt(&self) -> Compound<String> {
        let mut root = Compound::new();
        root.insert("Name", Value::String(self.block_id.clone()));
        if !self.properties.is_empty() {
            let mut p = Compound::new();
            for (k, v) in self.properties.iter() {
                p.insert(k.clone(), Value::String(v.clone()));
            }
            root.insert("Properties", Value::Compound(p));
        }
        root
    }

    pub fn from_nbt(tag: &Compound<String>) -> Result<Self> {
        let name_val = required(tag, "BlockState.from_nbt", "Name")?;
        let name_s = crate::nbt_helpers::expect_string(name_val, "BlockState.from_nbt:Name")?;
        assert_valid_identifier(&name_s)?;

        let properties = match tag.get("Properties") {
            None => BTreeMap::new(),
            Some(Value::Compound(p)) => {
                let mut m = BTreeMap::new();
                for (k, v) in p.iter() {
                    if let Value::String(s) = v {
                        m.insert(k.clone(), s.clone());
                    }
                }
                m
            }
            _ => BTreeMap::new(),
        };
        Ok(Self {
            block_id: name_s,
            properties,
        })
    }
}

impl fmt::Display for BlockState {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.to_block_state_identifier(true))
    }
}

fn list_double_from_f64(v: [f64; 3]) -> Value<String> {
    Value::List(List::Double(vec![v[0], v[1], v[2]]))
}

#[derive(Clone, Debug)]
pub struct Entity {
    data: Compound<String>,
    id: String,
}

impl Entity {
    pub fn from_id(id: &str) -> Result<Self> {
        assert_valid_identifier(id)?;
        let mut data = Compound::new();
        data.insert("id", Value::String(id.to_string()));
        data.insert("Pos", list_double_from_f64([0.0, 0.0, 0.0]));
        data.insert("Rotation", Value::List(List::Double(vec![0.0, 0.0])));
        data.insert("Motion", list_double_from_f64([0.0, 0.0, 0.0]));
        Ok(Self {
            id: id.to_string(),
            data,
        })
    }

    pub fn from_nbt(nbt: Compound<String>) -> Result<Self> {
        if !nbt.contains_key("id") {
            return Err(crate::error::Error::Corrupted(
                RequiredKeyMissingException::new("id").to_string(),
            ));
        }
        let mut data = nbt;
        let id_src = required(&data, "Entity.from_nbt", "id")?;
        let id_str = crate::nbt_helpers::expect_string(id_src, "Entity.id")?;
        assert_valid_identifier(&id_str)?;
        let id_string = id_str.clone();

        if !data.contains_key("Pos") {
            data.insert("Pos", list_double_from_f64([0.0, 0.0, 0.0]));
        }
        if !data.contains_key("Rotation") {
            data.insert("Rotation", Value::List(List::Double(vec![0.0, 0.0])));
        }
        if !data.contains_key("Motion") {
            data.insert("Motion", list_double_from_f64([0.0, 0.0, 0.0]));
        }

        Ok(Self {
            id: id_string,
            data,
        })
    }

    pub fn to_nbt(&self) -> Compound<String> {
        self.data.clone()
    }

    pub fn data(&self) -> &Compound<String> {
        &self.data
    }

    pub fn data_mut(&mut self) -> &mut Compound<String> {
        &mut self.data
    }

    pub fn add_tag(&mut self, key: String, value: Value<String>) -> Result<()> {
        self.data.insert(key.clone(), value);
        self.sync_from_data_key(&key)?;
        Ok(())
    }

    pub fn get_tag(&self, key: &str) -> Result<&Value<String>> {
        self.data
            .get(key)
            .ok_or_else(|| crate::error::Error::MissingField {
                context: "Entity",
                key: key.into(),
            })
    }

    fn sync_from_data_key(&mut self, key: &str) -> Result<()> {
        match key {
            "id" => {
                if let Some(Value::String(s)) = self.data.get("id") {
                    self.id = s.clone();
                }
            }
            _ => {}
        }
        Ok(())
    }

    pub fn id(&self) -> &str {
        &self.id
    }

    pub fn set_id(&mut self, id: &str) -> Result<()> {
        assert_valid_identifier(id)?;
        self.id = id.to_string();
        self.data.insert("id", Value::String(self.id.clone()));
        Ok(())
    }

    pub fn position(&self) -> [f64; 3] {
        read_pos(self.data.get("Pos").expect("Pos"))
    }

    pub fn set_position(&mut self, pos: [f64; 3]) {
        self.data.insert("Pos", list_double_from_f64(pos));
    }

    pub fn rotation(&self) -> [f64; 2] {
        match self.data.get("Rotation").expect("Rotation") {
            Value::List(List::Double(v)) if v.len() >= 2 => [v[0], v[1]],
            _ => [0.0, 0.0],
        }
    }

    pub fn set_rotation(&mut self, rot: [f64; 2]) {
        self.data
            .insert("Rotation", Value::List(List::Double(vec![rot[0], rot[1]])));
    }

    pub fn motion(&self) -> [f64; 3] {
        read_pos(self.data.get("Motion").expect("Motion"))
    }

    pub fn set_motion(&mut self, m: [f64; 3]) {
        self.data.insert("Motion", list_double_from_f64(m));
    }
}

fn read_pos(v: &Value<String>) -> [f64; 3] {
    match v {
        Value::List(List::Double(p)) if p.len() >= 3 => [p[0], p[1], p[2]],
        _ => [0.0, 0.0, 0.0],
    }
}

#[derive(Clone, Debug)]
pub struct TileEntity {
    data: Compound<String>,
    position: (i32, i32, i32),
}

impl TileEntity {
    pub fn from_nbt(mut nbt: Compound<String>) -> Result<Self> {
        if !nbt.contains_key("x") {
            nbt.insert("x", Value::Int(0));
        }
        if !nbt.contains_key("y") {
            nbt.insert("y", Value::Int(0));
        }
        if !nbt.contains_key("z") {
            nbt.insert("z", Value::Int(0));
        }
        let x = expect_i32(required(&nbt, "TileEntity", "x")?, "tile.x")?;
        let y = expect_i32(required(&nbt, "TileEntity", "y")?, "tile.y")?;
        let z = expect_i32(required(&nbt, "TileEntity", "z")?, "tile.z")?;
        Ok(Self {
            data: nbt,
            position: (x, y, z),
        })
    }

    pub fn to_nbt(&self) -> Compound<String> {
        self.data.clone()
    }

    pub fn data(&self) -> &Compound<String> {
        &self.data
    }

    pub fn data_mut(&mut self) -> &mut Compound<String> {
        &mut self.data
    }

    pub fn position(&self) -> (i32, i32, i32) {
        self.position
    }

    pub fn set_position(&mut self, p: (i32, i32, i32)) {
        self.position = p;
        self.data.insert("x", Value::Int(p.0));
        self.data.insert("y", Value::Int(p.1));
        self.data.insert("z", Value::Int(p.2));
    }

    pub fn add_tag(&mut self, key: String, value: Value<String>) {
        self.data.insert(key.clone(), value);
        sync_tile_pos_coord(self, &key);
    }

    pub fn get_tag(&self, key: &str) -> Result<&Value<String>> {
        self.data
            .get(key)
            .ok_or_else(|| crate::error::Error::MissingField {
                context: "TileEntity",
                key: key.into(),
            })
    }
}

fn sync_tile_pos_coord(te: &mut TileEntity, key: &str) {
    if !matches!(key, "x" | "y" | "z") {
        return;
    }
    if let (Ok(x), Ok(y), Ok(z)) = (
        te.data
            .get("x")
            .map(|v| crate::nbt_helpers::expect_i32(v, "x"))
            .unwrap_or(Ok(0)),
        te.data
            .get("y")
            .map(|v| crate::nbt_helpers::expect_i32(v, "y"))
            .unwrap_or(Ok(0)),
        te.data
            .get("z")
            .map(|v| crate::nbt_helpers::expect_i32(v, "z"))
            .unwrap_or(Ok(0)),
    ) {
        te.position = (x, y, z);
    }
}

#[allow(dead_code)]
/// Split `minecraft:foo[a=b,c=d]` (for Sponge / WorldEdit parity when wired up).
pub(crate) fn parse_block_state_identifier(s: &str) -> Result<BlockState> {
    if let Some(b) = s.find('[') {
        let id = &s[..b];
        let rest = &s[b + 1..s.rfind(']').ok_or_else(|| {
            crate::error::Error::Corrupted(format!("bad block identifier {}", s))
        })?];
        let mut props: BTreeMap<String, String> = BTreeMap::new();
        if !rest.is_empty() {
            for part in rest.split(',') {
                let kv: Vec<&str> = part.splitn(2, '=').collect();
                if kv.len() == 2 {
                    props.insert(kv[0].trim().into(), kv[1].trim().into());
                }
            }
        }
        BlockState::with_properties(id, props)
    } else {
        BlockState::new(s)
    }
}
