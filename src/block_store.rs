//! Block storage backing a [`Region`](crate::region::Region).
//!
//! Litematica stores a region as a packed bit array covering its whole bounding
//! box. That is the right layout for dense builds and for reading files, but it
//! is a poor fit for artwork, which is mostly air: an 8x6 map art region spans
//! hundreds of millions of cells while only ~0.2% of them hold a block.
//!
//! [`BlockStore`] therefore keeps built regions sparse and only promotes to the
//! packed layout once that becomes the cheaper (or required) representation:
//!
//! - building a region block by block costs memory proportional to the blocks
//!   actually placed, not to `width * height * length`;
//! - saving streams the packed longs straight into the gzip writer, so the
//!   full-size array never has to exist in memory at all;
//! - regions read from a file adopt the packed words without any per-cell work.

use std::collections::BTreeMap;
use std::io::Write;

use crate::bit_array::{words_required, LitematicaBitArray};
use crate::error::{Error, Result};

/// Rough per-entry cost of the sparse map (key + value + B-tree overhead) used
/// to decide when packing the box becomes cheaper than staying sparse.
const SPARSE_ENTRY_BYTES: usize = 48;

/// Longs written per `write_all` call while streaming (4 KiB).
const CHUNK_WORDS: usize = 512;

#[derive(Clone, Debug)]
pub(crate) enum BlockStore {
    /// Only non-zero (non-air) cells are stored.
    Sparse(BTreeMap<usize, u32>),
    /// Packed over the whole box — identical to the `.litematic` `BlockStates` array.
    Packed(LitematicaBitArray),
}

impl BlockStore {
    pub(crate) fn empty() -> Self {
        Self::Sparse(BTreeMap::new())
    }

    #[cfg(test)]
    pub(crate) fn is_sparse(&self) -> bool {
        matches!(self, Self::Sparse(_))
    }

    pub(crate) fn get(&self, index: usize) -> u32 {
        match self {
            Self::Sparse(map) => map.get(&index).copied().unwrap_or(0),
            Self::Packed(array) => array.get_or_zero(index),
        }
    }

    /// Writes `value` at `index`; `0` means air and drops the sparse entry.
    pub(crate) fn set(&mut self, index: usize, value: u32) {
        match self {
            Self::Sparse(map) => {
                if value == 0 {
                    map.remove(&index);
                } else {
                    map.insert(index, value);
                }
            }
            Self::Packed(array) => {
                // `value` comes from the palette, so it always fits the current width.
                let _ = array.set(index, value);
            }
        }
    }

    pub(crate) fn count_non_zero(&self) -> usize {
        match self {
            Self::Sparse(map) => map.values().filter(|&&value| value != 0).count(),
            Self::Packed(array) => {
                let mut count = 0;
                for index in 0..array.size() {
                    if array.get_or_zero(index) != 0 {
                        count += 1;
                    }
                }
                count
            }
        }
    }

    pub(crate) fn contains(&self, palette_index: u32) -> bool {
        match self {
            Self::Sparse(map) => map.values().any(|&value| value == palette_index),
            Self::Packed(array) => {
                (0..array.size()).any(|index| array.get_or_zero(index) == palette_index)
            }
        }
    }

    /// Visits every cell holding a block, in ascending cell order.
    pub(crate) fn for_each_set<F: FnMut(usize, u32)>(&self, mut f: F) {
        match self {
            Self::Sparse(map) => {
                for (&index, &value) in map.iter() {
                    if value != 0 {
                        f(index, value);
                    }
                }
            }
            Self::Packed(array) => {
                for index in 0..array.size() {
                    let value = array.get_or_zero(index);
                    if value != 0 {
                        f(index, value);
                    }
                }
            }
        }
    }

    /// Rewrites palette indices. Callers that must also rewrite the implicit air
    /// cells (see `Region::filter`) pack first; `optimize_palette` never remaps
    /// index 0 away from 0, so this is safe for both layouts.
    pub(crate) fn remap_indices(&mut self, old: u32, new: u32) {
        if old == new {
            return;
        }
        match self {
            Self::Sparse(map) => {
                for value in map.values_mut() {
                    if *value == old {
                        *value = new;
                    }
                }
            }
            Self::Packed(array) => {
                for index in 0..array.size() {
                    if array.get_or_zero(index) == old {
                        let _ = array.set(index, new);
                    }
                }
            }
        }
    }

    /// Promotes to the packed layout when the sparse map costs more than the box.
    pub(crate) fn maybe_pack(&mut self, volume: usize, nbits: u32) -> Result<()> {
        let Self::Sparse(map) = self else {
            return Ok(());
        };
        let Some(packed_bytes) = words_required(volume, nbits).map(|words| words.saturating_mul(8))
        else {
            // Too large to pack on this target (32-bit `usize`); the sparse layout
            // still serialises correctly, so keep it.
            return Ok(());
        };
        if map.len().saturating_mul(SPARSE_ENTRY_BYTES) >= packed_bytes {
            self.pack(volume, nbits)?;
        }
        Ok(())
    }

    /// Materialises the packed layout.
    pub(crate) fn pack(&mut self, volume: usize, nbits: u32) -> Result<()> {
        if matches!(self, Self::Packed(_)) {
            return Ok(());
        }
        let mut array = LitematicaBitArray::try_new(volume, nbits)?;
        if let Self::Sparse(map) = self {
            for (&index, &value) in map.iter() {
                if value != 0 {
                    array.set(index, value)?;
                }
            }
        }
        *self = Self::Packed(array);
        Ok(())
    }

    pub(crate) fn packed(&self, volume: usize, nbits: u32) -> Result<LitematicaBitArray> {
        match self {
            Self::Packed(array) => {
                if array.nbits() == nbits {
                    Ok(array.clone())
                } else {
                    let mut array = array.clone();
                    array.repack(nbits)?;
                    Ok(array)
                }
            }
            Self::Sparse(_) => {
                let mut store = self.clone();
                store.pack(volume, nbits)?;
                match store {
                    Self::Packed(array) => Ok(array),
                    Self::Sparse(_) => unreachable!("pack always yields the packed layout"),
                }
            }
        }
    }

    /// Streams the `BlockStates` payload: `i32` long count followed by big-endian longs.
    ///
    /// Nothing larger than [`CHUNK_WORDS`] longs is ever allocated, so the peak
    /// memory of a save is the region's own storage plus the gzip output.
    pub(crate) fn write_block_states<W: Write + ?Sized>(
        &self,
        writer: &mut W,
        volume: usize,
        nbits: u32,
    ) -> Result<()> {
        let words = words_required(volume, nbits).ok_or_else(|| {
            Error::Value(format!(
                "region of {volume} cells x {nbits} bits does not fit in memory"
            ))
        })?;
        let word_count = i32::try_from(words).map_err(|_| {
            Error::Value(format!(
                "region block states array is too long ({words} longs)"
            ))
        })?;
        writer.write_all(&word_count.to_be_bytes())?;

        match self {
            Self::Packed(array) => {
                if array.nbits() != nbits {
                    return Err(Error::Value(format!(
                        "packed block states width {} does not match palette width {nbits}",
                        array.nbits()
                    )));
                }
                write_words(writer, array.words())
            }
            Self::Sparse(map) => write_sparse(writer, map, volume, nbits),
        }
    }
}

fn write_words<W: Write + ?Sized>(writer: &mut W, words: &[u64]) -> Result<()> {
    let mut buf = [0u8; CHUNK_WORDS * 8];
    for chunk in words.chunks(CHUNK_WORDS) {
        for (slot, word) in chunk.iter().enumerate() {
            buf[slot * 8..slot * 8 + 8].copy_from_slice(&word.to_be_bytes());
        }
        writer.write_all(&buf[..chunk.len() * 8])?;
    }
    Ok(())
}

fn write_sparse<W: Write + ?Sized>(
    writer: &mut W,
    map: &BTreeMap<usize, u32>,
    volume: usize,
    nbits: u32,
) -> Result<()> {
    let mut streamer = PackedStreamer::new(writer, nbits)?;
    let mut cursor = 0usize;
    for (&index, &value) in map.iter() {
        if value == 0 || index >= volume {
            continue;
        }
        if index > cursor {
            streamer.skip(index - cursor)?;
        }
        streamer.push(value)?;
        cursor = index + 1;
    }
    if cursor < volume {
        streamer.skip(volume - cursor)?;
    }
    streamer.finish()
}

/// Packs palette indices into Litematica longs, flushing in fixed-size chunks.
struct PackedStreamer<'a, W: Write + ?Sized> {
    writer: &'a mut W,
    buffer: [u8; CHUNK_WORDS * 8],
    buffered_words: usize,
    current: u64,
    current_bits: u32,
    nbits: u32,
    mask: u64,
}

impl<'a, W: Write + ?Sized> PackedStreamer<'a, W> {
    fn new(writer: &'a mut W, nbits: u32) -> Result<Self> {
        if nbits == 0 || nbits > 32 {
            return Err(Error::Value(format!("unsupported bit width {nbits}")));
        }
        Ok(Self {
            writer,
            buffer: [0u8; CHUNK_WORDS * 8],
            buffered_words: 0,
            current: 0,
            current_bits: 0,
            nbits,
            mask: (1u64 << nbits) - 1,
        })
    }

    fn push(&mut self, value: u32) -> Result<()> {
        let mut value = value as u64 & self.mask;
        let mut remaining = self.nbits;
        while remaining > 0 {
            let space = 64 - self.current_bits;
            let take = space.min(remaining);
            self.current |= (value & ((1u64 << take) - 1)) << self.current_bits;
            self.current_bits += take;
            remaining -= take;
            value >>= take;
            if self.current_bits == 64 {
                self.flush_word()?;
            }
        }
        Ok(())
    }

    /// Emits `count` zero values (air runs) without touching memory per cell.
    fn skip(&mut self, count: usize) -> Result<()> {
        // Bit counts of large regions exceed 2^32, so do this in 64-bit math.
        let mut bits = count as u64 * self.nbits as u64;
        if self.current_bits != 0 {
            let space = (64 - self.current_bits) as u64;
            let take = space.min(bits);
            self.current_bits += take as u32;
            bits -= take;
            if self.current_bits == 64 {
                self.flush_word()?;
            }
        }
        // Only once the current word is complete can whole words be emitted directly;
        // a partially skipped word has to keep its cursor.
        if self.current_bits == 0 {
            let whole_words = (bits / 64) as usize;
            if whole_words > 0 {
                self.write_zero_words(whole_words)?;
                bits -= whole_words as u64 * 64;
            }
            self.current = 0;
            self.current_bits = bits as u32;
        }
        Ok(())
    }

    fn flush_word(&mut self) -> Result<()> {
        let slot = self.buffered_words;
        self.buffer[slot * 8..slot * 8 + 8].copy_from_slice(&self.current.to_be_bytes());
        self.buffered_words += 1;
        self.current = 0;
        self.current_bits = 0;
        if self.buffered_words == CHUNK_WORDS {
            self.flush_buffer()?;
        }
        Ok(())
    }

    fn write_zero_words(&mut self, mut words: usize) -> Result<()> {
        while words > 0 {
            let space = CHUNK_WORDS - self.buffered_words;
            let take = space.min(words);
            let start = self.buffered_words * 8;
            self.buffer[start..start + take * 8].fill(0);
            self.buffered_words += take;
            words -= take;
            if self.buffered_words == CHUNK_WORDS {
                self.flush_buffer()?;
            }
        }
        Ok(())
    }

    fn flush_buffer(&mut self) -> Result<()> {
        let bytes = self.buffered_words * 8;
        self.writer.write_all(&self.buffer[..bytes])?;
        self.buffered_words = 0;
        Ok(())
    }

    fn finish(mut self) -> Result<()> {
        if self.current_bits > 0 {
            self.flush_word()?;
        }
        self.flush_buffer()?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn stream(store: &BlockStore, volume: usize, nbits: u32) -> Vec<u8> {
        let mut out = Vec::new();
        store.write_block_states(&mut out, volume, nbits).unwrap();
        out
    }

    fn mask(nbits: u32) -> u32 {
        (1u32 << nbits) - 1
    }

    #[test]
    fn sparse_and_packed_layouts_stream_identically() {
        for nbits in 2..=10u32 {
            let volume = 4099usize;
            let mut sparse = BlockStore::empty();
            for index in 0..volume {
                let value = if index % 3 == 0 {
                    0
                } else {
                    ((index * 37) as u32) % mask(nbits)
                };
                sparse.set(index, value);
            }

            let mut packed = sparse.clone();
            packed.pack(volume, nbits).unwrap();
            assert!(matches!(packed, BlockStore::Packed(_)));

            assert_eq!(
                stream(&sparse, volume, nbits),
                stream(&packed, volume, nbits),
                "nbits {nbits}"
            );
            assert_eq!(sparse.count_non_zero(), packed.count_non_zero());
            for index in [0, 1, 63, 64, 65, 4095, 4098] {
                assert_eq!(sparse.get(index), packed.get(index), "index {index}");
            }
        }
    }

    #[test]
    fn remap_keeps_both_layouts_in_sync() {
        let volume = 1024usize;
        let mut sparse = BlockStore::empty();
        for index in 0..volume {
            sparse.set(index, (index % 3) as u32);
        }
        let mut packed = sparse.clone();
        packed.pack(volume, 2).unwrap();

        sparse.remap_indices(1, 2);
        packed.remap_indices(1, 2);

        assert_eq!(stream(&sparse, volume, 2), stream(&packed, volume, 2));
    }

    #[test]
    fn dense_regions_promote_to_packed_layout() {
        let (width, height, length) = (64usize, 64usize, 64usize);
        let volume = width * height * length;
        let mut store = BlockStore::empty();
        for index in 0..volume {
            store.set(index, 1);
            store.maybe_pack(volume, 2).unwrap();
        }
        assert!(matches!(store, BlockStore::Packed(_)));
        assert_eq!(store.count_non_zero(), volume);
    }

    #[test]
    fn sparse_regions_stay_sparse_for_large_boxes() {
        // 1e9 cells: the packed layout would need ~250 MiB, the sparse one ~5 KiB.
        let volume = 1_000_000_000usize;
        let mut store = BlockStore::empty();
        for index in (0..volume).step_by(100_000_000) {
            store.set(index, 1);
            store.maybe_pack(volume, 2).unwrap();
        }
        assert!(store.is_sparse());
        assert_eq!(store.count_non_zero(), 10);
    }
}
