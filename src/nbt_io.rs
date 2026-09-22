//! NBT binary IO (gzip + uncompressed).
//!
//! Besides the tree-based helpers (which need a complete [`Compound`]), this
//! module exposes a streaming writer. It emits byte-identical NBT but lets the
//! schematic save path hand large arrays (region `BlockStates`) straight to the
//! gzip writer instead of materialising them as a `Value::LongArray` copy.

use std::io::{Read, Write};

use flate2::read::GzDecoder;
use flate2::write::GzEncoder;
use flate2::Compression;
use valence_nbt::binary::ToModifiedUtf8;
use valence_nbt::{Compound, List, Value};

use crate::error::{Error, Result};

/// NBT tag ids, matching Java's `Tag` constants and `valence_nbt::Tag` order.
pub const TAG_END: u8 = 0;
pub const TAG_BYTE: u8 = 1;
pub const TAG_SHORT: u8 = 2;
pub const TAG_INT: u8 = 3;
pub const TAG_LONG: u8 = 4;
pub const TAG_FLOAT: u8 = 5;
pub const TAG_DOUBLE: u8 = 6;
pub const TAG_BYTE_ARRAY: u8 = 7;
pub const TAG_STRING: u8 = 8;
pub const TAG_LIST: u8 = 9;
pub const TAG_COMPOUND: u8 = 10;
pub const TAG_INT_ARRAY: u8 = 11;
pub const TAG_LONG_ARRAY: u8 = 12;

pub fn read_gzip_nbt(bytes: &[u8]) -> Result<Compound<String>> {
    let mut decoder = GzDecoder::new(bytes);
    let mut buf = Vec::new();
    decoder.read_to_end(&mut buf)?;
    let mut slice = buf.as_slice();
    let (root, _) = valence_nbt::from_binary(&mut slice)?;
    Ok(root)
}

pub fn write_gzip_nbt(compound: &Compound<String>, level: Compression) -> Result<Vec<u8>> {
    let mut raw = Vec::new();
    valence_nbt::to_binary(compound, &mut raw, "")?;
    let mut enc = GzEncoder::new(Vec::new(), level);
    enc.write_all(&raw)?;
    Ok(enc.finish()?)
}

pub fn write_nbt_uncompressed(compound: &Compound<String>) -> Result<Vec<u8>> {
    let mut raw = Vec::new();
    valence_nbt::to_binary(compound, &mut raw, "")?;
    Ok(raw)
}

// ---------------------------------------------------------------------------
// Streaming writer
// ---------------------------------------------------------------------------

fn length_error(kind: &str, len: usize) -> Error {
    Error::Value(format!(
        "{kind} of length {len} exceeds maximum of i32::MAX"
    ))
}

/// Writes a bare tag id.
pub fn write_tag<W: Write + ?Sized>(writer: &mut W, tag: u8) -> Result<()> {
    writer.write_all(&[tag])?;
    Ok(())
}

/// Writes a name: `u16` length + Java modified UTF-8 bytes.
pub fn write_raw_string<W: Write + ?Sized>(writer: &mut W, value: &str) -> Result<()> {
    let len = value.modified_uf8_len();
    let len = u16::try_from(len).map_err(|_| {
        Error::Value(format!(
            "string of length {len} exceeds maximum of u16::MAX"
        ))
    })?;
    writer.write_all(&len.to_be_bytes())?;
    value.to_modified_utf8(len as usize, &mut *writer)?;
    Ok(())
}

/// Terminates a compound payload.
pub fn write_end<W: Write + ?Sized>(writer: &mut W) -> Result<()> {
    write_tag(writer, TAG_END)
}

fn write_count<W: Write + ?Sized>(writer: &mut W, len: usize, kind: &str) -> Result<()> {
    let len = i32::try_from(len).map_err(|_| length_error(kind, len))?;
    writer.write_all(&len.to_be_bytes())?;
    Ok(())
}

/// Writes a primitive tag with a name.
pub fn write_named_int<W: Write + ?Sized>(writer: &mut W, name: &str, value: i32) -> Result<()> {
    write_tag(writer, TAG_INT)?;
    write_raw_string(writer, name)?;
    writer.write_all(&value.to_be_bytes())?;
    Ok(())
}

pub fn write_named_long<W: Write + ?Sized>(writer: &mut W, name: &str, value: i64) -> Result<()> {
    write_tag(writer, TAG_LONG)?;
    write_raw_string(writer, name)?;
    writer.write_all(&value.to_be_bytes())?;
    Ok(())
}

/// Writes any named [`Value`], matching `valence_nbt::to_binary` byte for byte.
pub fn write_named_value<W: Write + ?Sized>(
    writer: &mut W,
    name: &str,
    value: &Value<String>,
) -> Result<()> {
    write_tag(writer, value.tag() as u8)?;
    write_raw_string(writer, name)?;
    write_value_payload(writer, value)
}

pub fn write_compound_payload<W: Write + ?Sized>(
    writer: &mut W,
    compound: &Compound<String>,
) -> Result<()> {
    for (key, value) in compound.iter() {
        write_tag(writer, value.tag() as u8)?;
        write_raw_string(writer, key)?;
        write_value_payload(writer, value)?;
    }
    write_end(writer)
}

/// Writes a named compound (tag header + payload).
pub fn write_compound<W: Write + ?Sized>(
    writer: &mut W,
    name: &str,
    compound: &Compound<String>,
) -> Result<()> {
    write_tag(writer, TAG_COMPOUND)?;
    write_raw_string(writer, name)?;
    write_compound_payload(writer, compound)
}

/// Writes a complete NBT document with a named root compound.
pub fn write_root_compound<W: Write + ?Sized>(
    writer: &mut W,
    name: &str,
    compound: &Compound<String>,
) -> Result<()> {
    write_compound(writer, name, compound)
}

pub fn write_value_payload<W: Write + ?Sized>(writer: &mut W, value: &Value<String>) -> Result<()> {
    match value {
        Value::Byte(v) => writer.write_all(&[*v as u8])?,
        Value::Short(v) => writer.write_all(&v.to_be_bytes())?,
        Value::Int(v) => writer.write_all(&v.to_be_bytes())?,
        Value::Long(v) => writer.write_all(&v.to_be_bytes())?,
        Value::Float(v) => writer.write_all(&v.to_be_bytes())?,
        Value::Double(v) => writer.write_all(&v.to_be_bytes())?,
        Value::ByteArray(bytes) => {
            write_count(writer, bytes.len(), "byte array")?;
            for byte in bytes {
                writer.write_all(&[*byte as u8])?;
            }
        }
        Value::String(text) => write_raw_string(writer, text)?,
        Value::List(list) => write_list_payload(writer, list)?,
        Value::Compound(compound) => write_compound_payload(writer, compound)?,
        Value::IntArray(array) => {
            write_count(writer, array.len(), "int array")?;
            for item in array {
                writer.write_all(&item.to_be_bytes())?;
            }
        }
        Value::LongArray(array) => {
            write_count(writer, array.len(), "long array")?;
            for item in array {
                writer.write_all(&item.to_be_bytes())?;
            }
        }
    }
    Ok(())
}

fn write_list_payload<W: Write + ?Sized>(writer: &mut W, list: &List<String>) -> Result<()> {
    match list {
        List::End => {
            write_tag(writer, TAG_END)?;
            writer.write_all(&0i32.to_be_bytes())?;
        }
        List::Byte(items) => {
            write_tag(writer, TAG_BYTE)?;
            write_count(writer, items.len(), "byte list")?;
            for item in items {
                writer.write_all(&[*item as u8])?;
            }
        }
        List::Short(items) => {
            write_tag(writer, TAG_SHORT)?;
            write_count(writer, items.len(), "short list")?;
            for item in items {
                writer.write_all(&item.to_be_bytes())?;
            }
        }
        List::Int(items) => {
            write_tag(writer, TAG_INT)?;
            write_count(writer, items.len(), "int list")?;
            for item in items {
                writer.write_all(&item.to_be_bytes())?;
            }
        }
        List::Long(items) => {
            write_tag(writer, TAG_LONG)?;
            write_count(writer, items.len(), "long list")?;
            for item in items {
                writer.write_all(&item.to_be_bytes())?;
            }
        }
        List::Float(items) => {
            write_tag(writer, TAG_FLOAT)?;
            write_count(writer, items.len(), "float list")?;
            for item in items {
                writer.write_all(&item.to_be_bytes())?;
            }
        }
        List::Double(items) => {
            write_tag(writer, TAG_DOUBLE)?;
            write_count(writer, items.len(), "double list")?;
            for item in items {
                writer.write_all(&item.to_be_bytes())?;
            }
        }
        List::ByteArray(items) => {
            write_tag(writer, TAG_BYTE_ARRAY)?;
            write_count(writer, items.len(), "byte array list")?;
            for item in items {
                write_value_payload(writer, &Value::ByteArray(item.clone()))?;
            }
        }
        List::String(items) => {
            write_tag(writer, TAG_STRING)?;
            write_count(writer, items.len(), "string list")?;
            for item in items {
                write_raw_string(writer, item)?;
            }
        }
        List::List(items) => {
            write_tag(writer, TAG_LIST)?;
            write_count(writer, items.len(), "list of lists")?;
            for item in items {
                write_list_payload(writer, item)?;
            }
        }
        List::Compound(items) => {
            write_tag(writer, TAG_COMPOUND)?;
            write_count(writer, items.len(), "compound list")?;
            for item in items {
                write_compound_payload(writer, item)?;
            }
        }
        List::IntArray(items) => {
            write_tag(writer, TAG_INT_ARRAY)?;
            write_count(writer, items.len(), "int array list")?;
            for item in items {
                write_value_payload(writer, &Value::IntArray(item.clone()))?;
            }
        }
        List::LongArray(items) => {
            write_tag(writer, TAG_LONG_ARRAY)?;
            write_count(writer, items.len(), "long array list")?;
            for item in items {
                write_value_payload(writer, &Value::LongArray(item.clone()))?;
            }
        }
    }
    Ok(())
}
