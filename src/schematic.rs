use std::io::{BufWriter, Write};
use std::path::Path;

use flate2::write::GzEncoder;
use flate2::Compression;

#[cfg(all(target_arch = "wasm32", feature = "wasm"))]
use js_sys::Date as JsDate;
#[cfg(not(target_arch = "wasm32"))]
use std::time::{SystemTime, UNIX_EPOCH};

use valence_nbt::{Compound, Value};

use crate::constants::*;
use crate::error::Result;
use crate::nbt_helpers::{expect_compound, expect_i32, expect_string, required};
use crate::nbt_io::{
    read_gzip_nbt, write_compound, write_end, write_named_int, write_raw_string, write_tag,
    TAG_COMPOUND,
};
use crate::region::Region;
use crate::regions_map::RegionsMap;

#[derive(Clone, Debug)]
pub struct SaveMeta {
    pub update_meta: bool,
    pub save_soft: bool,
    pub gzip_compression: Compression,
}

impl Default for SaveMeta {
    fn default() -> Self {
        Self {
            update_meta: true,
            save_soft: true,
            gzip_compression: Compression::default(),
        }
    }
}

#[derive(Clone, Debug)]
pub struct Schematic {
    pub name: String,
    pub author: String,
    pub description: String,
    lm_version: i32,
    lm_subversion: i32,
    mc_version: i32,
    created: i64,
    modified: i64,
    preview: Vec<i32>,
    regions: RegionsMap,
}

impl Default for Schematic {
    fn default() -> Self {
        let now_ms = millis_now();
        Self {
            name: DEFAULT_NAME.into(),
            author: String::new(),
            description: String::new(),
            lm_version: LITEMATIC_VERSION,
            lm_subversion: LITEMATIC_SUBVERSION,
            mc_version: MC_DATA_VERSION,
            created: now_ms,
            modified: now_ms,
            preview: Vec::new(),
            regions: RegionsMap::new_empty(),
        }
    }
}

pub fn millis_now() -> i64 {
    #[cfg(all(target_arch = "wasm32", feature = "wasm"))]
    {
        JsDate::now() as i64
    }
    #[cfg(not(all(target_arch = "wasm32", feature = "wasm")))]
    {
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_millis() as i64)
            .unwrap_or(0)
    }
}

impl Schematic {
    pub fn new(
        name: Option<&str>,
        author: Option<&str>,
        description: Option<&str>,
        mc_version: i32,
    ) -> Self {
        let mut s = Self::default();
        if let Some(n) = name {
            s.name = n.into();
        }
        if let Some(a) = author {
            s.author = a.into();
        }
        if let Some(d) = description {
            s.description = d.into();
        }
        s.mc_version = mc_version;
        s.created = millis_now();
        s.modified = millis_now();
        s
    }

    pub fn with_regions(
        name: &str,
        author: &str,
        description: &str,
        regions: Vec<(String, Region)>,
    ) -> Self {
        let mut m = RegionsMap::new_empty();
        for (k, v) in regions {
            let _ = m.insert(k, v);
        }
        let now = millis_now();
        Self {
            name: name.into(),
            author: author.into(),
            description: description.into(),
            lm_version: LITEMATIC_VERSION,
            lm_subversion: LITEMATIC_SUBVERSION,
            mc_version: MC_DATA_VERSION,
            created: now,
            modified: now,
            preview: Vec::new(),
            regions: m,
        }
    }

    pub fn regions(&self) -> &RegionsMap {
        &self.regions
    }

    pub fn regions_mut(&mut self) -> &mut RegionsMap {
        &mut self.regions
    }

    pub fn width(&self) -> i32 {
        self.regions.width()
    }
    pub fn height(&self) -> i32 {
        self.regions.height()
    }
    pub fn length(&self) -> i32 {
        self.regions.length()
    }

    pub fn lm_version(&self) -> i32 {
        self.lm_version
    }
    pub fn set_lm_version(&mut self, v: i32) {
        self.lm_version = v;
    }
    pub fn lm_subversion(&self) -> i32 {
        self.lm_subversion
    }
    pub fn set_lm_subversion(&mut self, v: i32) {
        self.lm_subversion = v;
    }
    pub fn mc_version(&self) -> i32 {
        self.mc_version
    }
    pub fn set_mc_version(&mut self, v: i32) {
        self.mc_version = v;
    }

    pub fn created(&self) -> i64 {
        self.created
    }
    pub fn set_created(&mut self, v: i64) {
        self.created = v;
    }
    pub fn modified(&self) -> i64 {
        self.modified
    }
    pub fn set_modified(&mut self, v: i64) {
        self.modified = v;
    }

    pub fn preview(&self) -> &[i32] {
        &self.preview
    }
    pub fn set_preview(&mut self, p: Vec<i32>) {
        self.preview = p;
    }

    pub fn name(&self) -> &str {
        &self.name
    }
    pub fn set_name(&mut self, n: String) {
        self.name = n;
    }
    pub fn author(&self) -> &str {
        &self.author
    }
    pub fn set_author(&mut self, a: String) {
        self.author = a;
    }
    pub fn description(&self) -> &str {
        &self.description
    }
    pub fn set_description(&mut self, d: String) {
        self.description = d;
    }

    pub fn update_metadata(&mut self) {
        self.modified = millis_now();
    }

    pub fn to_nbt(&mut self, save_soft: bool) -> Result<Compound<String>> {
        if self.regions.is_empty() {
            return Err(crate::error::Error::Value(
                "Empty schematic does not have any regions".into(),
            ));
        }
        let mut root = Compound::new();
        root.insert("Version", Value::Int(self.lm_version));
        root.insert("SubVersion", Value::Int(self.lm_subversion));
        root.insert("MinecraftDataVersion", Value::Int(self.mc_version));
        root.insert("Metadata", Value::Compound(self.build_metadata(save_soft)));

        let mut regs = Compound::new();
        for (name, region) in self.regions.iter_mut() {
            regs.insert(name.clone(), Value::Compound(region.to_nbt()?));
        }
        root.insert("Regions", Value::Compound(regs));
        Ok(root)
    }

    /// Metadata compound shared by the tree and streaming writers.
    fn build_metadata(&self, save_soft: bool) -> Compound<String> {
        let mut meta = Compound::new();
        let mut enclose = Compound::new();
        enclose.insert("x", Value::Int(self.width()));
        enclose.insert("y", Value::Int(self.height()));
        enclose.insert("z", Value::Int(self.length()));
        meta.insert("EnclosingSize", Value::Compound(enclose));
        meta.insert("Author", Value::String(self.author.clone()));
        meta.insert("Description", Value::String(self.description.clone()));
        meta.insert("Name", Value::String(self.name.clone()));
        if save_soft {
            meta.insert(
                "Software",
                Value::String(format!("{}_{}", LITEMAPY_NAME, LITEMAPY_VERSION)),
            );
        }
        meta.insert("RegionCount", Value::Int(self.regions.len() as i32));
        meta.insert("TimeCreated", Value::Long(self.created));
        meta.insert("TimeModified", Value::Long(self.modified));
        let total_blocks: i32 = self.regions.values().map(|r| r.count_blocks() as i32).sum();
        let total_volume: i32 = self.regions.values().map(|r| r.volume() as i32).sum();
        meta.insert("TotalBlocks", Value::Int(total_blocks));
        meta.insert("TotalVolume", Value::Int(total_volume));
        meta.insert("PreviewImageData", Value::IntArray(self.preview.clone()));
        meta
    }

    /// Serialises the schematic as NBT straight into `writer`.
    ///
    /// Region `BlockStates` arrays are streamed from their packed storage, so
    /// saving never needs a second full-size copy of the artwork (the tree-based
    /// [`Self::to_nbt`] does, which is why the save paths use this instead).
    pub fn write_nbt_to<W: Write + ?Sized>(
        &mut self,
        writer: &mut W,
        save_soft: bool,
    ) -> Result<()> {
        if self.regions.is_empty() {
            return Err(crate::error::Error::Value(
                "Empty schematic does not have any regions".into(),
            ));
        }
        let meta = self.build_metadata(save_soft);

        write_tag(writer, TAG_COMPOUND)?;
        write_raw_string(writer, "")?;
        write_named_int(writer, "Version", self.lm_version)?;
        write_named_int(writer, "SubVersion", self.lm_subversion)?;
        write_named_int(writer, "MinecraftDataVersion", self.mc_version)?;
        write_compound(writer, "Metadata", &meta)?;

        write_tag(writer, TAG_COMPOUND)?;
        write_raw_string(writer, "Regions")?;
        for (name, region) in self.regions.iter_mut() {
            write_tag(writer, TAG_COMPOUND)?;
            write_raw_string(writer, name)?;
            region.write_nbt_body(writer)?;
            write_end(writer)?;
        }
        write_end(writer)?;
        write_end(writer)?;
        Ok(())
    }

    pub fn save(&mut self, path: impl AsRef<Path>, options: SaveMeta) -> Result<()> {
        if options.update_meta {
            self.update_metadata();
        }
        let file = std::fs::File::create(path)?;
        let mut encoder = GzEncoder::new(BufWriter::new(file), options.gzip_compression);
        self.write_nbt_to(&mut encoder, options.save_soft)?;
        encoder.finish()?.flush()?;
        Ok(())
    }

    pub fn save_bytes(&mut self, options: SaveMeta) -> Result<Vec<u8>> {
        if options.update_meta {
            self.update_metadata();
        }
        let mut encoder = GzEncoder::new(Vec::new(), options.gzip_compression);
        self.write_nbt_to(&mut encoder, options.save_soft)?;
        Ok(encoder.finish()?)
    }

    pub fn load(path: impl AsRef<Path>) -> Result<Self> {
        let bytes = std::fs::read(path)?;
        Self::from_bytes(&bytes)
    }

    pub fn from_bytes(bytes: &[u8]) -> Result<Self> {
        let root = read_gzip_nbt(bytes)?;
        Self::from_nbt(&root)
    }

    pub fn from_nbt(nbt: &Compound<String>) -> Result<Self> {
        let meta = expect_compound(required(nbt, "Schematic", "Metadata")?, "Metadata", "")?;
        let lm_version = expect_i32(required(nbt, "Schematic", "Version")?, "Version")?;
        let lm_subversion = match nbt.get("SubVersion") {
            Some(v) => expect_i32(v, "SubVersion")?,
            None => 0,
        };
        let mc_version = expect_i32(
            required(nbt, "Schematic", "MinecraftDataVersion")?,
            "MinecraftDataVersion",
        )?;

        let width = expect_i32(
            expect_compound(
                required(meta, "Metadata", "EnclosingSize")?,
                "EnclosingSize",
                "",
            )?
            .get("x")
            .ok_or_else(|| crate::error::Error::MissingField {
                context: "EnclosingSize",
                key: "x".into(),
            })?,
            "EnclosingSize.x",
        )?;
        let height = expect_i32(
            expect_compound(
                required(meta, "Metadata", "EnclosingSize")?,
                "EnclosingSize",
                "",
            )?
            .get("y")
            .ok_or_else(|| crate::error::Error::MissingField {
                context: "EnclosingSize",
                key: "y".into(),
            })?,
            "EnclosingSize.y",
        )?;
        let length = expect_i32(
            expect_compound(
                required(meta, "Metadata", "EnclosingSize")?,
                "EnclosingSize",
                "",
            )?
            .get("z")
            .ok_or_else(|| crate::error::Error::MissingField {
                context: "EnclosingSize",
                key: "z".into(),
            })?,
            "EnclosingSize.z",
        )?;

        let author = expect_string(required(meta, "Metadata", "Author")?, "Author")?;
        let name = expect_string(required(meta, "Metadata", "Name")?, "Name")?;
        let desc = expect_string(required(meta, "Metadata", "Description")?, "Description")?;

        let regions_compound =
            expect_compound(required(nbt, "Schematic", "Regions")?, "Regions", "")?;
        let mut regions = indexmap::IndexMap::new();
        for (key, val) in regions_compound.iter() {
            let reg_nbt = expect_compound(val, "Region entry", key)?;
            let reg = Region::from_nbt(reg_nbt)?;
            regions.insert(key.clone(), reg);
        }

        let sch = Schematic {
            name,
            author,
            description: desc,
            lm_version,
            lm_subversion,
            mc_version,
            created: crate::nbt_helpers::expect_i64(
                required(meta, "Metadata", "TimeCreated")?,
                "TimeCreated",
            )?,
            modified: crate::nbt_helpers::expect_i64(
                required(meta, "Metadata", "TimeModified")?,
                "TimeModified",
            )?,
            preview: match meta.get("PreviewImageData") {
                Some(Value::IntArray(arr)) => arr.clone(),
                _ => Vec::new(),
            },
            regions: RegionsMap::from_entries(regions),
        };

        verify_enclosure(width, height, length, &sch)?;

        if let Some(rc) = meta.get("RegionCount") {
            let n = expect_i32(rc, "RegionCount")?;
            if n as usize != sch.regions.len() {
                return Err(crate::error::Error::Corrupted(
                    "Number of regions in metadata does not match the number of parsed regions"
                        .into(),
                ));
            }
        }

        Ok(sch)
    }
}

pub fn schematic_from_single_region(
    region: Region,
    name: &str,
    author: &str,
    description: &str,
    mc_version: i32,
) -> Schematic {
    let mut sch = Schematic::new(Some(name), Some(author), Some(description), mc_version);
    sch.regions_mut().insert(name.to_string(), region);
    sch
}

fn verify_enclosure(w: i32, h: i32, l: i32, sch: &Schematic) -> Result<()> {
    let ok_w = sch.width() == w;
    let ok_h = sch.height() == h;
    let ok_l = sch.length() == l;
    if !ok_w {
        return Err(crate::error::Error::Corrupted(format!(
            "Invalid schematic width in metadata, excepted {} was {}",
            sch.width(),
            w
        )));
    }
    if !ok_h {
        return Err(crate::error::Error::Corrupted(format!(
            "Invalid schematic height in metadata, excepted {} was {}",
            sch.height(),
            h
        )));
    }
    if !ok_l {
        return Err(crate::error::Error::Corrupted(format!(
            "Invalid schematic length in metadata, excepted {} was {}",
            sch.length(),
            l
        )));
    }
    Ok(())
}
