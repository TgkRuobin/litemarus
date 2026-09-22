#![allow(clippy::too_many_arguments)]

use std::io::Write;

use valence_nbt::{Compound, List, Value};

use crate::bit_array::{words_required, LitematicaBitArray};
use crate::block_store::BlockStore;
use crate::error::Result;
use crate::minecraft::{BlockState, Entity, TileEntity};
use crate::nbt_helpers::{expect_compound, expect_list_compound, expect_long_array, required};

/// Global air palette entry (`litemapy.schematic.AIR`).
pub static AIR: once_cell::sync::Lazy<BlockState> =
    once_cell::sync::Lazy::new(|| BlockState::air());

#[derive(Clone, Debug)]
pub struct Region {
    x: i32,
    y: i32,
    z: i32,
    width: i32,
    height: i32,
    length: i32,
    palette: Vec<BlockState>,
    /// Flat palette indices in Minecraft order `[abs(width)][abs(height)][abs(length)]`.
    ///
    /// Kept sparse while the region is being built and promoted to the packed
    /// Litematica layout when that is cheaper — see [`BlockStore`].
    blocks: BlockStore,
    pub entities: Vec<Entity>,
    pub tile_entities: Vec<TileEntity>,
    pub block_ticks: Vec<Compound<String>>,
    pub fluid_ticks: Vec<Compound<String>>,
}

fn palette_bit_width(palette_len: usize) -> u32 {
    let lg = if palette_len <= 1 {
        0.0
    } else {
        (palette_len as f64).log2().ceil()
    };
    (lg as u32).max(2)
}

impl Region {
    pub fn new(x: i32, y: i32, z: i32, width: i32, height: i32, length: i32) -> Result<Self> {
        if width == 0 || height == 0 || length == 0 {
            return Err(crate::error::Error::Corrupted(
                "Region dimensions cannot be 0".into(),
            ));
        }
        let mut palette = Vec::with_capacity(1);
        palette.push(AIR.clone());
        Ok(Self {
            x,
            y,
            z,
            width,
            height,
            length,
            palette,
            blocks: BlockStore::empty(),
            entities: Vec::new(),
            tile_entities: Vec::new(),
            block_ticks: Vec::new(),
            fluid_ticks: Vec::new(),
        })
    }

    fn abs_w(&self) -> usize {
        self.width.unsigned_abs() as usize
    }
    fn abs_h(&self) -> usize {
        self.height.unsigned_abs() as usize
    }
    fn abs_l(&self) -> usize {
        self.length.unsigned_abs() as usize
    }

    pub fn volume(&self) -> usize {
        self.abs_w() * self.abs_h() * self.abs_l()
    }

    fn coord_to_flat(&self, x: usize, y: usize, z: usize) -> usize {
        y * (self.abs_w() * self.abs_l()) + z * self.abs_w() + x
    }

    pub fn region_to_store(&self, x: i32, y: i32, z: i32) -> (usize, usize, usize) {
        let mut x = x;
        let mut y = y;
        let mut z = z;
        if self.width < 0 {
            x -= self.width + 1;
        }
        if self.height < 0 {
            y -= self.height + 1;
        }
        if self.length < 0 {
            z -= self.length + 1;
        }
        (x as usize, y as usize, z as usize)
    }

    pub fn get_block_at(&self, x: i32, y: i32, z: i32) -> &BlockState {
        let Ok(i) = self.flat_index(x, y, z) else {
            return &AIR;
        };
        self.palette.get(self.blocks.get(i) as usize).unwrap_or(&AIR)
    }

    pub fn set_block_at(&mut self, x: i32, y: i32, z: i32, block: BlockState) -> Result<()> {
        let i = self.flat_index(x, y, z)?;
        let pindex = self.palette_index_or_push(block);
        // The palette may have just widened: sync before writing so the value fits.
        self.sync_bit_width()?;
        self.blocks.set(i, pindex as u32);
        self.blocks.maybe_pack(self.volume(), self.nbits())
    }

    /// Interns `block` and returns its palette index.
    ///
    /// Batched writers pair this with [`Self::set_palette_index_at`], so a write of
    /// hundreds of thousands of blocks does not clone a `BlockState` per block.
    pub fn intern_block_state(&mut self, block: BlockState) -> Result<u32> {
        let index = self.palette_index_or_push(block);
        self.sync_bit_width()?;
        Ok(index as u32)
    }

    /// Places a block using a palette index from [`Self::intern_block_state`].
    pub fn set_palette_index_at(
        &mut self,
        x: i32,
        y: i32,
        z: i32,
        palette_index: u32,
    ) -> Result<()> {
        if palette_index as usize >= self.palette.len() {
            return Err(crate::error::Error::Value(format!(
                "palette index {palette_index} is out of range (palette holds {} states)",
                self.palette.len()
            )));
        }
        let i = self.flat_index(x, y, z)?;
        self.blocks.set(i, palette_index);
        self.blocks.maybe_pack(self.volume(), self.nbits())
    }

    /// Flat cell index for region-local coordinates, rejecting out-of-range positions.
    ///
    /// The old dense array trapped on those; sparse storage would silently record a
    /// bogus cell instead, so both entry points validate.
    fn flat_index(&self, x: i32, y: i32, z: i32) -> Result<usize> {
        let (xs, ys, zs) = self.region_to_store(x, y, z);
        let (width, height, length) = (self.abs_w(), self.abs_h(), self.abs_l());
        if xs >= width || ys >= height || zs >= length {
            return Err(crate::error::Error::Value(format!(
                "block position ({x}, {y}, {z}) is outside the region bounds ({width}, {height}, {length})"
            )));
        }
        Ok(self.coord_to_flat(xs, ys, zs))
    }

    /// Bit width implied by the current palette (Litematica's rule).
    fn nbits(&self) -> u32 {
        palette_bit_width(self.palette.len())
    }

    /// Keeps a packed store's width equal to `nbits()`. Sparse stores need no work.
    fn sync_bit_width(&mut self) -> Result<()> {
        let want = self.nbits();
        if let BlockStore::Packed(array) = &mut self.blocks {
            if array.nbits() != want {
                array.repack(want)?;
            }
        }
        Ok(())
    }

    fn palette_index_or_push(&mut self, block: BlockState) -> usize {
        if let Some(i) = self.palette.iter().position(|b| b == &block) {
            i
        } else {
            self.palette.push(block);
            self.palette.len() - 1
        }
    }

    fn replace_palette_index(&mut self, old_index: u32, new_index: u32) {
        if old_index == new_index {
            return;
        }
        self.blocks.remap_indices(old_index, new_index);
    }

    fn palette_index_used(&self, old_index: u32) -> bool {
        self.blocks.contains(old_index)
    }

    pub fn optimize_palette(&mut self) {
        let mut new_palette: Vec<BlockState> = Vec::new();
        let old_palette = std::mem::take(&mut self.palette);
        for (old_index, state) in old_palette.into_iter().enumerate() {
            let old_index = old_index as u32;
            if old_index != 0 && !self.palette_index_used(old_index) {
                continue;
            }
            let new_index_u = {
                let mut found = None;
                for (i, other_state) in new_palette.iter().enumerate() {
                    if other_state == &state {
                        found = Some(i as u32);
                        break;
                    }
                }
                match found {
                    Some(i) => i,
                    None => {
                        let i = new_palette.len() as u32;
                        new_palette.push(state);
                        i
                    }
                }
            };
            self.replace_palette_index(old_index, new_index_u);
        }
        self.palette = new_palette;
    }

    pub fn count_blocks(&self) -> usize {
        self.blocks.count_non_zero()
    }

    pub fn palette(&mut self) -> Vec<BlockState> {
        self.optimize_palette();
        self.palette.clone()
    }

    pub fn palette_view_without_optimize(&self) -> &[BlockState] {
        &self.palette
    }

    pub fn contains_block_state(&self, block: &BlockState) -> bool {
        let Some(idx) = self.palette.iter().position(|b| b == block) else {
            return false;
        };
        self.blocks.contains(idx as u32)
    }

    pub fn filter(&mut self, mut f: impl FnMut(BlockState) -> BlockState) {
        self.palette = self.palette.drain(..).map(&mut f).collect();
        if self.palette.first() != Some(&*AIR) {
            // Remapping index 0 means the implicit air cells have to become
            // explicit first, so make sure the region is packed.
            let tmp = self.palette[0].clone();
            self.palette.push(tmp);
            let last = (self.palette.len() - 1) as u32;
            let _ = self.sync_bit_width();
            let _ = self.blocks.pack(self.volume(), self.nbits());
            self.replace_palette_index(0, last);
            self.palette[0] = AIR.clone();
        }
        let _ = self.sync_bit_width();
    }

    pub fn replace_block(&mut self, from: &BlockState, to: BlockState) {
        let Some(index) = self.palette.iter().position(|b| b == from) else {
            return;
        };
        let index = index as u32;
        if index == 0 {
            self.palette.push(to);
            let last = (self.palette.len() - 1) as u32;
            let _ = self.sync_bit_width();
            let _ = self.blocks.pack(self.volume(), self.nbits());
            self.replace_palette_index(0, last);
        } else {
            self.palette[index as usize] = to;
        }
        let _ = self.sync_bit_width();
    }

    pub fn x(&self) -> i32 {
        self.x
    }
    pub fn y(&self) -> i32 {
        self.y
    }
    pub fn z(&self) -> i32 {
        self.z
    }
    pub fn width(&self) -> i32 {
        self.width
    }
    pub fn height(&self) -> i32 {
        self.height
    }
    pub fn length(&self) -> i32 {
        self.length
    }

    pub fn min_schem_x(&self) -> i32 {
        std::cmp::min(self.x, self.x + self.width + 1)
    }
    pub fn max_schem_x(&self) -> i32 {
        std::cmp::max(self.x, self.x + self.width - 1)
    }
    pub fn min_schem_y(&self) -> i32 {
        std::cmp::min(self.y, self.y + self.height + 1)
    }
    pub fn max_schem_y(&self) -> i32 {
        std::cmp::max(self.y, self.y + self.height - 1)
    }
    pub fn min_schem_z(&self) -> i32 {
        std::cmp::min(self.z, self.z + self.length + 1)
    }
    pub fn max_schem_z(&self) -> i32 {
        std::cmp::max(self.z, self.z + self.length - 1)
    }

    pub fn min_x(&self) -> i32 {
        std::cmp::min(0, self.width + 1)
    }
    pub fn max_x(&self) -> i32 {
        std::cmp::max(0, self.width - 1)
    }
    pub fn min_y(&self) -> i32 {
        std::cmp::min(0, self.height + 1)
    }
    pub fn max_y(&self) -> i32 {
        std::cmp::max(0, self.height - 1)
    }
    pub fn min_z(&self) -> i32 {
        std::cmp::min(0, self.length + 1)
    }
    pub fn max_z(&self) -> i32 {
        std::cmp::max(0, self.length - 1)
    }

    pub fn range_x(&self) -> std::ops::RangeInclusive<i32> {
        self.min_x()..=self.max_x()
    }
    pub fn range_y(&self) -> std::ops::RangeInclusive<i32> {
        self.min_y()..=self.max_y()
    }
    pub fn range_z(&self) -> std::ops::RangeInclusive<i32> {
        self.min_z()..=self.max_z()
    }

    pub fn block_positions(&self) -> impl Iterator<Item = (i32, i32, i32)> + '_ {
        self.range_x().flat_map(move |x| {
            self.range_y()
                .flat_map(move |y| self.range_z().map(move |z| (x, y, z)))
        })
    }

    /// Visits every cell holding a block, yielding region-local coordinates.
    ///
    /// For sparse regions this is proportional to the number of placed blocks
    /// instead of to the bounding box, which makes dump/preview calls cheap on
    /// large artwork.
    pub fn for_each_set_block<F: FnMut(i32, i32, i32, &BlockState)>(&self, mut f: F) {
        let width = self.abs_w();
        let plane = width * self.abs_l();
        if plane == 0 {
            return;
        }
        self.blocks.for_each_set(|index, palette_index| {
            let Some(block) = self.palette.get(palette_index as usize) else {
                return;
            };
            let y = index / plane;
            let rest = index % plane;
            let z = rest / width;
            let x = rest % width;
            f(x as i32, y as i32, z as i32, block);
        });
    }

    pub fn to_nbt(&mut self) -> Result<Compound<String>> {
        self.optimize_palette();
        self.sync_bit_width()?;
        let mut root = Compound::new();
        let mut pos = Compound::new();
        pos.insert("x", Value::Int(self.x));
        pos.insert("y", Value::Int(self.y));
        pos.insert("z", Value::Int(self.z));
        root.insert("Position", Value::Compound(pos));
        let mut size = Compound::new();
        size.insert("x", Value::Int(self.width));
        size.insert("y", Value::Int(self.height));
        size.insert("z", Value::Int(self.length));
        root.insert("Size", Value::Compound(size));

        let plt: Vec<Compound<String>> = self.palette.iter().map(|b| b.to_nbt()).collect();
        root.insert("BlockStatePalette", Value::List(List::Compound(plt)));

        let entities: Vec<Compound<String>> = self.entities.iter().map(|e| e.to_nbt()).collect();
        root.insert("Entities", Value::List(List::Compound(entities)));

        let tile_entities: Vec<Compound<String>> =
            self.tile_entities.iter().map(|t| t.to_nbt()).collect();
        root.insert("TileEntities", Value::List(List::Compound(tile_entities)));

        root.insert(
            "PendingBlockTicks",
            Value::List(List::Compound(self.block_ticks.clone())),
        );
        root.insert(
            "PendingFluidTicks",
            Value::List(List::Compound(self.fluid_ticks.clone())),
        );

        let nbits = palette_bit_width(self.palette.len());
        let arr = self.blocks.packed(self.volume(), nbits)?;
        root.insert("BlockStates", arr.to_nbt_value());
        Ok(root)
    }

    /// Streams this region's NBT compound body (without the enclosing tag header)
    /// straight into `writer`, so the `BlockStates` array never exists as a second
    /// in-memory copy.
    pub fn write_nbt_body<W: Write + ?Sized>(&mut self, writer: &mut W) -> Result<()> {
        use crate::nbt_io::{write_named_value, write_raw_string, write_tag, TAG_LONG_ARRAY};

        self.optimize_palette();
        self.sync_bit_width()?;

        let mut pos = Compound::new();
        pos.insert("x", Value::Int(self.x));
        pos.insert("y", Value::Int(self.y));
        pos.insert("z", Value::Int(self.z));
        write_named_value(writer, "Position", &Value::Compound(pos))?;

        let mut size = Compound::new();
        size.insert("x", Value::Int(self.width));
        size.insert("y", Value::Int(self.height));
        size.insert("z", Value::Int(self.length));
        write_named_value(writer, "Size", &Value::Compound(size))?;

        let plt: Vec<Compound<String>> = self.palette.iter().map(|b| b.to_nbt()).collect();
        write_named_value(
            writer,
            "BlockStatePalette",
            &Value::List(List::Compound(plt)),
        )?;

        let entities: Vec<Compound<String>> = self.entities.iter().map(|e| e.to_nbt()).collect();
        write_named_value(writer, "Entities", &Value::List(List::Compound(entities)))?;

        let tile_entities: Vec<Compound<String>> =
            self.tile_entities.iter().map(|t| t.to_nbt()).collect();
        write_named_value(
            writer,
            "TileEntities",
            &Value::List(List::Compound(tile_entities)),
        )?;

        write_named_value(
            writer,
            "PendingBlockTicks",
            &Value::List(List::Compound(self.block_ticks.clone())),
        )?;
        write_named_value(
            writer,
            "PendingFluidTicks",
            &Value::List(List::Compound(self.fluid_ticks.clone())),
        )?;

        write_tag(writer, TAG_LONG_ARRAY)?;
        write_raw_string(writer, "BlockStates")?;
        let nbits = self.nbits();
        self.blocks.write_block_states(writer, self.volume(), nbits)
    }

    pub fn from_nbt(nbt: &Compound<String>) -> Result<Self> {
        let pos = expect_compound(required(nbt, "Region", "Position")?, "Region", "Position")?;
        let pos_x = crate::nbt_helpers::expect_i32(required(pos, "Region.Pos", "x")?, "x")?;
        let pos_y = crate::nbt_helpers::expect_i32(required(pos, "Region.Pos", "y")?, "y")?;
        let pos_z = crate::nbt_helpers::expect_i32(required(pos, "Region.Pos", "z")?, "z")?;

        let size = expect_compound(required(nbt, "Region", "Size")?, "Region", "Size")?;
        let width = crate::nbt_helpers::expect_i32(required(size, "Region.Size", "x")?, "width")?;
        let height = crate::nbt_helpers::expect_i32(required(size, "Region.Size", "y")?, "height")?;
        let length = crate::nbt_helpers::expect_i32(required(size, "Region.Size", "z")?, "length")?;

        let mut region = Region::new(pos_x, pos_y, pos_z, width, height, length)?;
        region.palette.clear();

        let plt = expect_list_compound(required(nbt, "Region", "BlockStatePalette")?, "palette")?;
        for block_nbt in plt {
            region.palette.push(BlockState::from_nbt(block_nbt)?);
        }

        let ent = expect_list_compound(required(nbt, "Region", "Entities")?, "entities")?;
        for e in ent {
            region.entities.push(Entity::from_nbt(e.clone())?);
        }

        let tile = expect_list_compound(required(nbt, "Region", "TileEntities")?, "tile_entities")?;
        for t in tile {
            region.tile_entities.push(TileEntity::from_nbt(t.clone())?);
        }

        let block_states_val = required(nbt, "Region", "BlockStates")?;
        let longs = match block_states_val {
            Value::LongArray(a) => a.as_slice(),
            _ => expect_long_array(block_states_val, "BlockStates")?,
        };
        // Adopt the file's packed longs as-is: no per-cell copy, no dense index array.
        let nbits = palette_bit_width(region.palette.len());
        let expected = words_required(region.volume(), nbits).ok_or_else(|| {
            crate::error::Error::Corrupted(format!(
                "region of {} cells does not fit in memory",
                region.volume()
            ))
        })?;
        if expected != longs.len() {
            return Err(crate::error::Error::Corrupted(format!(
                "long array length does not match bit array size and nbits, expected {}, not {}",
                expected,
                longs.len()
            )));
        }
        let words: Vec<u64> = longs.iter().map(|&value| value as u64).collect();
        region.blocks = BlockStore::Packed(LitematicaBitArray::from_words(
            words,
            region.volume(),
            nbits,
        )?);

        if let Some(v) = nbt.get("PendingBlockTicks") {
            if let Value::List(List::Compound(list)) = v {
                region.block_ticks = list.clone();
            }
        }
        if let Some(v) = nbt.get("PendingFluidTicks") {
            if let Value::List(List::Compound(list)) = v {
                region.fluid_ticks = list.clone();
            }
        }

        Ok(region)
    }
}
