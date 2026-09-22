//! Litematica packed long array (matches `litemapy.storage.LitematicaBitArray`).

use valence_nbt::Value;

use crate::error::{Error, Result};

const MASK_64: u64 = u64::MAX;

/// Widest palette index supported (indices are handed to JS as `u32`).
pub const MAX_NBITS: u32 = 32;

/// Number of 64-bit words needed to pack `size` values of `nbits` bits.
///
/// Returns `None` when the result cannot be represented on the target
/// (`usize` is 32-bit inside wasm32, so large regions must be rejected
/// explicitly instead of wrapping around and trapping later).
pub fn words_required(size: usize, nbits: u32) -> Option<usize> {
    let nbits = nbits as usize;
    // `size * nbits` can exceed 32 bits for large regions, so split the product:
    // size * nbits = (size / 64) * nbits * 64 + (size % 64) * nbits.
    let whole = (size / 64).checked_mul(nbits)?;
    let remainder = (size % 64) * nbits;
    Some(whole + remainder / 64 + if remainder % 64 == 0 { 0 } else { 1 })
}

/// Pack size (in bits) of the `index`-th value.
fn bit_range(index: usize, nbits: u32) -> Option<(usize, u32)> {
    // Bit offsets need 64-bit math: 5e8 cells x 9 bits already exceeds 2^32.
    let start = (index as u64).checked_mul(nbits as u64)?;
    let word = usize::try_from(start >> 6).ok()?;
    Some((word, (start & 0x3F) as u32))
}

/// Last word touched by the `index`-th value.
fn end_word(index: usize, nbits: u32) -> Option<usize> {
    let end = (index as u64 + 1).checked_mul(nbits as u64)? - 1;
    usize::try_from(end >> 6).ok()
}

fn mask_for(nbits: u32) -> Result<u64> {
    if nbits == 0 || nbits > MAX_NBITS {
        return Err(Error::Value(format!(
            "unsupported bit width {nbits} (expected 1..={MAX_NBITS})"
        )));
    }
    Ok((1u64 << nbits) - 1)
}

#[derive(Clone, Debug)]
pub struct LitematicaBitArray {
    pub size: usize,
    pub nbits: u32,
    array: Vec<u64>,
    mask: u64,
}

impl LitematicaBitArray {
    /// Panicking constructor kept for API compatibility — prefer [`Self::try_new`].
    pub fn new(size: usize, nbits: u32) -> Self {
        Self::try_new(size, nbits).expect("invalid bit array dimensions")
    }

    pub fn try_new(size: usize, nbits: u32) -> Result<Self> {
        let mask = mask_for(nbits)?;
        let words = words_required(size, nbits).ok_or_else(|| {
            Error::Value(format!(
                "bit array of {size} values x {nbits} bits does not fit in memory"
            ))
        })?;
        Ok(Self {
            size,
            nbits,
            array: vec![0u64; words],
            mask,
        })
    }

    /// Adopt already-packed words (no re-packing, no per-cell copy).
    pub fn from_words(words: Vec<u64>, size: usize, nbits: u32) -> Result<Self> {
        let mask = mask_for(nbits)?;
        let expected = words_required(size, nbits).ok_or_else(|| {
            Error::Value(format!(
                "bit array of {size} values x {nbits} bits does not fit in memory"
            ))
        })?;
        if expected != words.len() {
            return Err(Error::Corrupted(format!(
                "long array length does not match bit array size and nbits, expected {}, not {}",
                expected,
                words.len()
            )));
        }
        Ok(Self {
            size,
            nbits,
            array: words,
            mask,
        })
    }

    pub fn size(&self) -> usize {
        self.size
    }

    pub fn nbits(&self) -> u32 {
        self.nbits
    }

    /// Raw packed words, exactly as stored in the `.litematic` `BlockStates` array.
    pub fn words(&self) -> &[u64] {
        &self.array
    }

    pub fn into_words(self) -> Vec<u64> {
        self.array
    }

    /// Value at `index` without bounds checks (0 when out of range).
    pub fn get_or_zero(&self, index: usize) -> u32 {
        if index >= self.size {
            return 0;
        }
        self.read_bits(self.nbits, self.mask, index) as u32
    }

    fn read_bits(&self, nbits: u32, mask: u64, index: usize) -> u64 {
        let Some((start_arr_index, start_bit_offset)) = bit_range(index, nbits) else {
            return 0;
        };
        let Some(end_arr_index) = end_word(index, nbits) else {
            return 0;
        };
        if start_arr_index >= self.array.len() || end_arr_index >= self.array.len() {
            return 0;
        }
        if start_arr_index == end_arr_index {
            (self.array[start_arr_index] >> start_bit_offset) & mask
        } else {
            let end_offset = 64 - start_bit_offset;
            let val = self.array[start_arr_index] >> start_bit_offset
                | self.array[end_arr_index] << end_offset;
            val & mask
        }
    }

    fn write_bits(&mut self, nbits: u32, mask: u64, index: usize, value: u64) {
        let Some((start_arr_index, start_bit_offset)) = bit_range(index, nbits) else {
            return;
        };
        let Some(end_arr_index) = end_word(index, nbits) else {
            return;
        };
        if start_arr_index >= self.array.len() || end_arr_index >= self.array.len() {
            return;
        }
        let zeroed = self.array[start_arr_index] & !(mask.wrapping_shl(start_bit_offset));
        self.array[start_arr_index] = (zeroed | (value << start_bit_offset)) & MASK_64;
        if start_arr_index != end_arr_index {
            let end_offset = 64 - start_bit_offset;
            let j1 = nbits - end_offset;
            self.array[end_arr_index] =
                (self.array[end_arr_index] >> j1 << j1 | (value & mask) >> end_offset) & MASK_64;
        }
    }

    /// Re-pack in place to a different bit width, reusing the existing buffer.
    ///
    /// Growing walks backwards and shrinking walks forwards so a value is always
    /// read before its slot is overwritten: the operation never needs a second
    /// full-size array.
    pub fn repack(&mut self, new_nbits: u32) -> Result<()> {
        if new_nbits == self.nbits {
            return Ok(());
        }
        let new_mask = mask_for(new_nbits)?;
        let new_words = words_required(self.size, new_nbits).ok_or_else(|| {
            Error::Value(format!(
                "bit array of {} values x {new_nbits} bits does not fit in memory",
                self.size
            ))
        })?;
        let old_nbits = self.nbits;
        let old_mask = self.mask;

        if new_nbits > old_nbits {
            self.array.resize(new_words, 0);
            for index in (0..self.size).rev() {
                let value = self.read_bits(old_nbits, old_mask, index);
                self.write_bits(new_nbits, new_mask, index, value);
            }
        } else {
            for index in 0..self.size {
                let value = self.read_bits(old_nbits, old_mask, index);
                self.write_bits(new_nbits, new_mask, index, value);
            }
            self.array.truncate(new_words);
        }

        self.nbits = new_nbits;
        self.mask = new_mask;
        self.clear_unused_bits();
        Ok(())
    }

    /// Zero the padding bits of the final word so re-packs stay byte-stable.
    fn clear_unused_bits(&mut self) {
        let Some(last) = self.array.last_mut() else {
            return;
        };
        let used = ((self.size as u64 * self.nbits as u64) % 64) as u32;
        if used != 0 {
            *last &= (1u64 << used) - 1;
        }
    }

    pub fn from_nbt_long_array(arr: &[i64], size: usize, nbits: u32) -> Result<Self> {
        let expected_len = words_required(size, nbits).ok_or_else(|| {
            Error::Value(format!(
                "bit array of {size} values x {nbits} bits does not fit in memory"
            ))
        })?;
        if expected_len != arr.len() {
            return Err(Error::Corrupted(format!(
                "long array length does not match bit array size and nbits, expected {}, not {}",
                expected_len,
                arr.len()
            )));
        }
        let mask = mask_for(nbits)?;
        let mut r = Self {
            size,
            nbits,
            array: Vec::with_capacity(arr.len()),
            mask,
        };
        for i in arr {
            r.array.push(*i as u64 & MASK_64);
        }
        Ok(r)
    }

    fn to_long_list(&self) -> Vec<i64> {
        let m1 = 1i64 << 63;
        let m2 = (1i128 << 64) - 1;
        self.array
            .iter()
            .map(|&i| {
                let mut i = i as i128;
                if (i & (m1 as i128)) != 0 {
                    i |= !m2;
                }
                i as i64
            })
            .collect()
    }

    pub fn to_nbt_value(&self) -> Value<String> {
        Value::LongArray(self.to_long_list())
    }

    pub fn get(&self, index: usize) -> Result<u32> {
        if index >= self.size {
            return Err(Error::Value(format!("Invalid index {}", index)));
        }
        Ok(self.read_bits(self.nbits, self.mask, index) as u32)
    }

    pub fn set(&mut self, index: usize, value: u32) -> Result<()> {
        if index >= self.size {
            return Err(Error::Value(format!("Invalid index {}", index)));
        }
        let v = value as u64;
        if v > self.mask {
            return Err(Error::Value(format!(
                "Invalid value {}, maximum value is {}",
                value, self.mask
            )));
        }
        let (nbits, mask) = (self.nbits, self.mask);
        self.write_bits(nbits, mask, index, v);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn repack_preserves_values_in_both_directions() {
        let size = 1000usize;
        let widths = [2u32, 3, 5, 8, 11, 16];
        for from in widths {
            for to in widths {
                let mut array = LitematicaBitArray::try_new(size, from).unwrap();
                // Values must fit whichever side is narrower, which is the invariant
                // callers rely on (a shrink only follows a deduplicated palette).
                let mask = (1u64 << from.min(to)) - 1;
                for index in 0..size {
                    array
                        .set(index, ((index as u64 * 2654435761) & mask) as u32)
                        .unwrap();
                }
                let expected: Vec<u32> = (0..size).map(|i| array.get(i).unwrap()).collect();

                array.repack(to).unwrap();
                assert_eq!(array.nbits(), to);
                assert_eq!(array.words().len(), words_required(size, to).unwrap());
                for index in 0..size {
                    assert_eq!(
                        array.get(index).unwrap(),
                        expected[index],
                        "{from}->{to} @{index}"
                    );
                }
            }
        }
    }

    #[test]
    fn repack_clears_padding_bits() {
        let size = 100usize;
        let mut array = LitematicaBitArray::try_new(size, 8).unwrap();
        for index in 0..size {
            array.set(index, 0xff).unwrap();
        }
        array.repack(2).unwrap();
        let used = (size * 2) % 64;
        let last = *array.words().last().unwrap();
        assert_eq!(
            last & !((1u64 << used) - 1),
            0,
            "padding bits must stay zero"
        );
    }

    #[test]
    fn word_counts_are_exact_for_huge_inputs() {
        // Splitting the product keeps this exact instead of silently wrapping on wasm32.
        assert_eq!(words_required(1 << 40, 8), Some(1 << 37));
        // 4_294_967_295 cells x 9 bits = 38_654_705_655 bits -> 603_979_776 longs.
        assert_eq!(words_required(4_294_967_295, 9), Some(603_979_776));
    }

    #[test]
    fn unsupported_bit_widths_are_rejected() {
        assert!(LitematicaBitArray::try_new(64, 33).is_err());
        assert!(LitematicaBitArray::try_new(64, 0).is_err());
    }

    #[test]
    fn word_counts_stay_exact_when_the_bit_total_exceeds_32_bits() {
        // 5.6e9 bits: an 8x6 3D map art at 800 cells tall.
        assert_eq!(words_required(629_145_600, 9), Some(88_473_600));
        assert_eq!(words_required(366_477_312, 9), Some(51_535_872));
        assert_eq!(words_required(64, 2), Some(2));
        assert_eq!(words_required(65, 2), Some(3));
    }
}
