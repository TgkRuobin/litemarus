//! **LitemaRust** (`litemarus`) — Rust reimplementation of [litemapy](https://github.com/SmylerMC/litemapy)
//! for reading and writing Litematica `.litematic` files on native targets and WASM.

pub mod bit_array;
mod block_store;
pub mod boxes;
pub mod constants;
pub mod discriminating_dictionary;
pub mod error;
pub mod minecraft;
pub mod nbt_helpers;
pub mod nbt_io;
pub mod region;
pub mod regions_map;
pub mod schematic;

#[cfg(feature = "wasm")]
mod wasm;

pub use bit_array::LitematicaBitArray;
pub use boxes::{block_is_in_box, box_is_in_box};
pub use constants::{
    DEFAULT_NAME, LITEMAPY_NAME, LITEMAPY_VERSION, LITEMATIC_SUBVERSION, LITEMATIC_VERSION,
    MC_DATA_VERSION, SPONGE_VERSION,
};
pub use discriminating_dictionary::DiscriminatingDictionary;
pub use error::{
    CorruptedSchematicError, DiscriminationError, Error, InvalidIdentifier,
    RequiredKeyMissingException, Result,
};
pub use minecraft::{assert_valid_identifier, is_valid_identifier, BlockState, Entity, TileEntity};
pub use region::{Region, AIR};
pub use regions_map::RegionsMap;
pub use schematic::{schematic_from_single_region, SaveMeta, Schematic};

#[cfg(feature = "wasm")]
pub use wasm::{
    wasm_build_demo_litematic, wasm_decode_litematic_metadata, wasm_litemarus_version, WasmRegion,
    WasmSchematic,
};
