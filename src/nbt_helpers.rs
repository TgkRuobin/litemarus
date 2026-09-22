//! Small helpers for reading `valence_nbt::Value`.

use valence_nbt::{Compound, List, Value};

use crate::error::{Error, Result};

pub fn expect_compound<'a>(
    v: &'a Value<String>,
    ctx: &'static str,
    key: &str,
) -> Result<&'a Compound<String>> {
    match v {
        Value::Compound(c) => Ok(c),
        other => Err(Error::UnexpectedTag {
            context: ctx,
            expected: key.into(),
            got: other.tag(),
        }),
    }
}

pub fn expect_list_compound<'a>(
    v: &'a Value<String>,
    ctx: &'static str,
) -> Result<&'a Vec<Compound<String>>> {
    match v {
        Value::List(List::Compound(v)) => Ok(v),
        other => Err(Error::UnexpectedTag {
            context: ctx,
            expected: "List<Compound>".into(),
            got: other.tag(),
        }),
    }
}

pub fn expect_i32(v: &Value<String>, ctx: &'static str) -> Result<i32> {
    match v {
        Value::Byte(b) => Ok(*b as i32),
        Value::Short(s) => Ok(*s as i32),
        Value::Int(i) => Ok(*i),
        Value::Long(l) => Ok(*l as i32),
        other => Err(Error::UnexpectedTag {
            context: ctx,
            expected: "int-like".into(),
            got: other.tag(),
        }),
    }
}

pub fn expect_i64(v: &Value<String>, ctx: &'static str) -> Result<i64> {
    match v {
        Value::Byte(b) => Ok(*b as i64),
        Value::Short(s) => Ok(*s as i64),
        Value::Int(i) => Ok(*i as i64),
        Value::Long(l) => Ok(*l),
        other => Err(Error::UnexpectedTag {
            context: ctx,
            expected: "long-like".into(),
            got: other.tag(),
        }),
    }
}

pub fn expect_string(v: &Value<String>, ctx: &'static str) -> Result<String> {
    match v {
        Value::String(s) => Ok(s.clone()),
        other => Err(Error::UnexpectedTag {
            context: ctx,
            expected: "String".into(),
            got: other.tag(),
        }),
    }
}

pub fn expect_long_array<'a>(v: &'a Value<String>, ctx: &'static str) -> Result<&'a [i64]> {
    match v {
        Value::LongArray(a) => Ok(a.as_slice()),
        other => Err(Error::UnexpectedTag {
            context: ctx,
            expected: "LongArray".into(),
            got: other.tag(),
        }),
    }
}

pub fn expect_int_array<'a>(v: &'a Value<String>, ctx: &'static str) -> Result<&'a [i32]> {
    match v {
        Value::IntArray(a) => Ok(a.as_slice()),
        other => Err(Error::UnexpectedTag {
            context: ctx,
            expected: "IntArray".into(),
            got: other.tag(),
        }),
    }
}

pub fn expect_byte_array(v: &Value<String>, ctx: &'static str) -> Result<Vec<u8>> {
    match v {
        Value::ByteArray(a) => Ok(a.iter().map(|x| *x as u8).collect()),
        other => Err(Error::UnexpectedTag {
            context: ctx,
            expected: "ByteArray".into(),
            got: other.tag(),
        }),
    }
}

pub fn required<'a>(
    compound: &'a Compound<String>,
    ctx: &'static str,
    key: &str,
) -> Result<&'a Value<String>> {
    compound.get(key).ok_or_else(|| Error::MissingField {
        context: ctx,
        key: key.into(),
    })
}
